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

use crate::{api::ScanFilter, Error, Result};
use windows::{core::Ref, Devices::Bluetooth::Advertisement::*, Foundation::TypedEventHandler};

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
        let ad = self.watcher.AdvertisementFilter()?.Advertisement()?;
        let ad_services = ad.ServiceUuids()?;
        ad_services.Clear()?;
        for service in services {
            ad_services.Append(windows::core::GUID::from(service.as_u128()))?;
        }
        self.watcher
            .SetScanningMode(BluetoothLEScanningMode::Active)?;
        let _ = self.watcher.SetAllowExtendedAdvertisements(true);
        let handler: TypedEventHandler<
            BluetoothLEAdvertisementWatcher,
            BluetoothLEAdvertisementReceivedEventArgs,
        > = TypedEventHandler::new(
            move |_sender, args: Ref<BluetoothLEAdvertisementReceivedEventArgs>| {
                if let Ok(args) = args.ok() {
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
