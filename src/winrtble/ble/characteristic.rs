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
    Error, Result,
    api::{Characteristic, WriteType},
    winrtble::utils,
};

use log::{debug, trace};
use std::{collections::HashMap, future::IntoFuture};
use tokio::sync::Mutex;
use uuid::Uuid;
use windows::core::Ref;
use windows::{
    Devices::Bluetooth::{
        BluetoothCacheMode,
        GenericAttributeProfile::{
            GattCharacteristic, GattClientCharacteristicConfigurationDescriptorValue,
            GattCommunicationStatus, GattValueChangedEventArgs, GattWriteOption,
        },
    },
    Foundation::TypedEventHandler,
    Storage::Streams::{DataReader, DataWriter},
};

pub type NotifiyEventHandler = Box<dyn Fn(Vec<u8>) + Send>;

impl From<WriteType> for GattWriteOption {
    fn from(val: WriteType) -> Self {
        match val {
            WriteType::WithoutResponse => GattWriteOption::WriteWithoutResponse,
            WriteType::WithResponse => GattWriteOption::WriteWithResponse,
        }
    }
}

#[derive(Debug)]
pub struct BLECharacteristic {
    characteristic: GattCharacteristic,
    pub descriptors: HashMap<Uuid, BLEDescriptor>,
    notify_token: Mutex<Option<i64>>,
}

impl BLECharacteristic {
    pub fn new(
        characteristic: GattCharacteristic,
        descriptors: HashMap<Uuid, BLEDescriptor>,
    ) -> Self {
        BLECharacteristic {
            characteristic,
            descriptors,
            notify_token: Mutex::new(None),
        }
    }

    pub async fn write_value(&self, data: &[u8], write_type: WriteType) -> Result<()> {
        let writer = DataWriter::new()?;
        writer.WriteBytes(data)?;
        let operation = self
            .characteristic
            .WriteValueWithOptionAsync(&writer.DetachBuffer()?, write_type.into())?;
        let result = operation.into_future().await?;
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
            .into_future()
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

    fn remove_notify_handler(&self, notify_token: &mut Option<i64>) -> Result<()> {
        if let Some(token) = *notify_token {
            // Only relinquish ownership after WinRT confirms removal. This keeps
            // the token available for a later retry when removal fails.
            self.characteristic.RemoveValueChanged(token)?;
            *notify_token = None;
        }
        Ok(())
    }

    pub async fn subscribe(&self, on_value_changed: NotifiyEventHandler) -> Result<()> {
        // Held across the CCCD write to serialize subscribe/unsubscribe per characteristic.
        let mut notify_token = self.notify_token.lock().await;

        // Validate before changing the existing subscription state.
        let config = to_descriptor_value(self.characteristic.CharacteristicProperties()?);
        if config == GattClientCharacteristicConfigurationDescriptorValue::None {
            return Err(Error::NotSupported("Can not subscribe to attribute".into()));
        }

        // A replacement is allowed, but never leave two handlers installed. If
        // removal fails, retain the old token and reject the replacement.
        self.remove_notify_handler(&mut notify_token)?;

        let token = {
            let value_handler = TypedEventHandler::new(
                move |_: Ref<GattCharacteristic>, args: Ref<GattValueChangedEventArgs>| {
                    if let Ok(args) = args.ok() {
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
            self.characteristic.ValueChanged(&value_handler)?
        };
        *notify_token = Some(token);

        let status = match self
            .characteristic
            .WriteClientCharacteristicConfigurationDescriptorAsync(config)
        {
            Ok(operation) => operation.into_future().await,
            Err(err) => {
                let _ = self.remove_notify_handler(&mut notify_token);
                return Err(err.into());
            }
        };
        let status = match status {
            Ok(status) => status,
            Err(err) => {
                let _ = self.remove_notify_handler(&mut notify_token);
                return Err(err.into());
            }
        };
        trace!("subscribe {:?}", status);
        if status == GattCommunicationStatus::Success {
            Ok(())
        } else {
            let _ = self.remove_notify_handler(&mut notify_token);
            Err(Error::Other(
                format!("Windows UWP threw error on subscribe: {:?}", status).into(),
            ))
        }
    }

    pub async fn unsubscribe(&self) -> Result<()> {
        let mut notify_token = self.notify_token.lock().await;

        // Disable the CCCD first. If that fails, retain the token and handler so
        // ownership is still available for a later cleanup retry.
        let config = GattClientCharacteristicConfigurationDescriptorValue::None;
        let status = self
            .characteristic
            .WriteClientCharacteristicConfigurationDescriptorAsync(config)?
            .into_future()
            .await?;
        trace!("unsubscribe {:?}", status);
        if status != GattCommunicationStatus::Success {
            return Err(Error::Other(
                format!("Windows UWP threw error on unsubscribe: {:?}", status).into(),
            ));
        }

        // Keep the token if removal fails; the next unsubscribe (or Drop) can retry.
        self.remove_notify_handler(&mut notify_token)
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
        if let Some(token) = *self.notify_token.get_mut() {
            let result = self.characteristic.RemoveValueChanged(token);
            if let Err(err) = result {
                debug!("Drop:remove_connection_status_changed {:?}", err);
            }
        }
    }
}
