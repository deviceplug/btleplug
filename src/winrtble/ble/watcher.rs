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

use crate::{Error, Result, api::ScanFilter, winrtble::utils};
use std::{collections::HashSet, sync::Mutex};
use windows::{Devices::Bluetooth::Advertisement::*, Foundation::TypedEventHandler, core::Ref};

const MATCH_CACHE_CAPACITY: usize = 1024;

pub type AdvertisementEventHandler =
    Box<dyn Fn(&BluetoothLEAdvertisementReceivedEventArgs) -> windows::core::Result<()> + Send>;

#[derive(Debug)]
pub struct BLEWatcher {
    watcher: BluetoothLEAdvertisementWatcher,
    received_token: Option<i64>,
}

impl From<windows::core::Error> for Error {
    fn from(err: windows::core::Error) -> Error {
        Error::Other(format!("{:?}", err).into())
    }
}

#[derive(Default)]
struct MatchCache {
    addresses: HashSet<u64>,
}

impl MatchCache {
    fn record(&mut self, address: u64) {
        if self.addresses.len() < MATCH_CACHE_CAPACITY || self.addresses.contains(&address) {
            self.addresses.insert(address);
        }
    }

    fn contains(&self, address: u64) -> bool {
        self.addresses.contains(&address)
    }
}

impl BLEWatcher {
    pub fn new() -> Result<Self> {
        let ad = BluetoothLEAdvertisementFilter::new()?;
        let watcher = BluetoothLEAdvertisementWatcher::Create(&ad)?;
        Ok(BLEWatcher {
            watcher,
            received_token: None,
        })
    }

    pub fn start(
        &mut self,
        filter: ScanFilter,
        on_received: AdvertisementEventHandler,
    ) -> Result<()> {
        self.remove_received_handler()?;
        let ScanFilter { services } = filter;

        // Clear any OS-level service UUID filter from a previous scan.
        // We intentionally do NOT set service UUIDs on the OS filter: on some
        // Windows BLE drivers the 128-bit UUID filter silently drops matching
        // advertisements. Software filtering in the handler is used instead.
        let ad = self.watcher.AdvertisementFilter()?.Advertisement()?;
        ad.ServiceUuids()?.Clear()?;

        self.watcher
            .SetScanningMode(BluetoothLEScanningMode::Active)?;
        let _ = self.watcher.SetAllowExtendedAdvertisements(true);

        // Pre-convert the filter UUIDs once so the handler closure is cheap.
        let filter_guids: Vec<windows::core::GUID> = services.iter().map(utils::to_guid).collect();
        let matching_devices = Mutex::new(MatchCache::default());

        let handler: TypedEventHandler<
            BluetoothLEAdvertisementWatcher,
            BluetoothLEAdvertisementReceivedEventArgs,
        > = TypedEventHandler::new(
            move |_sender, args: Ref<BluetoothLEAdvertisementReceivedEventArgs>| {
                if let Ok(args) = args.ok() {
                    // Software service-UUID filter.
                    if !filter_guids.is_empty() {
                        let address = args.BluetoothAddress().unwrap_or(0);
                        let mut is_match = false;

                        if let Ok(ad) = args.Advertisement() {
                            if let Ok(ad_uuids) = ad.ServiceUuids() {
                                let count = ad_uuids.Size().unwrap_or(0);
                                if count > 0 {
                                    let advertised: Vec<windows::core::GUID> =
                                        (0..count).filter_map(|i| ad_uuids.GetAt(i).ok()).collect();
                                    is_match = filter_guids.iter().any(|g| advertised.contains(g));
                                }
                            }
                        }

                        let mut cache = matching_devices.lock().unwrap();
                        if is_match {
                            cache.record(address);
                        } else if !matches!(
                            args.AdvertisementType(),
                            Ok(BluetoothLEAdvertisementType::ScanResponse)
                        ) || !cache.contains(address)
                        {
                            return Ok(());
                        }
                    }
                    on_received(args)?;
                }
                Ok(())
            },
        );

        self.received_token = Some(self.watcher.Received(&handler)?);
        self.watcher.Start()?;
        Ok(())
    }

    pub fn stop(&mut self) -> Result<()> {
        self.watcher.Stop()?;
        self.remove_received_handler()
    }

    fn remove_received_handler(&mut self) -> Result<()> {
        if let Some(token) = self.received_token.take() {
            self.watcher.RemoveReceived(token)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{MATCH_CACHE_CAPACITY, MatchCache};

    #[test]
    fn match_cache_is_bounded() {
        let mut cache = MatchCache::default();
        for address in 0..MATCH_CACHE_CAPACITY as u64 {
            cache.record(address);
        }
        cache.record(MATCH_CACHE_CAPACITY as u64);

        assert_eq!(cache.addresses.len(), MATCH_CACHE_CAPACITY);
        assert!(cache.contains(0));
        assert!(!cache.contains(MATCH_CACHE_CAPACITY as u64));
    }
}
