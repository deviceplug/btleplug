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

use super::{super::utils::to_descriptor_value, descriptor::BLEDescriptor};
use crate::{
    api::{Characteristic, WriteType},
    winrtble::utils,
    Error, Result,
};

use log::{debug, trace};
use std::collections::HashMap;
use uuid::Uuid;
use windows::{
    Devices::Bluetooth::{
        BluetoothCacheMode,
        GenericAttributeProfile::{
            GattCharacteristic, GattClientCharacteristicConfigurationDescriptorValue,
            GattCommunicationStatus, GattValueChangedEventArgs, GattWriteOption,
        },
    },
    Foundation::{EventRegistrationToken, TypedEventHandler},
    Storage::Streams::{DataReader, DataWriter},
};

pub type NotifiyEventHandler = Box<dyn Fn(Vec<u8>) + Send>;

impl Into<GattWriteOption> for WriteType {
    fn into(self) -> GattWriteOption {
        match self {
            WriteType::WithoutResponse => GattWriteOption::WriteWithoutResponse,
            WriteType::WithResponse => GattWriteOption::WriteWithResponse,
        }
    }
}

#[derive(Debug)]
pub struct BLECharacteristic {
    characteristic: GattCharacteristic,
    descriptors: HashMap<Uuid, BLEDescriptor>,
    notify_token: Option<EventRegistrationToken>,
}

impl BLECharacteristic {
    pub fn new(
        characteristic: GattCharacteristic,
        descriptors: HashMap<Uuid, BLEDescriptor>,
    ) -> Self {
        BLECharacteristic {
            characteristic,
            descriptors,
            notify_token: None,
        }
    }

    pub async fn write_value(&self, data: &[u8], write_type: WriteType) -> Result<()> {
        let writer = DataWriter::new()?;
        writer.WriteBytes(data)?;
        let operation = self
            .characteristic
            .WriteValueWithOptionAsync(&writer.DetachBuffer()?, write_type.into())?;
        let result = operation.await?;
        if result == GattCommunicationStatus::Success {
            Ok(())
        } else {
            Err(Error::Other(
                format!("Windows UWP threw error on write: {:?}", result).into(),
            ))
        }
    }

    pub async fn read_value(&self) -> Result<Vec<u8>> {
        let result = self
            .characteristic
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

    pub async fn subscribe(&mut self, on_value_changed: NotifiyEventHandler) -> Result<()> {
        {
            let value_handler = TypedEventHandler::new(
                move |_: &Option<GattCharacteristic>, args: &Option<GattValueChangedEventArgs>| {
                    if let Some(args) = args {
                        let value = args.CharacteristicValue()?;
                        let reader = DataReader::FromBuffer(&value)?;
                        let len = reader.UnconsumedBufferLength()? as usize;
                        let mut input: Vec<u8> = vec![0u8; len];
                        reader.ReadBytes(&mut input[0..len])?;
                        trace!("changed {:?}", input);
                        on_value_changed(input);
                    }
                    Ok(())
                },
            );
            let token = self.characteristic.ValueChanged(&value_handler)?;
            self.notify_token = Some(token);
        }
        let config = to_descriptor_value(self.characteristic.CharacteristicProperties()?);
        if config == GattClientCharacteristicConfigurationDescriptorValue::None {
            return Err(Error::NotSupported("Can not subscribe to attribute".into()));
        }

        let status = self
            .characteristic
            .WriteClientCharacteristicConfigurationDescriptorAsync(config)?
            .await?;
        trace!("subscribe {:?}", status);
        if status == GattCommunicationStatus::Success {
            Ok(())
        } else {
            Err(Error::Other(
                format!("Windows UWP threw error on subscribe: {:?}", status).into(),
            ))
        }
    }

    pub async fn unsubscribe(&mut self) -> Result<()> {
        if let Some(token) = &self.notify_token {
            self.characteristic.RemoveValueChanged(*token)?;
        }
        self.notify_token = None;
        let config = GattClientCharacteristicConfigurationDescriptorValue::None;
        let status = self
            .characteristic
            .WriteClientCharacteristicConfigurationDescriptorAsync(config)?
            .await?;
        trace!("unsubscribe {:?}", status);
        if status == GattCommunicationStatus::Success {
            Ok(())
        } else {
            Err(Error::Other(
                format!("Windows UWP threw error on unsubscribe: {:?}", status).into(),
            ))
        }
    }

    pub fn uuid(&self) -> Uuid {
        utils::to_uuid(&self.characteristic.Uuid().unwrap())
    }

    pub fn to_characteristic(&self, service_uuid: Uuid) -> Characteristic {
        let uuid = self.uuid();
        let properties =
            utils::to_char_props(&self.characteristic.CharacteristicProperties().unwrap());
        let descriptors = self
            .descriptors
            .values()
            .map(|descriptor| descriptor.to_descriptor(service_uuid, uuid))
            .collect();
        Characteristic {
            uuid,
            service_uuid,
            descriptors,
            properties,
        }
    }
}

impl Drop for BLECharacteristic {
    fn drop(&mut self) {
        if let Some(token) = &self.notify_token {
            let result = self.characteristic.RemoveValueChanged(*token);
            if let Err(err) = result {
                debug!("Drop:remove_connection_status_changed {:?}", err);
            }
        }
    }
}
