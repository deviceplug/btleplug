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

use super::{ble::watcher::BLEWatcher, peripheral::Peripheral, peripheral::PeripheralId};
use crate::{
    api::{BDAddr, Central, CentralEvent, CentralState, ScanFilter},
    common::adapter_manager::AdapterManager,
    Error, Result,
};
use async_trait::async_trait;
use futures::stream::Stream;
use log::{debug, trace, warn};
use std::convert::TryInto;
use std::fmt::{self, Debug, Formatter};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use uuid::Uuid;
use windows::{
    Devices::Bluetooth::BluetoothLEDevice,
    Devices::Enumeration::DeviceInformation,
    Devices::Radios::{Radio, RadioState},
    Foundation::TypedEventHandler,
};

/// Implementation of [api::Central](crate::api::Central).
#[derive(Clone)]
pub struct Adapter {
    watcher: Arc<Mutex<BLEWatcher>>,
    manager: Arc<AdapterManager<Peripheral>>,
    radio: Radio,
}

// https://github.com/microsoft/windows-rs/blob/master/crates/libs/windows/src/Windows/Devices/Radios/mod.rs
fn get_central_state(radio: &Radio) -> CentralState {
    let state = radio.State().unwrap_or(RadioState::Unknown);
    match state {
        RadioState::On => CentralState::PoweredOn,
        RadioState::Off => CentralState::PoweredOff,
        _ => CentralState::Unknown,
    }
}

impl Adapter {
    pub(crate) fn new(radio: Radio) -> Result<Self> {
        let watcher = Arc::new(Mutex::new(BLEWatcher::new()?));
        let manager = Arc::new(AdapterManager::default());

        let radio_clone = radio.clone();
        let manager_clone = manager.clone();
        let handler = TypedEventHandler::new(move |_sender, _args| {
            let state = get_central_state(&radio_clone);
            manager_clone.emit(CentralEvent::StateUpdate(state.into()));
            Ok(())
        });
        if let Err(err) = radio.StateChanged(&handler) {
            eprintln!("radio.StateChanged error: {}", err);
        }

        Ok(Adapter {
            watcher,
            manager,
            radio,
        })
    }

    pub fn clear_cache(&self) {
        self.manager.clear_peripherals();
    }
}

impl Debug for Adapter {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.debug_struct("Adapter")
            .field("manager", &self.manager)
            .finish()
    }
}

#[async_trait]
impl Central for Adapter {
    type Peripheral = Peripheral;

    async fn events(&self) -> Result<Pin<Box<dyn Stream<Item = CentralEvent> + Send>>> {
        Ok(self.manager.event_stream())
    }

    async fn start_scan(&self, filter: ScanFilter) -> Result<()> {
        let watcher = self.watcher.lock().map_err(Into::<Error>::into)?;
        let manager = self.manager.clone();
        watcher.start(
            filter,
            Box::new(move |args| {
                let bluetooth_address = args.BluetoothAddress()?;
                let address: BDAddr = bluetooth_address.try_into().unwrap();
                if let Some(mut entry) = manager.peripheral_mut(&address.into()) {
                    entry.value_mut().update_properties(args);
                    manager.emit(CentralEvent::DeviceUpdated(address.into()));
                } else {
                    let peripheral = Peripheral::new(Arc::downgrade(&manager), address);
                    peripheral.update_properties(args);
                    manager.add_peripheral(peripheral);
                    manager.emit(CentralEvent::DeviceDiscovered(address.into()));
                }
                Ok(())
            }),
        )
    }

    async fn stop_scan(&self) -> Result<()> {
        let watcher = self.watcher.lock().map_err(Into::<Error>::into)?;
        watcher.stop()?;
        Ok(())
    }

