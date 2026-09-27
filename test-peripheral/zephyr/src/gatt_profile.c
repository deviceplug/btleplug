#include <zephyr/bluetooth/bluetooth.h>
#include <zephyr/bluetooth/gatt.h>

#include "gatt_profile.h"

/* Forward declarations for callbacks in test_handlers.c */
extern ssize_t read_static(struct bt_conn *, const struct bt_gatt_attr *,
                           void *, uint16_t, uint16_t);
extern ssize_t read_counter(struct bt_conn *, const struct bt_gatt_attr *,
                            void *, uint16_t, uint16_t);
extern ssize_t write_with_resp(struct bt_conn *, const struct bt_gatt_attr *,
                               const void *, uint16_t, uint16_t, uint8_t);
extern ssize_t write_without_resp(struct bt_conn *, const struct bt_gatt_attr *,
                                  const void *, uint16_t, uint16_t, uint8_t);
extern ssize_t read_rw(struct bt_conn *, const struct bt_gatt_attr *,
                       void *, uint16_t, uint16_t);
extern ssize_t write_rw(struct bt_conn *, const struct bt_gatt_attr *,
                        const void *, uint16_t, uint16_t, uint8_t);
extern ssize_t read_long_value(struct bt_conn *, const struct bt_gatt_attr *,
                               void *, uint16_t, uint16_t);
extern ssize_t write_long_value(struct bt_conn *, const struct bt_gatt_attr *,
                                const void *, uint16_t, uint16_t, uint8_t);
extern ssize_t read_error(struct bt_conn *, const struct bt_gatt_attr *,
                          void *, uint16_t, uint16_t);
extern ssize_t write_error(struct bt_conn *, const struct bt_gatt_attr *,
                           const void *, uint16_t, uint16_t, uint8_t);
extern ssize_t read_write_log(struct bt_conn *, const struct bt_gatt_attr *,
                              void *, uint16_t, uint16_t);
extern ssize_t read_descriptor_test_char(struct bt_conn *,
                                         const struct bt_gatt_attr *,
                                         void *, uint16_t, uint16_t);
extern ssize_t read_ro_descriptor(struct bt_conn *, const struct bt_gatt_attr *,
                                  void *, uint16_t, uint16_t);
extern ssize_t read_rw_descriptor(struct bt_conn *, const struct bt_gatt_attr *,
                                  void *, uint16_t, uint16_t);
extern ssize_t write_rw_descriptor(struct bt_conn *, const struct bt_gatt_attr *,
                                   const void *, uint16_t, uint16_t, uint8_t);
extern void notify_ccc_changed(const struct bt_gatt_attr *, uint16_t);
extern void indicate_ccc_changed(const struct bt_gatt_attr *, uint16_t);
extern void configurable_notify_ccc_changed(const struct bt_gatt_attr *, uint16_t);
extern ssize_t read_refused_ccc(struct bt_conn *, const struct bt_gatt_attr *,
                                void *, uint16_t, uint16_t);
extern ssize_t write_refused_ccc(struct bt_conn *, const struct bt_gatt_attr *,
                                 const void *, uint16_t, uint16_t, uint8_t);

/* Forward declaration for control point write handler */
extern ssize_t write_control_point(struct bt_conn *, const struct bt_gatt_attr *,
                                   const void *, uint16_t, uint16_t, uint8_t);
extern void control_response_ccc_changed(const struct bt_gatt_attr *, uint16_t);

/* ============================================================
 * Service 1: Control Service
 * ============================================================ */
BT_GATT_SERVICE_DEFINE(control_svc,
    BT_GATT_PRIMARY_SERVICE(BT_UUID_CONTROL_SERVICE),
    /* Control Point — Write */
    BT_GATT_CHARACTERISTIC(BT_UUID_CONTROL_POINT,
        BT_GATT_CHRC_WRITE,
        BT_GATT_PERM_WRITE,
        NULL, write_control_point, NULL),
    /* Control Response — Notify */
    BT_GATT_CHARACTERISTIC(BT_UUID_CONTROL_RESPONSE,
        BT_GATT_CHRC_NOTIFY,
        BT_GATT_PERM_NONE,
        NULL, NULL, NULL),
    BT_GATT_CCC(control_response_ccc_changed,
        BT_GATT_PERM_READ | BT_GATT_PERM_WRITE),
);

/* ============================================================
 * Service 2: Read/Write Test Service
 * ============================================================ */
