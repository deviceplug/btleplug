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

use super::super::bindings;
use crate::{
    api::{Characteristic, WriteType},
    winrtble::utils,
    Error, Result,
};

use bindings::Windows::Devices::Bluetooth::BluetoothCacheMode;
use bindings::Windows::Devices::Bluetooth::GenericAttributeProfile::{
    GattCharacteristic, GattCharacteristicProperties,
    GattClientCharacteristicConfigurationDescriptorValue, GattCommunicationStatus,
    GattValueChangedEventArgs, GattWriteOption,
};
use bindings::Windows::Foundation::{EventRegistrationToken, TypedEventHandler};
use bindings::Windows::Storage::Streams::{DataReader, DataWriter};
use log::{debug, trace};

pub type NotifiyEventHandler = Box<dyn Fn(Vec<u8>) + Send>;

impl Into<GattWriteOption> for WriteType {
    fn into(self) -> GattWriteOption {
        match self {
            WriteType::WithoutResponse => GattWriteOption::WriteWithoutResponse,
            WriteType::WithResponse => GattWriteOption::WriteWithResponse,
        }
    }
}

impl From<GattCharacteristicProperties> for GattClientCharacteristicConfigurationDescriptorValue {
    fn from(properties: GattCharacteristicProperties) -> Self {
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
}

#[derive(Debug)]
pub struct BLECharacteristic {
    characteristic: GattCharacteristic,
    notify_token: Option<EventRegistrationToken>,
}

impl BLECharacteristic {
    pub fn new(characteristic: GattCharacteristic) -> Self {
        BLECharacteristic {
            characteristic,
            notify_token: None,
        }
    }

    pub async fn write_value(&self, data: &[u8], write_type: WriteType) -> Result<()> {
        let writer = DataWriter::new().unwrap();
        writer.WriteBytes(data).unwrap();
        let operation = self
            .characteristic
            .WriteValueWithOptionAsync(writer.DetachBuffer().unwrap(), write_type.into())
            .unwrap();
        let result = operation.await.unwrap();
        if result == GattCommunicationStatus::Success {
            Ok(())
        } else {
            Err(Error::Other(format!(
                "Windows UWP threw error on write: {:?}",
                result
            )))
        }
    }

    pub async fn read_value(&self) -> Result<Vec<u8>> {
        let result = self
            .characteristic
            .ReadValueWithCacheModeAsync(BluetoothCacheMode::Uncached)
            .unwrap()
            .await
            .unwrap();
        if result.Status().unwrap() == GattCommunicationStatus::Success {
            let value = result.Value().unwrap();
            let reader = DataReader::FromBuffer(&value).unwrap();
            let len = reader.UnconsumedBufferLength().unwrap() as usize;
            let mut input = vec![0u8; len];
            reader.ReadBytes(&mut input[0..len]).unwrap();
            Ok(input)
        } else {
            Err(Error::Other(format!(
                "Windows UWP threw error on read: {:?}",
                result
            )))
        }
    }

    pub async fn subscribe(&mut self, on_value_changed: NotifiyEventHandler) -> Result<()> {
        {
            let value_handler = TypedEventHandler::new(
                move |_: &Option<GattCharacteristic>, args: &Option<GattValueChangedEventArgs>| {
                    if let Some(args) = args {
                        let value = args.CharacteristicValue().unwrap();
                        let reader = DataReader::FromBuffer(&value).unwrap();
                        let len = reader.UnconsumedBufferLength().unwrap() as usize;
                        let mut input: Vec<u8> = vec![0u8; len];
                        reader.ReadBytes(&mut input[0..len]).unwrap();
                        trace!("changed {:?}", input);
                        on_value_changed(input);
                    }
                    Ok(())
                },
            );
            let token = self.characteristic.ValueChanged(&value_handler).unwrap();
            self.notify_token = Some(token);
        }
        let config = self.characteristic.CharacteristicProperties()?.into();
        if config == GattClientCharacteristicConfigurationDescriptorValue::None {
            return Err(Error::NotSupported("Can not subscribe to attribute".into()));
        }

        let status = self
            .characteristic
            .WriteClientCharacteristicConfigurationDescriptorAsync(config)
            .unwrap()
            .await
            .unwrap();
        trace!("subscribe {:?}", status);
        if status == GattCommunicationStatus::Success {
            Ok(())
        } else {
            Err(Error::Other(format!(
                "Windows UWP threw error on subscribe: {:?}",
                status
            )))
        }
    }

    pub async fn unsubscribe(&mut self) -> Result<()> {
        if let Some(token) = &self.notify_token {
            self.characteristic.RemoveValueChanged(token).unwrap();
        }
        self.notify_token = None;
        let config = GattClientCharacteristicConfigurationDescriptorValue::None;
        let status = self
            .characteristic
            .WriteClientCharacteristicConfigurationDescriptorAsync(config)
            .unwrap()
            .await
            .unwrap();
        trace!("unsubscribe {:?}", status);
        if status == GattCommunicationStatus::Success {
            Ok(())
        } else {
            Err(Error::Other(format!(
                "Windows UWP threw error on unsubscribe: {:?}",
                status
            )))
        }
    }

    pub fn to_characteristic(&self) -> Characteristic {
        let uuid = utils::to_uuid(&self.characteristic.Uuid().unwrap());
        let properties =
            utils::to_char_props(&self.characteristic.CharacteristicProperties().unwrap());
        Characteristic { uuid, properties }
    }
}

impl Drop for BLECharacteristic {
    fn drop(&mut self) {
        if let Some(token) = &self.notify_token {
            let result = self.characteristic.RemoveValueChanged(token);
            if let Err(err) = result {
                debug!("Drop:remove_connection_status_changed {:?}", err);
            }
        }
    }
}
