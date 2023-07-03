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

use super::super::utils;
use crate::{api::Descriptor, Error, Result};

use uuid::Uuid;
use windows::{
    Devices::Bluetooth::{
        BluetoothCacheMode,
        GenericAttributeProfile::{GattCommunicationStatus, GattDescriptor},
    },
    Storage::Streams::{DataReader, DataWriter},
};

#[derive(Debug)]
pub struct BLEDescriptor {
    descriptor: GattDescriptor,
}

impl BLEDescriptor {
    pub fn new(descriptor: GattDescriptor) -> Self {
        Self { descriptor }
    }

    pub fn uuid(&self) -> Uuid {
        utils::to_uuid(&self.descriptor.Uuid().unwrap())
    }

    pub fn to_descriptor(&self, service_uuid: Uuid, characteristic_uuid: Uuid) -> Descriptor {
        let uuid = self.uuid();
        Descriptor {
            uuid,
            service_uuid,
            characteristic_uuid,
        }
    }

    pub async fn write_value(&self, data: &[u8]) -> Result<()> {
        let writer = DataWriter::new()?;
        writer.WriteBytes(data)?;
        let operation = self.descriptor.WriteValueAsync(&writer.DetachBuffer()?)?;
        let result = operation.await?;
        if result == GattCommunicationStatus::Success {
            Ok(())
        } else {
            Err(Error::Other(
                format!("Windows UWP threw error on write descriptor: {:?}", result).into(),
            ))
        }
    }

    pub async fn read_value(&self) -> Result<Vec<u8>> {
        let result = self
            .descriptor
            .ReadValueWithCacheModeAsync(BluetoothCacheMode::Uncached)?
            .await?;
        if result.Status()? == GattCommunicationStatus::Success {
            let value = result.Value()?;
            let reader = DataReader::FromBuffer(&value)?;
            let len = reader.UnconsumedBufferLength()? as usize;
            let mut input = vec![0u8; len];
            reader.ReadBytes(&mut input[0..len])?;
            Ok(input)
        } else {
            Err(Error::Other(
                format!("Windows UWP threw error on read: {:?}", result).into(),
            ))
        }
    }
}
