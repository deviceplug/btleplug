#include <zephyr/kernel.h>
#include <zephyr/bluetooth/bluetooth.h>
#include <zephyr/bluetooth/gatt.h>
#include <string.h>

#include "gatt_profile.h"

#include <zephyr/logging/log.h>
LOG_MODULE_REGISTER(test_handlers, LOG_LEVEL_INF);

/* Static read value — always returns [0x01, 0x02, 0x03, 0x04] */
static const uint8_t static_read_val[] = STATIC_READ_VALUE;

/* --- Read/Write Test Service Callbacks --- */

ssize_t read_static(struct bt_conn *conn, const struct bt_gatt_attr *attr,
                    void *buf, uint16_t len, uint16_t offset)
{
    return bt_gatt_attr_read(conn, attr, buf, len, offset,
                             static_read_val, sizeof(static_read_val));
}

ssize_t read_counter(struct bt_conn *conn, const struct bt_gatt_attr *attr,
                     void *buf, uint16_t len, uint16_t offset)
{
    uint32_t counter = g_state.read_counter++;
    uint8_t data[4] = {
        (uint8_t)(counter & 0xFF),
        (uint8_t)((counter >> 8) & 0xFF),
        (uint8_t)((counter >> 16) & 0xFF),
        (uint8_t)((counter >> 24) & 0xFF),
    };
    return bt_gatt_attr_read(conn, attr, buf, len, offset, data, sizeof(data));
}

ssize_t write_with_resp(struct bt_conn *conn, const struct bt_gatt_attr *attr,
                        const void *buf, uint16_t len, uint16_t offset,
                        uint8_t flags)
{
    if (offset + len > sizeof(g_state.write_with_resp_value)) {
        return BT_GATT_ERR(BT_ATT_ERR_INVALID_OFFSET);
    }
    memcpy(g_state.write_with_resp_value + offset, buf, len);
    g_state.write_with_resp_len = offset + len;
    LOG_INF("Write with response: %u bytes", len);
    return len;
}

ssize_t write_without_resp(struct bt_conn *conn, const struct bt_gatt_attr *attr,
                           const void *buf, uint16_t len, uint16_t offset,
                           uint8_t flags)
{
    if (offset + len > sizeof(g_state.rw_value)) {
        return BT_GATT_ERR(BT_ATT_ERR_INVALID_OFFSET);
    }
    /*
     * Write-without-response stores to rw_value so the Rust test can
     * verify receipt by reading back through the Read/Write characteristic.
     */
    memcpy(g_state.rw_value + offset, buf, len);
    g_state.rw_value_len = offset + len;
    LOG_INF("Write without response: %u bytes", len);
    return len;
}

ssize_t read_rw(struct bt_conn *conn, const struct bt_gatt_attr *attr,
                void *buf, uint16_t len, uint16_t offset)
{
    return bt_gatt_attr_read(conn, attr, buf, len, offset,
                             g_state.rw_value, g_state.rw_value_len);
}

ssize_t write_rw(struct bt_conn *conn, const struct bt_gatt_attr *attr,
                 const void *buf, uint16_t len, uint16_t offset,
                 uint8_t flags)
{
    if (offset + len > sizeof(g_state.rw_value)) {
        return BT_GATT_ERR(BT_ATT_ERR_INVALID_OFFSET);
    }
    memcpy(g_state.rw_value + offset, buf, len);
    g_state.rw_value_len = offset + len;
    LOG_INF("Read/Write char written: %u bytes", len);
    return len;
}

ssize_t read_long_value(struct bt_conn *conn, const struct bt_gatt_attr *attr,
                        void *buf, uint16_t len, uint16_t offset)
{
    return bt_gatt_attr_read(conn, attr, buf, len, offset,
                             g_state.long_value, g_state.long_value_len);
}

ssize_t write_long_value(struct bt_conn *conn, const struct bt_gatt_attr *attr,
                         const void *buf, uint16_t len, uint16_t offset,
                         uint8_t flags)
{
    if (offset + len > sizeof(g_state.long_value)) {
        return BT_GATT_ERR(BT_ATT_ERR_INVALID_OFFSET);
    }
    memcpy(g_state.long_value + offset, buf, len);
    if (offset + len > g_state.long_value_len) {
        g_state.long_value_len = offset + len;
    }
    LOG_INF("Long value written: %u bytes at offset %u", len, offset);
    return len;
}

/* --- Error Test Characteristic Callbacks --- */

