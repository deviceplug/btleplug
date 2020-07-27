// btleplug Source Code File
//
// Copyright 2020 Nonpolynomial Labs LLC. All rights reserved.
//
// Licensed under the BSD 3-Clause license. See LICENSE file in the project root
// for full license information.
//
// Some portions of this file are taken and/or modified from Rumble
// (https://github.com/mwylde/rumble), using a dual MIT/Apache License under the
// following copyright:
//
// Copyright (c) 2014 The Rust Project Developers

use crate::{
    api::{BDAddr, UUID, CharPropFlags},
    Error,
    Result
};
use winrt::Guid;
use winrt::windows::devices::bluetooth::genericattributeprofile::{GattCharacteristicProperties, GattCommunicationStatus};

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

// If we want to get this into Bluez format, we've got to flip everything into a U128.
pub fn to_uuid(uuid: &Guid) -> UUID {
    let mut array = [0u8; 16];
    for i in 0..4 {
        array[i + 12] = (uuid.Data1 >> (8 * i)) as u8;
    }
    for i in 0..2 {
        array[i + 10] = (uuid.Data2 >> (8 * i)) as u8;
    }
    for i in 0..2 {
        array[i + 8] = (uuid.Data3 >> (8 * i)) as u8;
    }
    for i in 0..8 {
        array[i] = uuid.Data4[7-i];
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
        UUID::B16(_) => {
            Guid{ Data1: 0, Data2: 0, Data3: 0, Data4: [0; 8] }
        }
    }
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
