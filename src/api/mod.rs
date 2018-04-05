use std::fmt;
use std::fmt::{Display, Formatter, Debug};

use ::Result;
use std::collections::BTreeSet;

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum AddressType {
    Random,
    Public,
}

impl Default for AddressType {
    fn default() -> Self { AddressType::Public }
}

impl AddressType {
    pub fn from_u8(v: u8) -> Option<AddressType> {
        match v {
            0 => Some(AddressType::Public),
            1 => Some(AddressType::Random),
            _ => None,
        }
    }

    pub fn num(&self) -> u8 {
        match *self {
            AddressType::Public => 0,
            AddressType::Random => 1
        }
    }
}

#[derive(Copy, Hash, Eq, PartialEq, Default)]
#[repr(C)]
pub struct BDAddr {
    pub address: [ u8 ; 6usize ]
}

impl Clone for BDAddr {
    fn clone(&self) -> Self { *self }
}

impl Display for BDAddr {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let a = self.address;
        write!(f, "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
               a[5], a[4], a[3], a[2], a[1], a[0])
    }
}

impl Debug for BDAddr {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        (self as &Display).fmt(f)
    }
}

pub type HandleFn = Box<Fn(u16, &[u8]) + Send>;

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

#[derive(Debug, Default, Clone)]
pub struct Properties {
    pub address: BDAddr,
    pub address_type: AddressType,
    pub local_name: Option<String>,
    pub tx_power_level: Option<i8>,
    pub manufacturer_data: Option<Vec<u8>>,
    pub discovery_count: u32,
    pub has_scan_response: bool,
}

pub trait Peripheral: Send + Sync + Debug {
    fn address(&self) -> BDAddr;
    fn properties(&self) -> Properties;
    fn characteristics(&self) -> BTreeSet<Characteristic>;
    fn is_connected(&self) -> bool;

    fn connect(&self) -> Result<()>;
    fn disconnect(&self) -> Result<()>;

    fn discover_characteristics(&self) -> Result<()>;
    fn discover_characteristics_in_range(&self, start: u16, end: u16) -> Result<()>;

    fn command(&self, characteristic: &Characteristic, data: &[u8]) -> Result<()>;
    fn request(&self, characteristic: &Characteristic, data: &[u8],
               handler: Option<HandleFn>) -> Result<()>;

    fn subscribe(&self, characteristic: &Characteristic) -> Result<()>;
    fn unsubscribe(&self, characteristic: &Characteristic) -> Result<()>;
}

#[derive(Debug, Copy, Clone)]
pub enum Event {
    DeviceDiscovered(BDAddr),
    DeviceLost(BDAddr),
    DeviceUpdated(BDAddr),
    DeviceConnected(BDAddr),
    DeviceDisconnected(BDAddr),
}

pub type EventHandler = Box<Fn(Event) + Send>;

pub trait Host<P : Peripheral> {
    fn on_event(&self, handler: EventHandler);

    fn start_scan(&self) -> Result<()>;
    fn stop_scan(&self) -> Result<()>;

    fn peripherals(&self) -> Vec<P>;
    fn peripheral(&self, address: BDAddr) -> Option<P>;
}
