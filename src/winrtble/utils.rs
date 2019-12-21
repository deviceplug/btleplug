use api::BDAddr;
use api::UUID;
use winrt::Guid;
use api::CharPropFlags;
use winrt::windows::devices::bluetooth::genericattributeprofile::{GattCharacteristicProperties, GattCommunicationStatus};
use ::Error;
use ::Result;

pub fn to_error(status: GattCommunicationStatus) -> Result<()> {
    match status {
        GattCommunicationStatus::AccessDenied => {
            Err(Error::PermissionDenied)
        },
        GattCommunicationStatus::Unreachable => {
            Err(Error::NotConnected)
        },
        GattCommunicationStatus::Success => {
            Ok(())
        },
        GattCommunicationStatus::ProtocolError => {
            Err(Error::NotSupported("ProtocolError".to_string()))
        },
        GattCommunicationStatus(a) => {
            Err(Error::Other(format!("Communication Error: {}", a)))
        },
    }
}

pub fn to_addr(addr: u64) -> BDAddr {
    let mut address : [u8; 6usize] = [0, 0, 0, 0, 0, 0];
    for i in 0..6 {
        address[i] = (addr >> (8 * i)) as u8;
    }
    BDAddr{ address }
}

pub fn to_address(addr: BDAddr) -> u64 {
    let mut address = 0u64;
    for i in (0..6).rev() {
        address |= (u64::from(addr.address[i])) << (8 * i);
    }
    address
}

pub fn to_uuid(uuid: &Guid) -> UUID {
    let mut array = [0u8; 16];
    for i in 0..4 {
        array[i] = (uuid.Data1 >> (8 * i)) as u8;
    }
    for i in 0..2 {
        array[i + 4] = (uuid.Data2 >> (8 * i)) as u8;
    }
    for i in 0..2 {
        array[i + 6] = (uuid.Data3 >> (8 * i)) as u8;
    }
    for i in 0..8 {
        array[i + 8] = uuid.Data4[i];
    }
    UUID::B128(array)
}

pub fn to_guid(uuid: &UUID) -> Guid {
    match uuid {
        UUID::B128(a) => {
            let mut data1 = 0;
            for i in 0..4 {
                data1 |= u32::from(a[i]) << (8 * i);
            }
            let mut data2 = 0;
            for i in 0..2 {
                data2 |= u16::from(a[i + 4]) << (8 * i);
            }
            let mut data3 = 0;
            for i in 0..2 {
                data3 |= u16::from(a[i + 6]) << (8 * i);
            }
            let mut data4 = [0; 8];
            for i in 0..8 {
                data4[i] = a[i + 8];
            }
            Guid{ Data1: data1, Data2: data2, Data3: data3, Data4: data4 }
        },
        UUID::B16(b) => {
            Guid{ Data1: 0, Data2: 0, Data3: 0, Data4: [0; 8] }
        }
    }
}

fn guid_to_string(guid: &Guid) -> String {
    format!("{:08X}:{:04X}:{:04X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
	        guid.Data1, guid.Data2, guid.Data3,
	        guid.Data4[0], guid.Data4[1], guid.Data4[2], guid.Data4[3],
            guid.Data4[4], guid.Data4[5], guid.Data4[6], guid.Data4[7])
}

pub fn to_char_props(properties: &GattCharacteristicProperties) -> CharPropFlags {
    CharPropFlags::from_bits_truncate(properties.0 as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_address() {
        let bluetooth_address = 252566450624623;
        let addr = to_addr(bluetooth_address);
        let result = to_address(addr);
        assert_eq!(bluetooth_address, result);
    }
}
