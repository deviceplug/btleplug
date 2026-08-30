"""
btleplug Test Peripheral — Bumble Implementation

Implements the same GATT profile as the Zephyr firmware for testing
btleplug without dedicated hardware. Requires a USB BLE dongle or
platform-specific BLE transport.

Usage:
    python test_peripheral.py <transport>
    python test_peripheral.py usb:0
    python test_peripheral.py hci-socket:0
"""

import asyncio
import logging
import struct
import sys

from bumble.core import UUID as BumbleUUID, AdvertisingData
from bumble.device import Device, Connection
from bumble.gatt import (
    Service,
    Characteristic,
    CharacteristicValue,
    Descriptor,
)
from bumble.host import Host
from bumble.transport import open_transport_or_link

logging.basicConfig(level=logging.INFO)
logger = logging.getLogger(__name__)

# ──────────────────────────────────────────────────────────────
# UUID Constants (must match tests/common/gatt_uuids.rs and
# test-peripheral/zephyr/src/gatt_profile.h)
# ──────────────────────────────────────────────────────────────

CONTROL_SERVICE_UUID       = BumbleUUID("00000001-b5a3-f393-e0a9-e50e24dcca9e")
CONTROL_POINT_UUID         = BumbleUUID("00000101-b5a3-f393-e0a9-e50e24dcca9e")
CONTROL_RESPONSE_UUID      = BumbleUUID("00000102-b5a3-f393-e0a9-e50e24dcca9e")

RW_SERVICE_UUID            = BumbleUUID("00000002-b5a3-f393-e0a9-e50e24dcca9e")
STATIC_READ_UUID           = BumbleUUID("00000201-b5a3-f393-e0a9-e50e24dcca9e")
COUNTER_READ_UUID          = BumbleUUID("00000202-b5a3-f393-e0a9-e50e24dcca9e")
WRITE_WITH_RESP_UUID       = BumbleUUID("00000203-b5a3-f393-e0a9-e50e24dcca9e")
WRITE_WITHOUT_RESP_UUID    = BumbleUUID("00000204-b5a3-f393-e0a9-e50e24dcca9e")
READ_WRITE_UUID            = BumbleUUID("00000205-b5a3-f393-e0a9-e50e24dcca9e")
LONG_VALUE_UUID            = BumbleUUID("00000206-b5a3-f393-e0a9-e50e24dcca9e")

NOTIFY_SERVICE_UUID        = BumbleUUID("00000003-b5a3-f393-e0a9-e50e24dcca9e")
NOTIFY_CHAR_UUID           = BumbleUUID("00000301-b5a3-f393-e0a9-e50e24dcca9e")
INDICATE_CHAR_UUID         = BumbleUUID("00000302-b5a3-f393-e0a9-e50e24dcca9e")
CONFIGURABLE_NOTIFY_UUID   = BumbleUUID("00000303-b5a3-f393-e0a9-e50e24dcca9e")

DESCRIPTOR_SERVICE_UUID    = BumbleUUID("00000004-b5a3-f393-e0a9-e50e24dcca9e")
DESCRIPTOR_TEST_CHAR_UUID  = BumbleUUID("00000401-b5a3-f393-e0a9-e50e24dcca9e")
RO_DESCRIPTOR_UUID         = BumbleUUID("000004a1-b5a3-f393-e0a9-e50e24dcca9e")
RW_DESCRIPTOR_UUID         = BumbleUUID("000004a2-b5a3-f393-e0a9-e50e24dcca9e")

# Control Point opcodes
CMD_START_NOTIFICATIONS      = 0x01
CMD_STOP_NOTIFICATIONS       = 0x02
CMD_TRIGGER_DISCONNECT       = 0x03
CMD_CHANGE_ADVERTISEMENTS    = 0x04
CMD_RESET_STATE              = 0x05
CMD_SET_NOTIFICATION_PAYLOAD = 0x06