/* ERROR_CHAR: every read and write fails (#492). */
ssize_t read_error(struct bt_conn *conn, const struct bt_gatt_attr *attr,
                   void *buf, uint16_t len, uint16_t offset)
{
    return BT_GATT_ERR(ERROR_CHAR_ATT_ERROR);
}

ssize_t write_error(struct bt_conn *conn, const struct bt_gatt_attr *attr,
                    const void *buf, uint16_t len, uint16_t offset,
                    uint8_t flags)
{
    return BT_GATT_ERR(ERROR_CHAR_ATT_ERROR);
}

/* --- Descriptor Test Service Callbacks --- */

static const uint8_t read_only_descriptor_val[] = {0xDE, 0xAD, 0xBE, 0xEF};

ssize_t read_ro_descriptor(struct bt_conn *conn, const struct bt_gatt_attr *attr,
                           void *buf, uint16_t len, uint16_t offset)
{
    return bt_gatt_attr_read(conn, attr, buf, len, offset,
                             read_only_descriptor_val,
                             sizeof(read_only_descriptor_val));
}

ssize_t read_rw_descriptor(struct bt_conn *conn, const struct bt_gatt_attr *attr,
                           void *buf, uint16_t len, uint16_t offset)
{
    return bt_gatt_attr_read(conn, attr, buf, len, offset,
                             g_state.rw_descriptor_value,
                             g_state.rw_descriptor_len);
}

ssize_t write_rw_descriptor(struct bt_conn *conn, const struct bt_gatt_attr *attr,
                            const void *buf, uint16_t len, uint16_t offset,
                            uint8_t flags)
{
    if (offset + len > sizeof(g_state.rw_descriptor_value)) {
        return BT_GATT_ERR(BT_ATT_ERR_INVALID_OFFSET);
    }
    memcpy(g_state.rw_descriptor_value + offset, buf, len);
    g_state.rw_descriptor_len = offset + len;
    LOG_INF("R/W descriptor written: %u bytes", len);
    return len;
}

/* --- Descriptor Test Char (parent) --- */

ssize_t read_descriptor_test_char(struct bt_conn *conn,
                                  const struct bt_gatt_attr *attr,
                                  void *buf, uint16_t len, uint16_t offset)
{
    static const uint8_t val[] = {0x00};
    return bt_gatt_attr_read(conn, attr, buf, len, offset, val, sizeof(val));
}

/* --- Notification CCC Callbacks --- */

void notify_ccc_changed(const struct bt_gatt_attr *attr, uint16_t value)
{
    g_state.notify_enabled = (value == BT_GATT_CCC_NOTIFY);
    LOG_INF("Notify CCC: %s", g_state.notify_enabled ? "enabled" : "disabled");
}

void indicate_ccc_changed(const struct bt_gatt_attr *attr, uint16_t value)
{
    g_state.indicate_enabled = (value == BT_GATT_CCC_INDICATE);
    LOG_INF("Indicate CCC: %s", g_state.indicate_enabled ? "enabled" : "disabled");
}

void configurable_notify_ccc_changed(const struct bt_gatt_attr *attr, uint16_t value)
{
    g_state.configurable_notify_enabled = (value == BT_GATT_CCC_NOTIFY);
    LOG_INF("Configurable Notify CCC: %s",
            g_state.configurable_notify_enabled ? "enabled" : "disabled");
}

/*
 * Unmanaged CCC that rejects every write so subscribe fails with an ATT error (#471).
 * Unmanaged (not BT_GATT_CCC/BT_GATT_CCC_WITH_WRITE_CB) because a managed CCC claims
 * its one cfg slot before this write callback runs, and a rejected write never frees it.
 */
ssize_t write_refused_ccc(struct bt_conn *conn, const struct bt_gatt_attr *attr,
                          const void *buf, uint16_t len, uint16_t offset,
                          uint8_t flags)
{
    LOG_INF("Refused Notify CCC write rejected");
    return BT_GATT_ERR(BT_ATT_ERR_WRITE_REQ_REJECTED);
}

ssize_t read_refused_ccc(struct bt_conn *conn, const struct bt_gatt_attr *attr,
                         void *buf, uint16_t len, uint16_t offset)
{
    static const uint8_t ccc_value[2] = {0x00, 0x00};
    return bt_gatt_attr_read(conn, attr, buf, len, offset,
                             ccc_value, sizeof(ccc_value));
}
