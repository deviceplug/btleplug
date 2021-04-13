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

use super::bindings;
use crate::{
    api::{BDAddr, CharPropFlags},
    Error, Result,
};
use bindings::windows::{
    devices::bluetooth::generic_attribute_profile::{
        GattCharacteristicProperties, GattCommunicationStatus,
    },
    storage::streams::{DataReader, IBuffer},
};
use std::str::FromStr;
use uuid::Uuid;
use windows::Guid;

pub fn to_error(status: GattCommunicationStatus) -> Result<()> {
    if status == GattCommunicationStatus::AccessDenied {
        Err(Error::PermissionDenied)
    } else if status == GattCommunicationStatus::Unreachable {
        Err(Error::NotConnected)
    } else if status == GattCommunicationStatus::Success {
        Ok(())
    } else if status == GattCommunicationStatus::ProtocolError {
        Err(Error::NotSupported("ProtocolError".to_string()))
    } else {
        Err(Error::Other(format!("Communication Error:")))
    }
}

pub fn to_addr(addr: u64) -> BDAddr {
    let mut address: [u8; 6usize] = [0, 0, 0, 0, 0, 0];
    for i in 0..6 {
        address[i] = (addr >> (8 * i)) as u8;
    }
    BDAddr { address }
}

pub fn to_address(addr: BDAddr) -> u64 {
    let mut address = 0u64;
    for i in (0..6).rev() {
        address |= (u64::from(addr.address[i])) << (8 * i);
    }
    address
}

pub fn to_uuid(uuid: &Guid) -> Uuid {
    let guid_s = format!("{:?}", uuid);
    Uuid::from_str(&guid_s).unwrap()
}

pub fn to_vec(buffer: &IBuffer) -> Vec<u8> {
    let reader = DataReader::from_buffer(buffer).unwrap();
    let len = reader.unconsumed_buffer_length().unwrap() as usize;
    let mut data = vec![0u8; len];
    reader.read_bytes(&mut data).unwrap();
    data
}

pub fn to_guid(uuid: &Uuid) -> Guid {
    let (data1, data2, data3, data4) = uuid.as_fields();
    Guid::from_values(data1, data2, data3, data4.to_owned())
}

pub fn to_char_props(_: &GattCharacteristicProperties) -> CharPropFlags {
    CharPropFlags::from_bits_truncate(0 as u8)
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

    #[test]
    fn check_uuid_to_guid_conversion() {
        let uuid_str = "10B201FF-5B3B-45A1-9508-CF3EFCD7BBAF";
        let uuid = Uuid::from_str(uuid_str).unwrap();

        let guid_converted = to_guid(&uuid);

        let guid_expected = Guid::from(uuid_str);
        assert_eq!(guid_converted, guid_expected);
    }

    #[test]
    fn check_guid_to_uuid_conversion() {
        let uuid_str = "10B201FF-5B3B-45A1-9508-CF3EFCD7BBAF";
        let guid = Guid::from(uuid_str);

        let uuid_converted = to_uuid(&guid);

        let uuid_expected = Uuid::from_str(uuid_str).unwrap();
        assert_eq!(uuid_converted, uuid_expected);
    }
}