DEVICE_NAME = "btleplug-test"
TEST_APPEARANCE = 0x0340
MANUFACTURER_COMPANY_ID = 0xFFFF
STATIC_READ_VALUE = bytes([0x01, 0x02, 0x03, 0x04])
NOTIFICATION_INTERVAL = 1.0  # seconds


class TestPeripheralState:
    """Mutable state for the test peripheral."""

    def __init__(self):
        self.reset()

    def reset(self):
        self.read_counter = 0
        self.rw_value = bytearray()
        self.long_value = bytearray(512)
        self.write_with_resp_value = bytearray()
        self.write_without_resp_value = bytearray()
        self.notify_payload = bytearray()
        self.rw_descriptor_value = bytearray()
        self.notify_task: asyncio.Task | None = None


state = TestPeripheralState()


# ──────────────────────────────────────────────────────────────
# GATT Handlers
# ──────────────────────────────────────────────────────────────

def read_static(_connection):
    return STATIC_READ_VALUE


def read_counter(_connection):
    val = struct.pack("<I", state.read_counter)
    state.read_counter += 1
    return val


def write_with_resp(_connection, value):
    state.write_with_resp_value = bytearray(value)
    logger.info("Write with response: %d bytes", len(value))


def write_without_resp(_connection, value):
    state.write_without_resp_value = bytearray(value)
    logger.info("Write without response: %d bytes", len(value))


def read_rw(_connection):
    return bytes(state.rw_value)


def write_rw(_connection, value):
    state.rw_value = bytearray(value)
    logger.info("Read/Write char written: %d bytes", len(value))


def read_long(_connection):
    return bytes(state.long_value)


def write_long(_connection, value):
    state.long_value = bytearray(value)
    logger.info("Long value written: %d bytes", len(value))


def read_descriptor_test_char(_connection):
    return bytes([0x00])


def read_ro_descriptor(_connection):
    return bytes([0xDE, 0xAD, 0xBE, 0xEF])


def read_rw_descriptor(_connection):
    return bytes(state.rw_descriptor_value)


def write_rw_descriptor(_connection, value):
    state.rw_descriptor_value = bytearray(value)
    logger.info("R/W descriptor written: %d bytes", len(value))


# ──────────────────────────────────────────────────────────────
# Notification Engine
# ──────────────────────────────────────────────────────────────

notify_char_ref = None
indicate_char_ref = None
configurable_notify_ref = None


async def notification_loop(device: Device):
    """Periodically send notifications/indications to subscribed clients."""
    counter = 0
    while True:
        await asyncio.sleep(NOTIFICATION_INTERVAL)
        counter = (counter + 1) % 256

        if notify_char_ref is not None:
            try:
                await device.notify_subscribers(notify_char_ref, bytes([counter]))
            except Exception as e:
                logger.debug("Notify failed: %s", e)

        if indicate_char_ref is not None:
            try:
                await device.indicate_subscribers(indicate_char_ref, bytes([counter]))
            except Exception as e:
                logger.debug("Indicate failed: %s", e)

        if configurable_notify_ref is not None and len(state.notify_payload) > 0:
            try:
                await device.notify_subscribers(
                    configurable_notify_ref, bytes(state.notify_payload)
                )
            except Exception as e:
                logger.debug("Configurable notify failed: %s", e)


def start_notifications(device: Device):
    if state.notify_task is None or state.notify_task.done():
        state.notify_task = asyncio.get_running_loop().create_task(
            notification_loop(device)
        )
        logger.info("Periodic notifications started")


def stop_notifications():
    if state.notify_task is not None and not state.notify_task.done():
        state.notify_task.cancel()
        state.notify_task = None
        logger.info("Periodic notifications stopped")


# ──────────────────────────────────────────────────────────────
# Control Point Handler
# ──────────────────────────────────────────────────────────────

