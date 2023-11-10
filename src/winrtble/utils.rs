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

use crate::{api::CharPropFlags, Error, Result};
use std::str::FromStr;
use uuid::Uuid;
use windows::core::GUID;
use windows::{
    Devices::Bluetooth::GenericAttributeProfile::{
        GattCharacteristicProperties, GattClientCharacteristicConfigurationDescriptorValue,
        GattCommunicationStatus,
    },
    Storage::Streams::{DataReader, IBuffer},
};

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
        Err(Error::Other("Communication Error:".to_string().into()))
    }
}

pub fn to_descriptor_value(
    properties: GattCharacteristicProperties,
) -> GattClientCharacteristicConfigurationDescriptorValue {
    let notify = GattCharacteristicProperties::Notify;
    let indicate = GattCharacteristicProperties::Indicate;
    if properties & indicate == indicate {
        GattClientCharacteristicConfigurationDescriptorValue::Indicate
    } else if properties & notify == notify {
        GattClientCharacteristicConfigurationDescriptorValue::Notify
    } else {
        GattClientCharacteristicConfigurationDescriptorValue::None
    }
}

pub fn to_uuid(uuid: &GUID) -> Uuid {
    let guid_s = format!("{:?}", uuid);
    Uuid::from_str(&guid_s).unwrap()
}

pub fn to_vec(buffer: &IBuffer) -> Vec<u8> {
    let reader = DataReader::FromBuffer(buffer).unwrap();
    let len = reader.UnconsumedBufferLength().unwrap() as usize;
    let mut data = vec![0u8; len];
    reader.ReadBytes(&mut data).unwrap();
    data
}

#[allow(dead_code)]
pub fn to_guid(uuid: &Uuid) -> GUID {
    let (data1, data2, data3, data4) = uuid.as_fields();
    GUID::from_values(data1, data2, data3, data4.to_owned())
}

pub fn to_char_props(props: &GattCharacteristicProperties) -> CharPropFlags {
    let mut flags = CharPropFlags::default();
    if *props & GattCharacteristicProperties::Broadcast != GattCharacteristicProperties::None {
        flags |= CharPropFlags::BROADCAST;
    }
    if *props & GattCharacteristicProperties::Read != GattCharacteristicProperties::None {
        flags |= CharPropFlags::READ;
    }
    if *props & GattCharacteristicProperties::WriteWithoutResponse
        != GattCharacteristicProperties::None
    {
        flags |= CharPropFlags::WRITE_WITHOUT_RESPONSE;
    }
    if *props & GattCharacteristicProperties::Write != GattCharacteristicProperties::None {
        flags |= CharPropFlags::WRITE;
    }
    if *props & GattCharacteristicProperties::Notify != GattCharacteristicProperties::None {
        flags |= CharPropFlags::NOTIFY;
    }
    if *props & GattCharacteristicProperties::Indicate != GattCharacteristicProperties::None {
        flags |= CharPropFlags::INDICATE;
    }
    if *props & GattCharacteristicProperties::AuthenticatedSignedWrites
        != GattCharacteristicProperties::None
    {
        flags |= CharPropFlags::AUTHENTICATED_SIGNED_WRITES;
    }
    if *props & GattCharacteristicProperties::ExtendedProperties
        != GattCharacteristicProperties::None
    {
        flags |= CharPropFlags::EXTENDED_PROPERTIES;
    }
    flags
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_uuid_to_guid_conversion() {
        let uuid_str = "10B201FF-5B3B-45A1-9508-CF3EFCD7BBAF";
        let uuid = Uuid::from_str(uuid_str).unwrap();

        let guid_converted = to_guid(&uuid);

        let guid_expected = GUID::from(uuid_str);
        assert_eq!(guid_converted, guid_expected);
    }

    #[test]
    fn check_guid_to_uuid_conversion() {
        let uuid_str = "10B201FF-5B3B-45A1-9508-CF3EFCD7BBAF";
        let guid = GUID::from(uuid_str);

        let uuid_converted = to_uuid(&guid);

        let uuid_expected = Uuid::from_str(uuid_str).unwrap();
        assert_eq!(uuid_converted, uuid_expected);
    }
}
