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

use super::{ble::watcher::BLEWatcher, peripheral::Peripheral};
use crate::{
    api::{BDAddr, Central, CentralEvent},
    common::adapter_manager::AdapterManager,
    Error, Result,
};
use async_trait::async_trait;
use futures::stream::Stream;
use std::convert::TryInto;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

/// Implementation of [api::Central](crate::api::Central).
#[derive(Clone)]
pub struct Adapter {
    watcher: Arc<Mutex<BLEWatcher>>,
    manager: AdapterManager<Peripheral>,
}

impl Adapter {
    pub(crate) fn new() -> Self {
        let watcher = Arc::new(Mutex::new(BLEWatcher::new()));
        let manager = AdapterManager::default();
        Adapter { watcher, manager }
    }
}

#[async_trait]
impl Central for Adapter {
    type Peripheral = Peripheral;

    async fn events(&self) -> Result<Pin<Box<dyn Stream<Item = CentralEvent>>>> {
        Ok(self.manager.event_stream())
    }

    async fn start_scan(&self) -> Result<()> {
        let watcher = self.watcher.lock().unwrap();
        let manager = self.manager.clone();
        watcher.start(Box::new(move |args| {
            let bluetooth_address = args.BluetoothAddress().unwrap();
            let address = bluetooth_address.try_into().unwrap();
            if let Some(mut entry) = manager.peripheral_mut(address) {
                entry.value_mut().update_properties(args);
                manager.emit(CentralEvent::DeviceUpdated(address));
            } else {
                let peripheral = Peripheral::new(manager.clone(), address);
                peripheral.update_properties(args);
                manager.add_peripheral(address, peripheral);
                manager.emit(CentralEvent::DeviceDiscovered(address));
            }
        }))
    }

    async fn stop_scan(&self) -> Result<()> {
        let watcher = self.watcher.lock().unwrap();
        watcher.stop().unwrap();
        Ok(())
    }

    async fn peripherals(&self) -> Result<Vec<Peripheral>> {
        Ok(self.manager.peripherals())
    }

    async fn peripheral(&self, address: BDAddr) -> Result<Peripheral> {
        self.manager
            .peripheral(address)
            .ok_or(Error::DeviceNotFound)
    }
}
