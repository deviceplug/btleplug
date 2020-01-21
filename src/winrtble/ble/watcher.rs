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

use winrt::{
    ComPtr, RtDefaultConstructible,
    windows::devices::bluetooth::advertisement::*,
    windows::foundation::{TypedEventHandler},
};
use crate::Result;

pub type AdvertismentEventHandler = Box<Fn(&BluetoothLEAdvertisementReceivedEventArgs) + Send>;

pub struct BLEWatcher {
    watcher: ComPtr<BluetoothLEAdvertisementWatcher>,
}

unsafe impl Send for BLEWatcher {}
unsafe impl Sync for BLEWatcher {}

impl BLEWatcher {
    pub fn new() -> Self {
        let ad = BluetoothLEAdvertisementFilter::new();
        let watcher = BluetoothLEAdvertisementWatcher::create(&ad).unwrap();
        BLEWatcher{ watcher }
    }

    pub fn start(&self, on_received: AdvertismentEventHandler) -> Result<()> {
        self.watcher.set_scanning_mode(BluetoothLEScanningMode::Active).unwrap();
        let handler = TypedEventHandler::new(move |_sender, args: *mut BluetoothLEAdvertisementReceivedEventArgs| {
            let args = unsafe { (&*args) }; 
            on_received(args);
            Ok(())
        });
        self.watcher.add_received(&handler).unwrap();
        self.watcher.start().unwrap();
        Ok(())
    }

    pub fn stop(&self) -> Result<()> {
        self.watcher.stop().unwrap();
        Ok(())
    }
}
