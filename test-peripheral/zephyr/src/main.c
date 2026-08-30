#include <zephyr/kernel.h>
#include <zephyr/bluetooth/bluetooth.h>
#include <zephyr/bluetooth/conn.h>
#include <zephyr/bluetooth/gatt.h>
#include <zephyr/bluetooth/gap.h>

#include "gatt_profile.h"

#include <zephyr/logging/log.h>
LOG_MODULE_REGISTER(main, LOG_LEVEL_INF);

/* Advertisement data:
 * - Flags: General Discoverable + BR/EDR Not Supported
 * - Complete device name: "btleplug-test"
 * - GAP Appearance: Generic Heart Rate Sensor (0x0340)
 * - TX Power Level: 0 dBm
 */
static const struct bt_data ad[] = {
	BT_DATA_BYTES(BT_DATA_FLAGS, (BT_LE_AD_GENERAL | BT_LE_AD_NO_BREDR)),
	BT_DATA(BT_DATA_NAME_COMPLETE, CONFIG_BT_DEVICE_NAME,
		sizeof(CONFIG_BT_DEVICE_NAME) - 1),
	BT_DATA_BYTES(BT_DATA_GAP_APPEARANCE, 0x40, 0x03),
	BT_DATA_BYTES(BT_DATA_TX_POWER, 0x00),
};

/* Scan response data:
 * - 128-bit service UUID list (triggers ServicesAdvertisement in btleplug)
 * - Manufacturer data: Company ID 0xFFFF + [0xBB, 0xCC, 0x01]
 */
static const struct bt_data sd[] = {
	BT_DATA_BYTES(BT_DATA_UUID128_ALL,
		/* Control Service UUID (little-endian) */
		0x9e, 0xca, 0xdc, 0x24, 0x0e, 0xe5, 0xa9, 0xe0,
		0x93, 0xf3, 0xa3, 0xb5, 0x01, 0x00, 0x00, 0x00
	),
	BT_DATA_BYTES(BT_DATA_MANUFACTURER_DATA,
		0xFF, 0xFF,       /* Company ID 0xFFFF (little-endian) */
		0xBB, 0xCC, 0x01  /* "bt" + version */
	),
};

static void connected(struct bt_conn *conn, uint8_t err)
{
	if (err) {
		LOG_ERR("Connection failed (err 0x%02x)", err);
		return;
	}

	LOG_INF("Connected");
	g_state.conn = bt_conn_ref(conn);
}

static void restart_adv_work_handler(struct k_work *work);
static K_WORK_DELAYABLE_DEFINE(restart_adv_work, restart_adv_work_handler);

static void restart_adv_work_handler(struct k_work *work)
{
	/* Ensure advertising is stopped before restarting */
	bt_le_adv_stop();

	int err = bt_le_adv_start(BT_LE_ADV_CONN_FAST_1, ad, ARRAY_SIZE(ad),
				   sd, ARRAY_SIZE(sd));
	if (err) {
		LOG_ERR("Advertising restart failed (err %d)", err);
	} else {
		LOG_INF("Advertising restarted");
	}
}

static void disconnected(struct bt_conn *conn, uint8_t reason)
{
	LOG_INF("Disconnected (reason 0x%02x)", reason);

	if (g_state.conn) {
		bt_conn_unref(g_state.conn);
		g_state.conn = NULL;
	}

	stop_periodic_notifications();

	/* Defer advertising restart to system workqueue to avoid
	 * calling bt_le_adv_start from the connection callback context,
	 * which can fail on some Zephyr BLE controller configurations. */
	k_work_reschedule(&restart_adv_work, K_MSEC(100));
}

BT_CONN_CB_DEFINE(conn_callbacks) = {
	.connected = connected,
	.disconnected = disconnected,
};

int main(void)
{
	int err;

	LOG_INF("btleplug test peripheral starting");

	control_service_init();

	err = bt_enable(NULL);
	if (err) {
		LOG_ERR("Bluetooth init failed (err %d)", err);
		return 0;
	}

	LOG_INF("Bluetooth initialized");

	err = bt_le_adv_start(BT_LE_ADV_CONN_FAST_1, ad, ARRAY_SIZE(ad),
			       sd, ARRAY_SIZE(sd));
	if (err) {
		LOG_ERR("Advertising failed to start (err %d)", err);
		return 0;
	}

	LOG_INF("Advertising as '%s'", CONFIG_BT_DEVICE_NAME);

	/* Main thread sleeps — all work is done in callbacks and workqueue */
	while (1) {
		k_sleep(K_FOREVER);
	}

	return 0;
}
