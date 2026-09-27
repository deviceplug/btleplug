#include <zephyr/kernel.h>
#include <zephyr/bluetooth/bluetooth.h>
#include <zephyr/bluetooth/conn.h>
#include <zephyr/bluetooth/gatt.h>
#include <string.h>

#include "gatt_profile.h"

#include <zephyr/logging/log.h>
LOG_MODULE_REGISTER(control_service, LOG_LEVEL_INF);

/* Global peripheral state */
struct peripheral_state g_state;

/* Notification work item for periodic notifications */
static struct k_work_delayable notify_work;
static bool periodic_notify_active;

/* Reference to notify service attributes for bt_gatt_notify() */
extern struct bt_gatt_service_static notify_svc;

static void periodic_notify_handler(struct k_work *work)
{
	if (!periodic_notify_active || !g_state.conn) {
		return;
	}

	/*
	 * Attribute indices in notify_svc (BT_GATT_SERVICE_DEFINE layout):
	 *   [0] = Primary Service declaration
	 *   [1] = Notify Char declaration
	 *   [2] = Notify Char value  <-- use for bt_gatt_notify
	 *   [3] = Notify CCC
	 *   [4] = Indicate Char declaration
	 *   [5] = Indicate Char value  <-- use for bt_gatt_indicate
	 *   [6] = Indicate CCC
	 *   [7] = Configurable Notify declaration
	 *   [8] = Configurable Notify value  <-- use for bt_gatt_notify
	 *   [9] = Configurable Notify CCC
	 *   [10] = Refused Notify declaration
	 *   [11] = Refused Notify value (never notified -- CCC write always rejected)
	 *   [12] = Refused Notify CCC
	 */

	/* Send notification on Notify Char (attr index 2 = value attribute) */
	if (g_state.notify_enabled) {
		static uint8_t counter;
		uint8_t data[] = {counter++};
		bt_gatt_notify(g_state.conn, &notify_svc.attrs[2], data, sizeof(data));
	}

	/* Send indication on Indicate Char (attr index 5 = value attribute) */
	if (g_state.indicate_enabled) {
		static struct bt_gatt_indicate_params ind_params;
		static uint8_t ind_counter;
		uint8_t data[] = {ind_counter++};
		ind_params.attr = &notify_svc.attrs[5];
		ind_params.func = NULL;
		ind_params.destroy = NULL;
		ind_params.data = data;
		ind_params.len = sizeof(data);
		bt_gatt_indicate(g_state.conn, &ind_params);
	}

	/* Send configurable notify (attr index 8 = value attribute) */
	if (g_state.configurable_notify_enabled && g_state.notify_payload_len > 0) {
		bt_gatt_notify(g_state.conn, &notify_svc.attrs[8],
			       g_state.notify_payload, g_state.notify_payload_len);
	}

	/* Reschedule */
	k_work_reschedule(&notify_work, K_MSEC(NOTIFICATION_INTERVAL_MS));
}

void start_periodic_notifications(void)
{
	periodic_notify_active = true;
	k_work_reschedule(&notify_work, K_MSEC(NOTIFICATION_INTERVAL_MS));
	LOG_INF("Periodic notifications started");
}

void stop_periodic_notifications(void)
{
	periodic_notify_active = false;
	k_work_cancel_delayable(&notify_work);
	LOG_INF("Periodic notifications stopped");
}

void reset_peripheral_state(void)
{
	stop_periodic_notifications();
	g_state.read_counter = 0;
	g_state.rw_value_len = 0;
	g_state.long_value_len = 0;
	g_state.write_with_resp_len = 0;
	g_state.notify_payload_len = 0;
	g_state.rw_descriptor_len = 0;
	memset(g_state.rw_value, 0, sizeof(g_state.rw_value));
	memset(g_state.long_value, 0, sizeof(g_state.long_value));
	memset(g_state.write_with_resp_value, 0, sizeof(g_state.write_with_resp_value));
	memset(g_state.notify_payload, 0, sizeof(g_state.notify_payload));
	memset(g_state.rw_descriptor_value, 0, sizeof(g_state.rw_descriptor_value));
	LOG_INF("Peripheral state reset");
}

/* Disconnect work — delayed to allow the write response to be sent */
static struct k_work_delayable disconnect_work;

static void disconnect_handler(struct k_work *work)
{
	if (g_state.conn) {
		bt_conn_disconnect(g_state.conn, BT_HCI_ERR_REMOTE_USER_TERM_CONN);
	}
}

void control_handle_command(const uint8_t *data, uint16_t len)
{
	if (len < 1) {
		LOG_WRN("Empty control command");
		return;
	}

	uint8_t opcode = data[0];
	LOG_INF("Control command: 0x%02x", opcode);

	switch (opcode) {
	case CMD_START_NOTIFICATIONS:
		start_periodic_notifications();
		break;
	case CMD_STOP_NOTIFICATIONS:
		stop_periodic_notifications();
		break;
	case CMD_TRIGGER_DISCONNECT:
		k_work_reschedule(&disconnect_work, K_MSEC(500));
		break;
	case CMD_CHANGE_ADVERTISEMENTS:
		/* Deferred: advertisement rotation is not tested in the initial
		 * integration test suite. Implement when CentralEvent::ServiceDataAdvertisement
		 * tests are added. */
		LOG_INF("Change advertisements (deferred — not tested in initial suite)");
		break;
	case CMD_RESET_STATE:
		reset_peripheral_state();
		break;
	case CMD_SET_NOTIFICATION_PAYLOAD:
		if (len > 1) {
			uint16_t payload_len = len - 1;
			if (payload_len > sizeof(g_state.notify_payload)) {
				payload_len = sizeof(g_state.notify_payload);
			}
			memcpy(g_state.notify_payload, &data[1], payload_len);
			g_state.notify_payload_len = payload_len;
			LOG_INF("Notification payload set: %u bytes", payload_len);
		}
		break;
	default:
		LOG_WRN("Unknown control opcode: 0x%02x", opcode);
		break;
	}
}

/* GATT write handler for Control Point */
ssize_t write_control_point(struct bt_conn *conn,
			    const struct bt_gatt_attr *attr,
			    const void *buf, uint16_t len,
			    uint16_t offset, uint8_t flags)
{
	control_handle_command(buf, len);
	return len;
}

/* Control Response CCC changed */
void control_response_ccc_changed(const struct bt_gatt_attr *attr, uint16_t value)
{
	LOG_INF("Control Response CCC: 0x%04x", value);
}

void control_service_init(void)
{
	k_work_init_delayable(&notify_work, periodic_notify_handler);
	k_work_init_delayable(&disconnect_work, disconnect_handler);
	reset_peripheral_state();
}
