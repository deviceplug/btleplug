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

use crate::{api::BDAddr, winrtble::utils, Error, Result};
use winrt::{
    windows::devices::bluetooth::genericattributeprofile::{
        GattCharacteristic, GattCommunicationStatus, GattDeviceService, GattDeviceServicesResult,
        IGattDeviceService3,
    },
    windows::devices::bluetooth::{
        BluetoothConnectionStatus, BluetoothLEDevice, IBluetoothLEDevice3,
    },
    windows::foundation::{EventRegistrationToken, TypedEventHandler},
    ComPtr, IInspectable, RtAsyncOperation,
};

pub type ConnectedEventHandler = Box<dyn Fn(bool) + Send>;

pub struct BLEDevice {
    device: ComPtr<BluetoothLEDevice>,
    connection_token: EventRegistrationToken,
}

unsafe impl Send for BLEDevice {}
unsafe impl Sync for BLEDevice {}

impl BLEDevice {
    pub fn new(address: BDAddr, connection_status_changed: ConnectedEventHandler) -> Result<Self> {
        let async_op = BluetoothLEDevice::from_bluetooth_address_async(utils::to_address(address))
            .map_err(|_| Error::DeviceNotFound)?;
        let device = async_op
            .blocking_get()
            .map_err(|_| Error::DeviceNotFound)?
            .ok_or(Error::DeviceNotFound)?;
        let connection_status_handler = TypedEventHandler::new(
            move |sender: *mut BluetoothLEDevice, _: *mut IInspectable| {
                let sender = unsafe { &*sender };
                let is_connected = sender
                    .get_connection_status()
                    .ok()
                    .map_or(false, |v| v == BluetoothConnectionStatus::Connected);
                connection_status_changed(is_connected);
                info!("state {:?}", sender.get_connection_status());
                Ok(())
            },
        );
        let connection_token = device
            .add_connection_status_changed(&connection_status_handler)
            .map_err(|_| Error::Other("Could not add connection status handler".into()))?;

        Ok(BLEDevice {
            device,
            connection_token,
        })
    }

    fn get_gatt_services(&self) -> Result<ComPtr<GattDeviceServicesResult>> {
        let winrt_error = |e| Error::Other(format!("{:?}", e));
        let device3 = self
            .device
            .query_interface::<IBluetoothLEDevice3>()
            .ok_or_else(|| Error::NotSupported("Interface not implemented".into()))?;
        let async_op = device3.get_gatt_services_async().map_err(winrt_error)?;
        let service_result = async_op
            .blocking_get()
            .map_err(winrt_error)?
            .ok_or_else(|| Error::NotSupported("Interface not implemented".into()))?;
        Ok(service_result)
    }

    pub fn connect(&self) -> Result<()> {
        let service_result = self.get_gatt_services()?;
        let status = service_result
            .get_status()
            .map_err(|_| Error::DeviceNotFound)?;
        utils::to_error(status)
    }

    fn get_characteristics(
        &self,
        service: &ComPtr<GattDeviceService>,
    ) -> Vec<ComPtr<GattCharacteristic>> {
        let mut characteristics = Vec::new();
        let service3 = service.query_interface::<IGattDeviceService3>();
        if let Some(service3) = service3 {
            let async_result = service3
                .get_characteristics_async()
                .and_then(|ao| ao.blocking_get());
            match async_result {
                Ok(Some(async_result)) => match async_result.get_status() {
                    Ok(GattCommunicationStatus::Success) => {
                        match async_result.get_characteristics() {
                            Ok(Some(results)) => {
                                info!("characteristics {:?}", results.get_size());
                                for characteristic in &results {
                                    if let Some(characteristic) = characteristic {
                                        characteristics.push(characteristic);
                                    } else {
                                        info!("null pointer for characteristic");
                                    }
                                }
                            }
                            Ok(None) => {
                                info!("null pointer from get_characteristics");
                            }
                            Err(error) => {
                                info!("get_characteristics {:?}", error);
                            }
                        }
                    }
                    rest => {
                        info!("get_status {:?}", rest);
                    }
                },
                Ok(None) => {
                    info!("null pointer from get_characteristics");
                }
                Err(error) => {
                    info!("get_characteristics_async {:?}", error);
                }
            }
        }
        characteristics
    }

    pub fn discover_characteristics(&self) -> Result<Vec<ComPtr<GattCharacteristic>>> {
        let winrt_error = |e| Error::Other(format!("{:?}", e));
        let service_result = self.get_gatt_services()?;
        let status = service_result.get_status().map_err(winrt_error)?;
        if status == GattCommunicationStatus::Success {
            let mut characteristics = Vec::new();
            if let Some(services) = service_result.get_services().map_err(winrt_error)? {
                info!("services {:?}", services.get_size());
                for service in &services {
                    if let Some(service) = service {
                        characteristics.append(&mut self.get_characteristics(&service));
                    } else {
                        info!("null pointer for service");
                    }
                }
            } else {
                info!("null pointer from get_services()");
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
            .remove_connection_status_changed(self.connection_token);
        if let Err(err) = result {
            info!("Drop:remove_connection_status_changed {:?}", err);
        }
    }
}
