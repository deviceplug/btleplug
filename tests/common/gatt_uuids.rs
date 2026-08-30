//! UUID constants for the btleplug test GATT profile.
//!
//! Base UUID: XXXXXXXX-b5a3-f393-e0a9-e50e24dcca9e

use uuid::Uuid;

// --- Control Service ---
pub const CONTROL_SERVICE: Uuid = Uuid::from_u128(0x00000001_b5a3_f393_e0a9_e50e24dcca9e);
pub const CONTROL_POINT: Uuid = Uuid::from_u128(0x00000101_b5a3_f393_e0a9_e50e24dcca9e);
pub const CONTROL_RESPONSE: Uuid = Uuid::from_u128(0x00000102_b5a3_f393_e0a9_e50e24dcca9e);

// --- Read/Write Test Service ---
pub const READ_WRITE_SERVICE: Uuid = Uuid::from_u128(0x00000002_b5a3_f393_e0a9_e50e24dcca9e);
pub const STATIC_READ: Uuid = Uuid::from_u128(0x00000201_b5a3_f393_e0a9_e50e24dcca9e);
pub const COUNTER_READ: Uuid = Uuid::from_u128(0x00000202_b5a3_f393_e0a9_e50e24dcca9e);
pub const WRITE_WITH_RESPONSE: Uuid = Uuid::from_u128(0x00000203_b5a3_f393_e0a9_e50e24dcca9e);
pub const WRITE_WITHOUT_RESPONSE: Uuid = Uuid::from_u128(0x00000204_b5a3_f393_e0a9_e50e24dcca9e);
pub const READ_WRITE: Uuid = Uuid::from_u128(0x00000205_b5a3_f393_e0a9_e50e24dcca9e);
pub const LONG_VALUE: Uuid = Uuid::from_u128(0x00000206_b5a3_f393_e0a9_e50e24dcca9e);

// --- Notification Test Service ---
pub const NOTIFICATION_SERVICE: Uuid = Uuid::from_u128(0x00000003_b5a3_f393_e0a9_e50e24dcca9e);
pub const NOTIFY_CHAR: Uuid = Uuid::from_u128(0x00000301_b5a3_f393_e0a9_e50e24dcca9e);
pub const INDICATE_CHAR: Uuid = Uuid::from_u128(0x00000302_b5a3_f393_e0a9_e50e24dcca9e);
pub const CONFIGURABLE_NOTIFY: Uuid = Uuid::from_u128(0x00000303_b5a3_f393_e0a9_e50e24dcca9e);

// --- Descriptor Test Service ---
pub const DESCRIPTOR_SERVICE: Uuid = Uuid::from_u128(0x00000004_b5a3_f393_e0a9_e50e24dcca9e);
pub const DESCRIPTOR_TEST_CHAR: Uuid = Uuid::from_u128(0x00000401_b5a3_f393_e0a9_e50e24dcca9e);
pub const READ_ONLY_DESCRIPTOR: Uuid = Uuid::from_u128(0x000004a1_b5a3_f393_e0a9_e50e24dcca9e);
pub const READ_WRITE_DESCRIPTOR: Uuid = Uuid::from_u128(0x000004a2_b5a3_f393_e0a9_e50e24dcca9e);

// --- Control Point opcodes ---
pub const CMD_START_NOTIFICATIONS: u8 = 0x01;
pub const CMD_STOP_NOTIFICATIONS: u8 = 0x02;
pub const CMD_TRIGGER_DISCONNECT: u8 = 0x03;
pub const CMD_CHANGE_ADVERTISEMENTS: u8 = 0x04;
pub const CMD_RESET_STATE: u8 = 0x05;
pub const CMD_SET_NOTIFICATION_PAYLOAD: u8 = 0x06;

// --- Test constants ---
pub const STATIC_READ_VALUE: &[u8] = &[0x01, 0x02, 0x03, 0x04];
pub const TEST_PERIPHERAL_NAME: &str = "btleplug-test";
pub const TEST_APPEARANCE: u16 = 0x0340;
pub const MANUFACTURER_COMPANY_ID: u16 = 0xFFFF;
