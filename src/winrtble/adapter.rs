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
    Error, Result,
    api::{
        self, BDAddr, Central, CentralEvent, CentralState, RetrievePeripheralsOptions, ScanFilter,
    },
    common::adapter_manager::AdapterManager,
};
use async_trait::async_trait;
use futures::stream::Stream;
use std::convert::TryFrom;
use std::fmt::{self, Debug, Formatter};
use std::future::IntoFuture;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use windows::{
    Devices::{
        Bluetooth::{
            BluetoothAdapter, BluetoothCacheMode, BluetoothLEDevice,
            GenericAttributeProfile::GattCommunicationStatus,
        },
        Enumeration::DeviceInformation,
        Radios::{Radio, RadioState},
    },
    Foundation::TypedEventHandler,
};

/// Implementation of [api::Central](crate::api::Central).
#[derive(Clone)]
pub struct Adapter {
    watcher: Arc<Mutex<BLEWatcher>>,
    manager: Arc<AdapterManager<Peripheral>>,
    radio: Radio,
    bluetooth_adapter: BluetoothAdapter,
}

// https://github.com/microsoft/windows-rs/blob/master/crates/libs/windows/src/Windows/Devices/Radios/mod.rs
fn winrt_error<E: std::fmt::Debug>(error: E) -> Error {
    Error::Other(format!("{error:?}").into())
}

fn checked_address(value: u64) -> Result<BDAddr> {
    BDAddr::try_from(value).map_err(Error::from)
}

fn get_central_state(radio: &Radio) -> CentralState {
    let state = radio.State().unwrap_or(RadioState::Unknown);
    match state {
        RadioState::On => CentralState::PoweredOn,
        RadioState::Off => CentralState::PoweredOff,
        _ => CentralState::Unknown,
    }
}

