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
use windows::{Devices::Bluetooth::Advertisement::*, Foundation::TypedEventHandler, core::Ref};

pub type AdvertisementEventHandler =
    Box<dyn Fn(&BluetoothLEAdvertisementReceivedEventArgs) -> windows::core::Result<()> + Send>;

#[derive(Debug)]
pub struct BLEWatcher {
    watcher: BluetoothLEAdvertisementWatcher,
}

impl From<windows::core::Error> for Error {
    fn from(err: windows::core::Error) -> Error {
        Error::Other(format!("{:?}", err).into())
    }
}

impl BLEWatcher {
    pub fn new() -> Result<Self> {
        let ad = BluetoothLEAdvertisementFilter::new()?;
        let watcher = BluetoothLEAdvertisementWatcher::Create(&ad)?;
        Ok(BLEWatcher { watcher })
    }

    pub fn start(&self, filter: ScanFilter, on_received: AdvertisementEventHandler) -> Result<()> {
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
        
        let matching_devices = std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashSet::new()));

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
                                    is_match = filter_guids.iter().all(|g| advertised.contains(g));
                                }
                            }
                        }

                        let mut cache = matching_devices.lock().unwrap();
                        if is_match {
                            cache.insert(address);
                        } else if !cache.contains(&address) {
                            // If the current packet doesn't have the UUID and we haven't seen it before, drop it.
                            return Ok(());
                        }
                    }
                    on_received(args)?;
                }
                Ok(())
            },
        );

        self.watcher.Received(&handler)?;
        self.watcher.Start()?;
        Ok(())
    }

    pub fn stop(&self) -> Result<()> {
        self.watcher.Stop()?;
        Ok(())
    }
}
