//! Utilities for dealing with BLE UUIDs, converting to and from their short formats.

use uuid::Uuid;

const BLUETOOTH_BASE_UUID: u128 = 0x00000000_0000_1000_8000_00805f9b34fb;
const BLUETOOTH_BASE_MASK: u128 = 0x00000000_ffff_ffff_ffff_ffffffffffff;
const BLUETOOTH_BASE_MASK_16: u128 = 0xffff0000_ffff_ffff_ffff_ffffffffffff;

// TODO: Make these functions part of the `BleUuid` trait once const fn is allowed there.
/// Convert a 32-bit BLE short UUID to a full 128-bit UUID by filling in the standard Bluetooth Base
/// UUID.
pub const fn uuid_from_u32(short: u32) -> Uuid {
    Uuid::from_u128(BLUETOOTH_BASE_UUID | ((short as u128) << 96))
}

/// Convert a 16-bit BLE short UUID to a full 128-bit UUID by filling in the standard Bluetooth Base
/// UUID.
pub const fn uuid_from_u16(short: u16) -> Uuid {
    uuid_from_u32(short as u32)
}

/// An extension trait for `Uuid` which provides BLE-specific methods.
pub trait BleUuid {
    /// If the UUID is a valid BLE short UUID then return its short form, otherwise return `None`.
    fn to_ble_u32(&self) -> Option<u32>;

    /// If the UUID is a valid 16-bit BLE short UUID then return its short form, otherwise return
    /// `None`.
    fn to_ble_u16(&self) -> Option<u16>;

    /// Convert the UUID to a string, using short format if applicable.
    fn to_short_string(&self) -> String;
}

impl BleUuid for Uuid {
    fn to_ble_u32(&self) -> Option<u32> {
        let value = self.as_u128();
        if value & BLUETOOTH_BASE_MASK == BLUETOOTH_BASE_UUID {
            Some((value >> 96) as u32)
        } else {
            None
        }
    }

    fn to_ble_u16(&self) -> Option<u16> {
        let value = self.as_u128();
        if value & BLUETOOTH_BASE_MASK_16 == BLUETOOTH_BASE_UUID {
            Some((value >> 96) as u16)
        } else {
            None
        }
    }

    fn to_short_string(&self) -> String {
        if let Some(uuid16) = self.to_ble_u16() {
            format!("{:#04x}", uuid16)
        } else if let Some(uuid32) = self.to_ble_u32() {
            format!("{:#06x}", uuid32)
        } else {
            self.to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uuid_from_u32_test() {
        assert_eq!(
            uuid_from_u32(0x11223344),
            Uuid::parse_str("11223344-0000-1000-8000-00805f9b34fb").unwrap()
        );
    }

    #[test]
    fn uuid_from_u16_test() {
        assert_eq!(
            uuid_from_u16(0x1122),
            Uuid::parse_str("00001122-0000-1000-8000-00805f9b34fb").unwrap()
        );
    }

    #[test]
    fn uuid_to_from_u16_success() {
        let uuid = Uuid::parse_str("00001234-0000-1000-8000-00805f9b34fb").unwrap();
        assert_eq!(uuid_from_u16(uuid.to_ble_u16().unwrap()), uuid);
    }

    #[test]
    fn uuid_to_from_u32_success() {
        let uuid = Uuid::parse_str("12345678-0000-1000-8000-00805f9b34fb").unwrap();
        assert_eq!(uuid_from_u32(uuid.to_ble_u32().unwrap()), uuid);
    }

    #[test]
    fn uuid_to_u16_fail() {
        assert_eq!(
            Uuid::parse_str("12345678-0000-1000-8000-00805f9b34fb")
                .unwrap()
                .to_ble_u16(),
            None
        );
        assert_eq!(
            Uuid::parse_str("12340000-0000-1000-8000-00805f9b34fb")
                .unwrap()
                .to_ble_u16(),
            None
        );
        assert_eq!(Uuid::nil().to_ble_u16(), None);
    }

    #[test]
    fn uuid_to_u32_fail() {
        assert_eq!(
            Uuid::parse_str("12345678-9000-1000-8000-00805f9b34fb")
                .unwrap()
                .to_ble_u32(),
            None
        );
        assert_eq!(Uuid::nil().to_ble_u32(), None);
    }

    #[test]
    fn to_short_string_u16() {
        let uuid = uuid_from_u16(0x1122);
        assert_eq!(uuid.to_short_string(), "0x1122");
    }

    #[test]
    fn to_short_string_u32() {
        let uuid = uuid_from_u32(0x11223344);
        assert_eq!(uuid.to_short_string(), "0x11223344");
    }

    #[test]
    fn to_short_string_long() {
        let uuid_str = "12345678-9000-1000-8000-00805f9b34fb";
        let uuid = Uuid::parse_str(uuid_str).unwrap();
        assert_eq!(uuid.to_short_string(), uuid_str);
    }
}
