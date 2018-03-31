pub mod host;
pub mod peripheral;

use std::fmt;
use std::fmt::{Display, Formatter, Debug};

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