def make_control_point_handler(device: Device):
    def handler(_connection, value):
        if len(value) < 1:
            logger.warning("Empty control command")
            return

        opcode = value[0]
        logger.info("Control command: 0x%02x", opcode)

        if opcode == CMD_START_NOTIFICATIONS:
            start_notifications(device)
        elif opcode == CMD_STOP_NOTIFICATIONS:
            stop_notifications()
        elif opcode == CMD_TRIGGER_DISCONNECT:
            async def do_disconnect():
                await asyncio.sleep(0.5)
                if _connection and not _connection.is_disconnected:
                    await _connection.disconnect()
            asyncio.get_running_loop().create_task(do_disconnect())
        elif opcode == CMD_CHANGE_ADVERTISEMENTS:
            # Deferred: advertisement rotation is not tested in the initial suite
            logger.info("Change advertisements (deferred — not tested in initial suite)")
        elif opcode == CMD_RESET_STATE:
            stop_notifications()
            state.reset()
            logger.info("Peripheral state reset")
        elif opcode == CMD_SET_NOTIFICATION_PAYLOAD:
            if len(value) > 1:
                state.notify_payload = bytearray(value[1:])
                logger.info("Notification payload set: %d bytes", len(state.notify_payload))
        else:
            logger.warning("Unknown control opcode: 0x%02x", opcode)

    return handler


# ──────────────────────────────────────────────────────────────
# Build GATT Services
# ──────────────────────────────────────────────────────────────

def build_services(device: Device):
    global notify_char_ref, indicate_char_ref, configurable_notify_ref

    # --- Control Service ---
    control_point_char = Characteristic(
        CONTROL_POINT_UUID,
        Characteristic.Properties.WRITE,
        Characteristic.WRITEABLE,
        CharacteristicValue(write=make_control_point_handler(device)),
    )
    control_response_char = Characteristic(
        CONTROL_RESPONSE_UUID,
        Characteristic.Properties.NOTIFY,
        0,
    )
    control_service = Service(CONTROL_SERVICE_UUID, [
        control_point_char,
        control_response_char,
    ])

    # --- Read/Write Test Service ---
    static_read_char = Characteristic(
        STATIC_READ_UUID,
        Characteristic.Properties.READ,
        Characteristic.READABLE,
        CharacteristicValue(read=read_static),
    )
    counter_read_char = Characteristic(
        COUNTER_READ_UUID,
        Characteristic.Properties.READ,
        Characteristic.READABLE,
        CharacteristicValue(read=read_counter),
    )
    write_with_resp_char = Characteristic(
        WRITE_WITH_RESP_UUID,
        Characteristic.Properties.WRITE,
        Characteristic.WRITEABLE,
        CharacteristicValue(write=write_with_resp),
    )
    write_without_resp_char = Characteristic(
        WRITE_WITHOUT_RESP_UUID,
        Characteristic.Properties.WRITE_WITHOUT_RESPONSE,
        Characteristic.WRITEABLE,
        CharacteristicValue(write=write_without_resp),
    )
    read_write_char = Characteristic(
        READ_WRITE_UUID,
        Characteristic.Properties.READ | Characteristic.Properties.WRITE,
        Characteristic.READABLE | Characteristic.WRITEABLE,
        CharacteristicValue(read=read_rw, write=write_rw),
    )
    long_value_char = Characteristic(
        LONG_VALUE_UUID,
        Characteristic.Properties.READ | Characteristic.Properties.WRITE,
        Characteristic.READABLE | Characteristic.WRITEABLE,
        CharacteristicValue(read=read_long, write=write_long),
    )
    rw_service = Service(RW_SERVICE_UUID, [
        static_read_char,
        counter_read_char,
        write_with_resp_char,
        write_without_resp_char,
        read_write_char,
        long_value_char,
    ])

    # --- Notification Test Service ---
    notify_char = Characteristic(
        NOTIFY_CHAR_UUID,
        Characteristic.Properties.NOTIFY,
        0,
    )
    indicate_char = Characteristic(
        INDICATE_CHAR_UUID,
        Characteristic.Properties.INDICATE,
        0,
    )
    configurable_notify_char = Characteristic(
        CONFIGURABLE_NOTIFY_UUID,
        Characteristic.Properties.NOTIFY,
        0,
    )
    notify_service = Service(NOTIFY_SERVICE_UUID, [
        notify_char,
        indicate_char,
        configurable_notify_char,
    ])

    # Store references for notification sending
    notify_char_ref = notify_char
    indicate_char_ref = indicate_char
    configurable_notify_ref = configurable_notify_char

    # --- Descriptor Test Service ---
    ro_descriptor = Descriptor(
        RO_DESCRIPTOR_UUID,
        Descriptor.READABLE,
        CharacteristicValue(read=read_ro_descriptor),
    )
    rw_descriptor = Descriptor(
        RW_DESCRIPTOR_UUID,
        Descriptor.READABLE | Descriptor.WRITEABLE,
        CharacteristicValue(read=read_rw_descriptor, write=write_rw_descriptor),
    )
    descriptor_test_char = Characteristic(
        DESCRIPTOR_TEST_CHAR_UUID,
        Characteristic.Properties.READ,
        Characteristic.READABLE,
        CharacteristicValue(read=read_descriptor_test_char),
        descriptors=[ro_descriptor, rw_descriptor],
    )
    descriptor_service = Service(DESCRIPTOR_SERVICE_UUID, [
        descriptor_test_char,
    ])

    return [control_service, rw_service, notify_service, descriptor_service]


