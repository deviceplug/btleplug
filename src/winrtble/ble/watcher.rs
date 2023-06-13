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
use windows::{Devices::Bluetooth::Advertisement::*, Foundation::TypedEventHandler};

pub type AdvertismentEventHandler = Box<dyn Fn(&BluetoothLEAdvertisementReceivedEventArgs) + Send>;

pub struct BLEWatcher {
    watcher: BluetoothLEAdvertisementWatcher,
}

impl From<windows::core::Error> for Error {
    fn from(err: windows::core::Error) -> Error {
        Error::Other(format!("{:?}", err).into())
    }
}

impl BLEWatcher {
    pub fn new() -> Self {
        let ad = BluetoothLEAdvertisementFilter::new().unwrap();
        let watcher = BluetoothLEAdvertisementWatcher::Create(&ad).unwrap();
        BLEWatcher { watcher }
    }

    pub fn start(&self, filter: ScanFilter, on_received: AdvertismentEventHandler) -> Result<()> {
        let ScanFilter { services } = filter;
        let ad = self
            .watcher
            .AdvertisementFilter()
            .unwrap()
            .Advertisement()
            .unwrap();
        let ad_services = ad.ServiceUuids().unwrap();
        ad_services.Clear().unwrap();
        for service in services {
            ad_services
                .Append(windows::core::GUID::from(service.as_u128()))
                .unwrap();
        }
        self.watcher
            .SetScanningMode(BluetoothLEScanningMode::Active)
            .unwrap();
        let handler: TypedEventHandler<
            BluetoothLEAdvertisementWatcher,
            BluetoothLEAdvertisementReceivedEventArgs,
        > = TypedEventHandler::new(
            move |_sender, args: &Option<BluetoothLEAdvertisementReceivedEventArgs>| {
                if let Some(args) = args {
                    on_received(args);
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
