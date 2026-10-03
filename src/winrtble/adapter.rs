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
use futures::future::join_all;
use futures::stream::Stream;
use std::convert::TryFrom;
use std::fmt::{self, Debug, Formatter};
use std::future::IntoFuture;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::timeout;
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
    _state_handler: Option<Arc<RadioStateHandler>>,
}

struct RadioStateHandler {
    radio: Radio,
    token: i64,
}

impl Drop for RadioStateHandler {
    fn drop(&mut self) {
        if let Err(err) = self.radio.RemoveStateChanged(self.token) {
            log::warn!("Failed to remove Bluetooth radio state handler: {err}");
        }
    }
}

/// Timeout for a single device's Cached-mode GATT service lookup during
/// [`Adapter::retrieve_peripherals`]'s service/combined path.
///
/// WinRT's system-wide "connected" device selector can keep reporting a device as
/// connected long after it has gone out of range; `GetGattServicesWithCacheModeAsync`
/// on such a stale device has been observed to hang for tens of seconds (~51s on real
/// hardware) instead of returning `Unreachable` promptly. A device that does not
/// answer within this window is treated the same as one that returned a non-`Success`
/// status: no services, so it cannot match a service filter. Kept separate from
/// `ble::device::GATT_CACHE_TIMEOUT` (#325), which bounds a different, uncached call.
const RETRIEVE_GATT_TIMEOUT: Duration = Duration::from_secs(5);

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
        let coded_phy_supported = bluetooth_adapter
            .IsLowEnergyCodedPhySupported()
            .unwrap_or(false);
        let watcher = Arc::new(Mutex::new(BLEWatcher::new(coded_phy_supported)?));
        let manager = Arc::new(AdapterManager::default());

        let manager_weak = Arc::downgrade(&manager);
        let handler =
            TypedEventHandler::<Radio, windows::core::IInspectable>::new(move |sender, _args| {
                if let Some(manager) = manager_weak.upgrade() {
                    manager.emit(CentralEvent::StateUpdate(get_central_state(sender.ok()?)));
                }
                Ok(())
            });
        let state_handler = match radio.StateChanged(&handler) {
            Ok(token) => Some(Arc::new(RadioStateHandler {
                radio: radio.clone(),
                token,
            })),
            Err(err) => {
                log::warn!("Failed to register Bluetooth radio state handler: {err}");
                None
            }
        };

        Ok(Adapter {
            watcher,
            manager,
            radio,
            bluetooth_adapter,
            _state_handler: state_handler,
        })
    }

    /// Sets whether scans also receive advertisements on the Coded (long-range) PHY.
    /// Has no effect on adapters that do not report Coded PHY support, where requesting
    /// it would stop the scan from reporting anything. Some adapters that do report
    /// support stop delivering most 1M PHY advertisements while it is enabled. Takes
    /// effect on the next [`Central::start_scan`].
    pub fn set_use_coded_phy(&self, enabled: bool) -> Result<()> {
        let mut watcher = self.watcher.lock().map_err(Into::<Error>::into)?;
        watcher.set_use_coded_phy(enabled);
        Ok(())
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
                    let peripheral = manager.add_peripheral(peripheral);
                    peripheral.update_properties(args);
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
    /// For service and combined lookups, a device whose GATT services cannot be read
    /// within 5 seconds is treated as having no services, so it cannot match the
    /// service filter (it can still match by identifier in a combined lookup).
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
                        self.manager.add_peripheral(peripheral)
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

        // Look up devices concurrently; each lookup is bounded by RETRIEVE_GATT_TIMEOUT,
        // so stale devices cost about one timeout in total.
        let manager = &self.manager;
        let options = &options;
        let device_futures = devices.into_iter().map(|info| async move {
            let id = info.Id().map_err(winrt_error)?;
            let device = BluetoothLEDevice::FromIdAsync(&id)
                .map_err(winrt_error)?
                .into_future()
                .await
                .map_err(winrt_error)?;
            let address = checked_address(device.BluetoothAddress().map_err(winrt_error)?)?;
            let candidate_id = PeripheralId::from(address);

            let service_op = device
                .GetGattServicesWithCacheModeAsync(BluetoothCacheMode::Cached)
                .map_err(winrt_error)?;
            let service_uuids = match timeout(
                RETRIEVE_GATT_TIMEOUT,
                service_op.clone().into_future(),
            )
            .await
            {
                Ok(service_result) => {
                    let service_result = service_result.map_err(winrt_error)?;
                    if service_result.Status().map_err(winrt_error)?
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
                    }
                }
                Err(_) => {
                    log::debug!(
                        "GATT service lookup for {candidate_id:?} did not complete within \
                         {RETRIEVE_GATT_TIMEOUT:?}; treating it as having no services"
                    );
                    if let Err(cancel_err) = service_op.Cancel() {
                        log::warn!(
                            "Failed to cancel timed-out GATT service lookup for {candidate_id:?}: \
                             {cancel_err:?}"
                        );
                    }
                    Vec::new()
                }
            };
            if !api::matches_retrieval_selectors(&candidate_id, &service_uuids, options) {
                return Ok(None);
            }
            let peripheral = manager.peripheral(&candidate_id).unwrap_or_else(|| {
                let peripheral = Peripheral::new(Arc::downgrade(manager), address);
                manager.add_peripheral(peripheral)
            });
            Ok::<_, Error>(Some(peripheral))
        });

        let mut result = Vec::new();
        for outcome in join_all(device_futures).await {
            if let Some(peripheral) = outcome? {
                result.push(peripheral);
            }
        }
        Ok(api::merge_retrieved_peripherals(result, |peripheral| {
            crate::api::Peripheral::id(peripheral)
        }))
    }

    async fn peripheral(&self, id: &PeripheralId) -> Result<Peripheral> {
        self.manager.peripheral(id).ok_or(Error::DeviceNotFound)
    }

    async fn add_peripheral(&self, id: &PeripheralId) -> Result<Peripheral> {
        if let Some(peripheral) = self.manager.peripheral(id) {
            return Ok(peripheral);
        }
        // Create a peripheral straight from its address so a device the OS already knows (bonded or
        // connected to another central) can be reached without waiting for an advertisement.
        let peripheral = Peripheral::new(Arc::downgrade(&self.manager), id.clone().into());
        Ok(self.manager.add_peripheral(peripheral))
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

#[cfg(test)]
mod cleanup_tests {
    use super::*;
    use crate::api::Manager as _;

    #[tokio::test]
    #[ignore = "requires a Windows Bluetooth radio"]
    async fn dropping_last_radio_handler_unregisters_callback() {
        let manager = crate::platform::Manager::new().await.unwrap();
        let adapter = manager
            .adapters()
            .await
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        let owner = Arc::new(());
        let captured = owner.clone();
        let handler = TypedEventHandler::new(move |_sender, _args| {
            let _ = &captured;
            Ok(())
        });
        let token = adapter.radio.StateChanged(&handler).unwrap();
        let registration = Arc::new(RadioStateHandler {
            radio: adapter.radio.clone(),
            token,
        });
        drop(handler);
        let clone = registration.clone();
        drop(registration);
        assert_eq!(Arc::strong_count(&owner), 2);
        drop(clone);
        assert_eq!(Arc::strong_count(&owner), 1);
    }

    #[tokio::test]
    #[ignore = "requires a Windows Bluetooth radio"]
    async fn dropping_last_adapter_releases_manager() {
        let manager = crate::platform::Manager::new().await.unwrap();
        let adapter = manager
            .adapters()
            .await
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        let weak = Arc::downgrade(&adapter.manager);
        let clone = adapter.clone();
        let events = adapter.events().await.unwrap();
        drop(adapter);
        assert!(weak.upgrade().is_some());
        drop(clone);
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert!(weak.upgrade().is_none());
        drop(events);
    }

    #[tokio::test]
    #[ignore = "requires a Windows Bluetooth radio"]
    async fn stopped_scan_releases_manager_after_adapter_drop() {
        let manager = crate::platform::Manager::new().await.unwrap();
        for _ in 0..8 {
            let adapter = manager
                .adapters()
                .await
                .unwrap()
                .into_iter()
                .next()
                .unwrap();
            let weak = Arc::downgrade(&adapter.manager);
            adapter.start_scan(ScanFilter::default()).await.unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            adapter.stop_scan().await.unwrap();
            drop(adapter);
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            assert!(weak.upgrade().is_none());
        }
    }
}
