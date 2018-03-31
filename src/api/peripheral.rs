use std::fmt;
use std::fmt::{Debug, Display, Formatter};
use std::collections::BTreeSet;

use nix;

use api::{AddressType, BDAddr, HandleFn};

#[derive(Ord, PartialOrd, Eq, PartialEq, Clone)]
pub enum CharacteristicUUID {
    B16(u16),
    B128([u8; 16]),
}

impl Display for CharacteristicUUID {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match *self {
            CharacteristicUUID::B16(u) => write!(f, "{:02X}:{:02X}", u >> 8, u & 0xFF),
            CharacteristicUUID::B128(a) => {
                for i in (1..a.len()).rev() {
                    write!(f, "{:02X}:", a[i])?;
                }
                write!(f, "{:02X}", a[0])
            }
        }
    }
}

impl Debug for CharacteristicUUID {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        (self as &Display).fmt(f)
    }
}

bitflags! {
    pub struct CharPropFlags: u8 {
        const BROADCAST = 0x01;
        const READ = 0x02;
        const WRITE_WITHOUT_RESPONSE = 0x04;
        const WRITE = 0x08;
        const NOTIFY = 0x10;
        const INDICATE = 0x20;
        const AUTHENTICATED_SIGNED_WRITES = 0x40;
        const EXTENDED_PROPERTIES = 0x80;
    }
}

#[derive(Debug, Ord, PartialOrd, Eq, PartialEq, Clone)]
pub struct Characteristic {
    pub start_handle: u16,
    pub end_handle: u16,
    pub value_handle: u16,
    pub uuid: CharacteristicUUID,
    pub properties: CharPropFlags,
}

impl Display for Characteristic {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "handle: 0x{:04X}, char properties: 0x{:02X}, \
                   char value handle: 0x{:04X}, end handle: 0x{:04X}, uuid: {:?}",
               self.start_handle, self.properties,
               self.value_handle, self.end_handle, self.uuid)
    }
}

#[derive(Debug, Default)]
pub struct Properties {
    pub address: BDAddr,
    pub address_type: AddressType,
    pub local_name: Option<String>,
    pub tx_power_level: Option<i8>,
    pub manufacturer_data: Option<Vec<u8>>,
    pub discovery_count: u32,
    pub has_scan_response: bool,
}

pub trait Peripheral {
    fn properties(&self) -> Properties;
    fn characteristics(&self) -> BTreeSet<Characteristic>;
    fn is_connected(&self) -> bool;

    fn connect(&mut self) -> nix::Result<()>;
    fn disconnect(&mut self) -> nix::Result<()>;

    fn discover_characteristics(&mut self);
    fn discover_characteristics_in_range(&mut self, start: u16, end: u16);

    fn command(&self, characteristic: &Characteristic, data: &[u8]);
    fn request(&self, characteristic: &Characteristic, data: &[u8], handler: Option<HandleFn>);

    fn subscribe(&self, characteristic: &Characteristic);
    fn unsubscribe(&self, characteristic: &Characteristic);
}
