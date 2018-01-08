use std::collections::BTreeSet;
use std::fmt;
use std::fmt::{Display, Debug, Formatter};

use ::adapter::{BDAddr, AddressType};

#[derive(Ord, PartialOrd, Eq, PartialEq, Clone)]
pub enum CharacteristicUUID {
    B16(u16),
    B128([u8; 16]),
}

impl Display for CharacteristicUUID {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match *self {
            CharacteristicUUID::B16(u) => write!(f, "{:02X}:{:02X}", u >> 8, u & 0xFF),
            CharacteristicUUID::B128(a) => write!(f, "{:X}:{:X}:{:X}:{:X}:{:X}:{:X}",
                                                  a[5], a[4], a[3], a[2], a[1], a[0])

        }
    }
}

impl Debug for CharacteristicUUID {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        (self as &Display).fmt(f)
    }
}

#[derive(Debug, Ord, PartialOrd, Eq, PartialEq, Clone)]
pub struct Characteristic {
    pub start_handle: u16,
    pub end_handle: u16,
    pub value_handle: u16,
    pub properties: u8,
    pub uuid: CharacteristicUUID,
}

impl Display for Characteristic {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "handle: 0x{:04X}, char properties: 0x{:02X}, \
                   char value handle: 0x{:04X}, uuid: {:?}",
               self.start_handle, self.properties,
               self.value_handle, self.uuid)
    }
}

#[derive(Debug, Clone)]
pub struct Device {
    pub address: BDAddr,
    pub address_type: AddressType,
    pub local_name: Option<String>,
    pub tx_power_level: Option<i8>,
    pub manufacturer_data: Option<Vec<u8>>,

    // TODO service_data, service_uuids, solicitation_uuids
    pub characteristics: BTreeSet<Characteristic>,
}

impl Device {
    pub fn new(address: BDAddr, address_type: AddressType) -> Device {
        Device {
            address,
            address_type,
            local_name: None,
            tx_power_level: None,
            manufacturer_data: None,
            characteristics: BTreeSet::new(),
        }
    }
}