# ──────────────────────────────────────────────────────────────
# Main
# ──────────────────────────────────────────────────────────────

async def main():
    if len(sys.argv) < 2:
        print(f"Usage: {sys.argv[0]} <transport>")
        print("  e.g.: python test_peripheral.py usb:0")
        print("  e.g.: python test_peripheral.py hci-socket:0")
        sys.exit(1)

    transport_name = sys.argv[1]
    logger.info("Opening transport: %s", transport_name)

    async with await open_transport_or_link(transport_name) as (
        hci_source,
        hci_sink,
    ):
        device = Device(name=DEVICE_NAME, host=Host(hci_source, hci_sink))

        # Register GATT services
        services = build_services(device)
        for service in services:
            device.add_service(service)

        # Power on
        await device.power_on()

        # Set up advertising data
        device.advertising_data = bytes(
            AdvertisingData(
                [
                    (
                        AdvertisingData.COMPLETE_LOCAL_NAME,
                        DEVICE_NAME.encode("utf-8"),
                    ),
                    (
                        AdvertisingData.INCOMPLETE_LIST_OF_128_BIT_SERVICE_CLASS_UUIDS,
                        bytes(CONTROL_SERVICE_UUID),
                    ),
                    (
                        AdvertisingData.MANUFACTURER_SPECIFIC_DATA,
                        struct.pack("<H", MANUFACTURER_COMPANY_ID)
                        + bytes([0xBB, 0xCC, 0x01]),
                    ),
                    (AdvertisingData.APPEARANCE, struct.pack("<H", TEST_APPEARANCE)),
                    (AdvertisingData.TX_POWER_LEVEL, bytes([0])),  # 0 dBm
                ]
            )
        )

        # Start advertising
        await device.start_advertising(auto_restart=True)
        logger.info("Advertising as '%s'", DEVICE_NAME)

        # Connection event handlers
        @device.on("connection")
        def on_connection(connection: Connection):
            logger.info("Connected: %s", connection.peer_address)

        @device.on("disconnection")
        def on_disconnection(reason):
            logger.info("Disconnected (reason: %s)", reason)
            stop_notifications()

        # Run forever
        logger.info("Test peripheral running. Press Ctrl+C to stop.")
        await asyncio.Event().wait()


if __name__ == "__main__":
    asyncio.run(main())
