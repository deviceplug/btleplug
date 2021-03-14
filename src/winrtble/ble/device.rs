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
use crate::{api::BDAddr, winrtble::utils, Error, Result};
use bindings::windows::devices::bluetooth::generic_attribute_profile::{
    GattCharacteristic, GattCommunicationStatus, GattDeviceService, GattDeviceServicesResult,
};
use bindings::windows::devices::bluetooth::{BluetoothConnectionStatus, BluetoothLEDevice};
use bindings::windows::foundation::{EventRegistrationToken, TypedEventHandler};
use log::{debug, error, trace};

pub type ConnectedEventHandler = Box<dyn Fn(bool) + Send>;

pub struct BLEDevice {
    device: BluetoothLEDevice,
    connection_token: EventRegistrationToken,
}

unsafe impl Send for BLEDevice {}
unsafe impl Sync for BLEDevice {}

impl BLEDevice {
    pub async fn new(
        address: BDAddr,
        connection_status_changed: ConnectedEventHandler,
    ) -> Result<Self> {
        let async_op = BluetoothLEDevice::from_bluetooth_address_async(address.into())
            .map_err(|_| Error::DeviceNotFound)?;
        let device = async_op.await.map_err(|_| Error::DeviceNotFound)?;
        let connection_status_handler = TypedEventHandler::new(
            move |sender: &Option<BluetoothLEDevice>, _: &Option<windows::Object>| {
                if let Some(sender) = sender {
                    let is_connected = sender
                        .connection_status()
                        .ok()
                        .map_or(false, |v| v == BluetoothConnectionStatus::Connected);
                    connection_status_changed(is_connected);
                    trace!("state {:?}", sender.connection_status());
                }

                Ok(())
            },
        );
        let connection_token = device
            .connection_status_changed(&connection_status_handler)
            .map_err(|_| Error::Other("Could not add connection status handler".into()))?;

        Ok(BLEDevice {
            device,
            connection_token,
        })
    }

    async fn get_gatt_services(&self) -> Result<GattDeviceServicesResult> {
        let winrt_error = |e| Error::Other(format!("{:?}", e));
        let async_op = self.device.get_gatt_services_async().map_err(winrt_error)?;
        let service_result = async_op.await.map_err(winrt_error)?;
        Ok(service_result)
    }

    pub async fn connect(&self) -> Result<()> {
        let service_result = self.get_gatt_services().await?;
        let status = service_result.status().map_err(|_| Error::DeviceNotFound)?;
        utils::to_error(status)
    }

    async fn get_characteristics(
        &self,
        service: &GattDeviceService,
    ) -> std::result::Result<Vec<GattCharacteristic>, windows::Error> {
        let operation = service.get_characteristics_async()?;
        let async_result = operation.await?;
        let status = async_result.status();
        if status == Ok(GattCommunicationStatus::Success) {
            let results = async_result.characteristics()?;
            debug!("characteristics {:?}", results.size());
            Ok(results.into_iter().collect())
        } else {
            trace!("get_status {:?}", status);
            Ok(vec![])
        }
    }

    pub async fn discover_characteristics(&self) -> Result<Vec<GattCharacteristic>> {
        let winrt_error = |e| Error::Other(format!("{:?}", e));
        let service_result = self.get_gatt_services().await?;
        let status = service_result.status().map_err(winrt_error)?;
        if status == GattCommunicationStatus::Success {
            let mut characteristics = Vec::new();
            // We need to convert the IVectorView to a Vec, because IVectorView is not Send and so
            // can't be help past the await point below.
            let services: Vec<_> = service_result
                .services()
                .map_err(winrt_error)?
                .into_iter()
                .collect();
            debug!("services {:?}", services.len());
            for service in &services {
                match self.get_characteristics(&service).await {
                    Ok(mut service_characteristics) => {
                        characteristics.append(&mut service_characteristics);
                    }
                    Err(e) => {
                        error!("get_characteristics_async {:?}", e);
                    }
                }
            }
            return Ok(characteristics);
        }
        Ok(Vec::new())
    }
}

impl Drop for BLEDevice {
    fn drop(&mut self) {
        let result = self
            .device
            .remove_connection_status_changed(&self.connection_token);
        if let Err(err) = result {
            debug!("Drop:remove_connection_status_changed {:?}", err);
        }
    }
}