BT_GATT_SERVICE_DEFINE(rw_svc,
    BT_GATT_PRIMARY_SERVICE(BT_UUID_RW_SERVICE),
    /* Static Read */
    BT_GATT_CHARACTERISTIC(BT_UUID_STATIC_READ,
        BT_GATT_CHRC_READ,
        BT_GATT_PERM_READ,
        read_static, NULL, NULL),
    /* Counter Read */
    BT_GATT_CHARACTERISTIC(BT_UUID_COUNTER_READ,
        BT_GATT_CHRC_READ,
        BT_GATT_PERM_READ,
        read_counter, NULL, NULL),
    /* Write With Response */
    BT_GATT_CHARACTERISTIC(BT_UUID_WRITE_WITH_RESP,
        BT_GATT_CHRC_WRITE,
        BT_GATT_PERM_WRITE,
        NULL, write_with_resp, NULL),
    /* Write Without Response */
    BT_GATT_CHARACTERISTIC(BT_UUID_WRITE_WITHOUT_RESP,
        BT_GATT_CHRC_WRITE_WITHOUT_RESP,
        BT_GATT_PERM_WRITE,
        NULL, write_without_resp, NULL),
    /* Read/Write */
    BT_GATT_CHARACTERISTIC(BT_UUID_READ_WRITE,
        BT_GATT_CHRC_READ | BT_GATT_CHRC_WRITE,
        BT_GATT_PERM_READ | BT_GATT_PERM_WRITE,
        read_rw, write_rw, NULL),
    /* Long Value (512 bytes) */
    BT_GATT_CHARACTERISTIC(BT_UUID_LONG_VALUE,
        BT_GATT_CHRC_READ | BT_GATT_CHRC_WRITE,
        BT_GATT_PERM_READ | BT_GATT_PERM_WRITE,
        read_long_value, write_long_value, NULL),
    /* Error Char -- read and write both always fail (#492) */
    BT_GATT_CHARACTERISTIC(BT_UUID_ERROR_CHAR,
        BT_GATT_CHRC_READ | BT_GATT_CHRC_WRITE,
        BT_GATT_PERM_READ | BT_GATT_PERM_WRITE,
        read_error, write_error, NULL),
    /* Write Log -- write-without-response ordering counters (#464) */
    BT_GATT_CHARACTERISTIC(BT_UUID_WRITE_LOG_CHAR,
        BT_GATT_CHRC_READ,
        BT_GATT_PERM_READ,
        read_write_log, NULL, NULL),
);

/* ============================================================
 * Service 3: Notification Test Service
 * ============================================================ */
BT_GATT_SERVICE_DEFINE(notify_svc,
    BT_GATT_PRIMARY_SERVICE(BT_UUID_NOTIFY_SERVICE),
    /* Notify Char */
    BT_GATT_CHARACTERISTIC(BT_UUID_NOTIFY_CHAR,
        BT_GATT_CHRC_NOTIFY,
        BT_GATT_PERM_NONE,
        NULL, NULL, NULL),
    BT_GATT_CCC(notify_ccc_changed,
        BT_GATT_PERM_READ | BT_GATT_PERM_WRITE),
    /* Indicate Char */
    BT_GATT_CHARACTERISTIC(BT_UUID_INDICATE_CHAR,
        BT_GATT_CHRC_INDICATE,
        BT_GATT_PERM_NONE,
        NULL, NULL, NULL),
    BT_GATT_CCC(indicate_ccc_changed,
        BT_GATT_PERM_READ | BT_GATT_PERM_WRITE),
    /* Configurable Notify */
    BT_GATT_CHARACTERISTIC(BT_UUID_CONFIGURABLE_NOTIFY,
        BT_GATT_CHRC_NOTIFY,
        BT_GATT_PERM_NONE,
        NULL, NULL, NULL),
    BT_GATT_CCC(configurable_notify_ccc_changed,
        BT_GATT_PERM_READ | BT_GATT_PERM_WRITE),
    /* Refused Notify -- unmanaged CCC whose write callback rejects every subscribe (#471) */
    BT_GATT_CHARACTERISTIC(BT_UUID_REFUSED_NOTIFY_CHAR,
        BT_GATT_CHRC_NOTIFY,
        BT_GATT_PERM_NONE,
        NULL, NULL, NULL),
    BT_GATT_DESCRIPTOR(BT_UUID_GATT_CCC,
        BT_GATT_PERM_READ | BT_GATT_PERM_WRITE,
        read_refused_ccc, write_refused_ccc, NULL),
);

/* ============================================================
 * Service 4: Descriptor Test Service
 * ============================================================ */
BT_GATT_SERVICE_DEFINE(descriptor_svc,
    BT_GATT_PRIMARY_SERVICE(BT_UUID_DESCRIPTOR_SERVICE),
    /* Descriptor Test Char — Read, with custom descriptors */
    BT_GATT_CHARACTERISTIC(BT_UUID_DESCRIPTOR_TEST_CHAR,
        BT_GATT_CHRC_READ,
        BT_GATT_PERM_READ,
        read_descriptor_test_char, NULL, NULL),
    /* Read-Only Descriptor */
    BT_GATT_DESCRIPTOR(BT_UUID_READ_ONLY_DESCRIPTOR,
        BT_GATT_PERM_READ,
        read_ro_descriptor, NULL, NULL),
    /* Read/Write Descriptor */
    BT_GATT_DESCRIPTOR(BT_UUID_RW_DESCRIPTOR,
        BT_GATT_PERM_READ | BT_GATT_PERM_WRITE,
        read_rw_descriptor, write_rw_descriptor, NULL),
);