    async fn connected_peripherals(&self, filter: ScanFilter) -> Result<()> {
        let base_selector = BluetoothLEDevice::GetDeviceSelector()
            .map_err(|e| Error::Other(format!("GetDeviceSelector failed: {:?}", e).into()))?;
        let aqs = format!(
            "{} AND System.Devices.Aep.IsConnected:=System.StructuredQueryType.Boolean#True",
            base_selector.to_string()
        );

        // Query all BLE devices that are currently connected to the system
        let devices = DeviceInformation::FindAllAsyncAqsFilter(&windows::core::HSTRING::from(aqs))
            .map_err(|e| Error::Other(format!("FindAllAsyncAqsFilter failed: {:?}", e).into()))?
            .get()
            .map_err(|e| Error::Other(format!("FindAllAsync().get() failed: {:?}", e).into()))?;

        let manager = self.manager.clone();
        let required_services: Vec<Uuid> = filter.services.clone();

        debug!(
            "Scanning for connected peripherals with {} service filters",
            required_services.len()
        );

        // Iterate through each connected device
        for device in devices {
            let device_id = match device.Id() {
                Ok(id) => id,
                Err(e) => {
                    warn!("Failed to get device ID: {:?}", e);
                    continue;
                }
            };
            debug!("Checking connected device: {:?}", device_id);

            // BluetoothLEDevice from the device ID
            let ble_device = match BluetoothLEDevice::FromIdAsync(&device_id) {
                Ok(async_op) => match async_op.get() {
                    Ok(dev) => dev,
                    Err(e) => {
                        warn!("FromIdAsync.get() failed for {:?}: {:?}", device_id, e);
                        continue;
                    }
                },
                Err(e) => {
                    warn!("FromIdAsync failed for {:?}: {:?}", device_id, e);
                    continue;
                }
            };

            // Double-check the connection status
            match ble_device.ConnectionStatus() {
                Ok(status)
                    if status
                        == windows::Devices::Bluetooth::BluetoothConnectionStatus::Connected => {}
                Ok(_) => {
                    trace!("Device {:?} not connected, skipping", device_id);
                    continue;
                }
                Err(e) => {
                    warn!("Failed to get connection status: {:?}", e);
                    continue;
                }
            }

            // Service filtering logic:
            // - If no services specified in filter, accept all connected devices
            // - Otherwise, accept only if the device has at least one matching service
            let mut accept_device = required_services.is_empty();

            if !accept_device {
                // Query the device's GATT services to check for matches
                let services_result = match ble_device.GetGattServicesAsync() {
                    Ok(async_op) => async_op.get(),
                    Err(e) => {
                        warn!("GetGattServicesAsync failed: {:?}", e);
                        continue;
                    }
                };

                let services = match services_result {
                    Ok(gatt_services) => match gatt_services.Services() {
                        Ok(service_list) => service_list,
                        Err(e) => {
                            warn!("Failed to get Services list: {:?}", e);
                            continue;
                        }
                    },
                    Err(e) => {
                        warn!("GetGattServicesAsync.get() failed: {:?}", e);
                        continue;
                    }
                };

                // Check if any of the device's services match the filter
                for service in &services {
                    if let Ok(guid) = service.Uuid() {
                        let service_uuid = Uuid::from_u128(guid.to_u128());
                        if required_services.contains(&service_uuid) {
                            debug!("Found matching service: {:?}", service_uuid);
                            accept_device = true;
                            break;
                        }
                    }
                }
            }

            if !accept_device {
                debug!("Device does not match service filter, skipping");
                continue;
            }

            // Convert Bluetooth address to BDAddr
            let address: BDAddr = match ble_device.BluetoothAddress() {
                Ok(addr) => match (addr as u64).try_into() {
                    Ok(bd_addr) => bd_addr,
                    Err(_) => {
                        warn!("Failed to convert Bluetooth address: {}", addr);
                        continue;
                    }
                },
                Err(e) => {
                    warn!("BluetoothAddress() failed: {:?}", e);
                    continue;
                }
            };

            // Update the peripheral in the manager
            match manager.peripheral_mut(&address.into()) {
                Some(_) => {
                    debug!("Peripheral already exists in manager: {:?}", address);
                    manager.emit(CentralEvent::DeviceDiscovered(address.into()));
                }
                None => {
                    debug!("Adding new peripheral: {:?}", address);
                    let peripheral = Peripheral::new(Arc::downgrade(&manager), address);
                    manager.add_peripheral(peripheral);
                    manager.emit(CentralEvent::DeviceDiscovered(address.into()));
                }
            }
        }

        debug!("Finished scanning for connected peripherals");
        Ok(())
    }

    async fn peripherals(&self) -> Result<Vec<Peripheral>> {
        Ok(self.manager.peripherals())
    }

    async fn peripheral(&self, id: &PeripheralId) -> Result<Peripheral> {
        self.manager.peripheral(id).ok_or(Error::DeviceNotFound)
    }

    async fn add_peripheral(&self, _address: &PeripheralId) -> Result<Peripheral> {
        Err(Error::NotSupported(
            "Can't add a Peripheral from a BDAddr".to_string(),
        ))
    }

    async fn adapter_info(&self) -> Result<String> {
        // TODO: Get information about the adapter.
        Ok("WinRT".to_string())
    }

    async fn adapter_state(&self) -> Result<CentralState> {
        Ok(get_central_state(&self.radio))
    }
}