impl Adapter {
    pub(crate) fn new(bluetooth_adapter: BluetoothAdapter, radio: Radio) -> Result<Self> {
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
            bluetooth_adapter,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_address_rejects_values_outside_six_bytes() {
        assert!(checked_address(0x11_22_33_44_55_66_77).is_err());
    }

    #[test]
    fn checked_address_preserves_windows_address_order() {
        assert_eq!(
            checked_address(0x11_22_33_44_55_66).unwrap().into_inner(),
            [0x11, 0x22, 0x33, 0x44, 0x55, 0x66]
        );
    }

    #[test]
    fn retrieve_selector_union_matches_identifier_or_service() {
        let id = PeripheralId::from(BDAddr::from([1, 2, 3, 4, 5, 6]));
        let options = RetrievePeripheralsOptions {
            identifiers: Some(vec![id.clone()]),
            services: Some(vec![uuid::Uuid::nil()]),
        };
        assert!(api::matches_retrieval_selectors(&id, &[], &options));
        assert!(api::matches_retrieval_selectors(
            &PeripheralId::from(BDAddr::from([6, 5, 4, 3, 2, 1])),
            &[uuid::Uuid::nil()],
            &options
        ));
    }

    #[test]
    fn retrieve_selector_empty_values_match_nothing() {
        let id = PeripheralId::from(BDAddr::from([1, 2, 3, 4, 5, 6]));
        let options = RetrievePeripheralsOptions {
            identifiers: Some(vec![]),
            services: None,
        };
        assert!(!api::matches_retrieval_selectors(&id, &[], &options));
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
        let mut watcher = self.watcher.lock().map_err(Into::<Error>::into)?;
        let manager = self.manager.clone();
        watcher.start(
            filter,
            Box::new(move |args| {
                let bluetooth_address = args.BluetoothAddress()?;
                let address = checked_address(bluetooth_address).map_err(|error| {
                    windows::core::Error::new(
                        windows::core::HRESULT::from_win32(87),
                        error.to_string(),
                    )
                })?;
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
        let mut watcher = self.watcher.lock().map_err(Into::<Error>::into)?;
        watcher.stop()?;
        Ok(())
    }

    async fn peripherals(&self) -> Result<Vec<Peripheral>> {
        Ok(self.manager.peripherals())
    }

    /// Retrieves connected BLE devices from the Windows device enumeration service.
    ///
    /// WinRT's connected-device selector is system-wide and cannot be restricted to
    /// this `Radio`; callers must treat results as belonging to the Windows BLE
    /// subsystem rather than to one physical adapter when multiple radios exist.
    async fn retrieve_peripherals(
        &self,
        options: RetrievePeripheralsOptions,
    ) -> Result<Vec<Peripheral>> {
        // Identifier-only retrieval must not use the connected-device selector: it is
        // intentionally independent of enumeration, and preserves the requested ID order.
        if options.identifiers.is_some() && options.services.is_none() {
            let mut result = Vec::new();
            for requested_id in options.identifiers.as_deref().unwrap_or_default() {
                let async_operation = match BluetoothLEDevice::FromBluetoothAddressAsync(
                    requested_id.address().into(),
                ) {
                    Ok(async_operation) => async_operation,
                    // Unknown cached IDs are omitted, not errors.
                    Err(_) => continue,
                };
                let device = match async_operation.into_future().await {
                    Ok(device) => device,
                    // Disconnected cached IDs are omitted, not errors.
                    Err(_) => continue,
                };
                if device.ConnectionStatus().map_err(winrt_error)?
                    != windows::Devices::Bluetooth::BluetoothConnectionStatus::Connected
                {
                    continue;
                }
                let address = checked_address(device.BluetoothAddress().map_err(winrt_error)?)?;
                let peripheral = self
                    .manager
                    .peripheral(&PeripheralId::from(address))
                    .unwrap_or_else(|| {
                        let peripheral = Peripheral::new(Arc::downgrade(&self.manager), address);
                        self.manager.add_peripheral(peripheral.clone());
                        peripheral
                    });
                result.push(peripheral);
            }
            return Ok(api::merge_retrieved_peripherals(result, |peripheral| {
                crate::api::Peripheral::id(peripheral)
            }));
        }

        // Service and combined retrieval use WinRT's connected-device enumeration.
        let selector = BluetoothLEDevice::GetDeviceSelectorFromConnectionStatus(
            windows::Devices::Bluetooth::BluetoothConnectionStatus::Connected,
        )
        .map_err(winrt_error)?;
        let devices = DeviceInformation::FindAllAsyncAqsFilter(&selector)
            .map_err(winrt_error)?
            .into_future()
            .await
            .map_err(winrt_error)?
            .into_iter()
            .collect::<Vec<_>>();
        let mut result = Vec::new();

        for info in devices {
            let id = info.Id().map_err(winrt_error)?;
            let device = BluetoothLEDevice::FromIdAsync(&id)
                .map_err(winrt_error)?
                .into_future()
                .await
                .map_err(winrt_error)?;
            let address = checked_address(device.BluetoothAddress().map_err(winrt_error)?)?;
            let candidate_id = PeripheralId::from(address);

            let service_result = device
                .GetGattServicesWithCacheModeAsync(BluetoothCacheMode::Cached)
                .map_err(winrt_error)?
                .into_future()
                .await
                .map_err(winrt_error)?;
            let service_uuids = if service_result.Status().map_err(winrt_error)?
                == GattCommunicationStatus::Success
            {
                service_result
                    .Services()
                    .map_err(winrt_error)?
                    .into_iter()
                    .map(|service| {
                        service
                            .Uuid()
                            .map(|uuid| crate::winrtble::utils::to_uuid(&uuid))
                    })
                    .collect::<windows::core::Result<Vec<_>>>()
                    .map_err(winrt_error)?
            } else {
                Vec::new()
            };
            if !api::matches_retrieval_selectors(&candidate_id, &service_uuids, &options) {
                continue;
            }
            let peripheral = self.manager.peripheral(&candidate_id).unwrap_or_else(|| {
                let peripheral = Peripheral::new(Arc::downgrade(&self.manager), address);
                self.manager.add_peripheral(peripheral.clone());
                peripheral
            });
            result.push(peripheral);
        }
        Ok(api::merge_retrieved_peripherals(result, |peripheral| {
            crate::api::Peripheral::id(peripheral)
        }))
    }

    async fn peripheral(&self, id: &PeripheralId) -> Result<Peripheral> {
        self.manager.peripheral(id).ok_or(Error::DeviceNotFound)
    }

    async fn add_peripheral(&self, _address: &PeripheralId) -> Result<Peripheral> {
        Err(Error::NotSupported(
            "Can't add a Peripheral from a BDAddr".to_string(),
        ))
    }

    async fn clear_peripherals(&self) -> Result<()> {
        self.manager.clear_peripherals();
        Ok(())
    }

    async fn adapter_info(&self) -> Result<String> {
        // TODO: Get information about the adapter.
        Ok("WinRT".to_string())
    }

    async fn adapter_address(&self) -> Result<Option<BDAddr>> {
        let bluetooth_address = self.bluetooth_adapter.BluetoothAddress().map_err(|error| {
            Error::Other(format!("Could not get Bluetooth adapter address: {error:?}").into())
        })?;
        if bluetooth_address == 0 {
            return Ok(None);
        }
        let address: BDAddr = bluetooth_address.try_into()?;
        Ok(Some(address))
    }

    async fn adapter_state(&self) -> Result<CentralState> {
        Ok(get_central_state(&self.radio))
    }
}
