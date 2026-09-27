#ifndef GATT_PROFILE_H_
#define GATT_PROFILE_H_

#include <zephyr/bluetooth/uuid.h>

/*
 * btleplug Test GATT Profile
 * Base UUID: XXXXXXXX-b5a3-f393-e0a9-e50e24dcca9e
 */

/* --- Control Service (0x00000001-...) --- */
#define BT_UUID_CONTROL_SERVICE \
    BT_UUID_DECLARE_128(BT_UUID_128_ENCODE(0x00000001, 0xb5a3, 0xf393, 0xe0a9, 0xe50e24dcca9e))
#define BT_UUID_CONTROL_POINT \
    BT_UUID_DECLARE_128(BT_UUID_128_ENCODE(0x00000101, 0xb5a3, 0xf393, 0xe0a9, 0xe50e24dcca9e))
#define BT_UUID_CONTROL_RESPONSE \
    BT_UUID_DECLARE_128(BT_UUID_128_ENCODE(0x00000102, 0xb5a3, 0xf393, 0xe0a9, 0xe50e24dcca9e))

/* --- Read/Write Test Service (0x00000002-...) --- */
#define BT_UUID_RW_SERVICE \
    BT_UUID_DECLARE_128(BT_UUID_128_ENCODE(0x00000002, 0xb5a3, 0xf393, 0xe0a9, 0xe50e24dcca9e))
#define BT_UUID_STATIC_READ \
    BT_UUID_DECLARE_128(BT_UUID_128_ENCODE(0x00000201, 0xb5a3, 0xf393, 0xe0a9, 0xe50e24dcca9e))
#define BT_UUID_COUNTER_READ \
    BT_UUID_DECLARE_128(BT_UUID_128_ENCODE(0x00000202, 0xb5a3, 0xf393, 0xe0a9, 0xe50e24dcca9e))
#define BT_UUID_WRITE_WITH_RESP \
    BT_UUID_DECLARE_128(BT_UUID_128_ENCODE(0x00000203, 0xb5a3, 0xf393, 0xe0a9, 0xe50e24dcca9e))
#define BT_UUID_WRITE_WITHOUT_RESP \
    BT_UUID_DECLARE_128(BT_UUID_128_ENCODE(0x00000204, 0xb5a3, 0xf393, 0xe0a9, 0xe50e24dcca9e))
#define BT_UUID_READ_WRITE \
    BT_UUID_DECLARE_128(BT_UUID_128_ENCODE(0x00000205, 0xb5a3, 0xf393, 0xe0a9, 0xe50e24dcca9e))
#define BT_UUID_LONG_VALUE \
    BT_UUID_DECLARE_128(BT_UUID_128_ENCODE(0x00000206, 0xb5a3, 0xf393, 0xe0a9, 0xe50e24dcca9e))
#define BT_UUID_ERROR_CHAR \
    BT_UUID_DECLARE_128(BT_UUID_128_ENCODE(0x00000207, 0xb5a3, 0xf393, 0xe0a9, 0xe50e24dcca9e))
#define BT_UUID_WRITE_LOG_CHAR \
    BT_UUID_DECLARE_128(BT_UUID_128_ENCODE(0x00000208, 0xb5a3, 0xf393, 0xe0a9, 0xe50e24dcca9e))

/* --- Notification Test Service (0x00000003-...) --- */
#define BT_UUID_NOTIFY_SERVICE \
    BT_UUID_DECLARE_128(BT_UUID_128_ENCODE(0x00000003, 0xb5a3, 0xf393, 0xe0a9, 0xe50e24dcca9e))
#define BT_UUID_NOTIFY_CHAR \
    BT_UUID_DECLARE_128(BT_UUID_128_ENCODE(0x00000301, 0xb5a3, 0xf393, 0xe0a9, 0xe50e24dcca9e))
#define BT_UUID_INDICATE_CHAR \
    BT_UUID_DECLARE_128(BT_UUID_128_ENCODE(0x00000302, 0xb5a3, 0xf393, 0xe0a9, 0xe50e24dcca9e))
#define BT_UUID_CONFIGURABLE_NOTIFY \
    BT_UUID_DECLARE_128(BT_UUID_128_ENCODE(0x00000303, 0xb5a3, 0xf393, 0xe0a9, 0xe50e24dcca9e))
#define BT_UUID_REFUSED_NOTIFY_CHAR \
    BT_UUID_DECLARE_128(BT_UUID_128_ENCODE(0x00000304, 0xb5a3, 0xf393, 0xe0a9, 0xe50e24dcca9e))

/* --- Descriptor Test Service (0x00000004-...) --- */
#define BT_UUID_DESCRIPTOR_SERVICE \
    BT_UUID_DECLARE_128(BT_UUID_128_ENCODE(0x00000004, 0xb5a3, 0xf393, 0xe0a9, 0xe50e24dcca9e))
#define BT_UUID_DESCRIPTOR_TEST_CHAR \
    BT_UUID_DECLARE_128(BT_UUID_128_ENCODE(0x00000401, 0xb5a3, 0xf393, 0xe0a9, 0xe50e24dcca9e))
#define BT_UUID_READ_ONLY_DESCRIPTOR \
    BT_UUID_DECLARE_128(BT_UUID_128_ENCODE(0x000004A1, 0xb5a3, 0xf393, 0xe0a9, 0xe50e24dcca9e))
#define BT_UUID_RW_DESCRIPTOR \
    BT_UUID_DECLARE_128(BT_UUID_128_ENCODE(0x000004A2, 0xb5a3, 0xf393, 0xe0a9, 0xe50e24dcca9e))

/* --- Control Point Opcodes --- */
#define CMD_START_NOTIFICATIONS  0x01
#define CMD_STOP_NOTIFICATIONS   0x02
#define CMD_TRIGGER_DISCONNECT   0x03
#define CMD_CHANGE_ADVERTISEMENTS 0x04
#define CMD_RESET_STATE          0x05
#define CMD_SET_NOTIFICATION_PAYLOAD 0x06

/* --- Test Constants --- */
#define STATIC_READ_VALUE        {0x01, 0x02, 0x03, 0x04}
#define LONG_VALUE_SIZE          512
#define MANUFACTURER_COMPANY_ID  0xFFFF
#define NOTIFICATION_INTERVAL_MS 1000

/* Application error (0x80-0x9F); auth/encryption errors would trigger pairing. */
#define ERROR_CHAR_ATT_ERROR 0x80

/* --- Shared State --- */

/** Peripheral state accessible from all modules. */
struct peripheral_state {
    struct bt_conn *conn;
    bool notify_enabled;
    bool indicate_enabled;
    bool configurable_notify_enabled;
    uint32_t read_counter;
    uint8_t rw_value[256];
    uint16_t rw_value_len;
    uint8_t long_value[LONG_VALUE_SIZE];
    uint16_t long_value_len;
    uint8_t write_with_resp_value[256];
    uint16_t write_with_resp_len;
    uint8_t notify_payload[20];
    uint16_t notify_payload_len;
    uint8_t rw_descriptor_value[256];
    uint16_t rw_descriptor_len;
    /* Write-without-response ordering log (#464); see WRITE_LOG_CHAR. */
    uint16_t wwr_count;
    uint8_t wwr_last_seq;
    uint8_t wwr_out_of_order;
};

extern struct peripheral_state g_state;

/* --- Functions shared across modules --- */
void control_service_init(void);
void control_handle_command(const uint8_t *data, uint16_t len);
void start_periodic_notifications(void);
void stop_periodic_notifications(void);
void reset_peripheral_state(void);

#endif /* GATT_PROFILE_H_ */
