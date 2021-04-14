/// Implements common functionality for adapters across platforms.
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
use crate::{
    api::{BDAddr, CentralEvent, Peripheral},
    common::util::send_notification,
};
use dashmap::{mapref::one::RefMut, DashMap};
use futures::channel::mpsc::{self, UnboundedSender};
use futures::stream::Stream;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug)]
pub struct AdapterManager<PeripheralType>
where
    PeripheralType: Peripheral,
{
    peripherals: Arc<DashMap<BDAddr, PeripheralType>>,
    async_senders: Arc<Mutex<Vec<UnboundedSender<CentralEvent>>>>,
}

impl<PeripheralType: Peripheral + 'static> Default for AdapterManager<PeripheralType> {
    fn default() -> Self {
        let peripherals = Arc::new(DashMap::new());
        AdapterManager {
            peripherals,
            async_senders: Arc::new(Mutex::new(vec![])),
        }
    }
}

impl<PeripheralType> AdapterManager<PeripheralType>
where
    PeripheralType: Peripheral + 'static,
{
    pub fn emit(&self, event: CentralEvent) {
        match event {
            CentralEvent::DeviceDisconnected(addr) => {
                self.peripherals.remove(&addr);
            }
            CentralEvent::DeviceLost(addr) => {
                self.peripherals.remove(&addr);
            }
            _ => {}
        }

        send_notification(&self.async_senders, &event);
    }

    pub fn event_stream(&self) -> Pin<Box<dyn Stream<Item = CentralEvent>>> {
        let (sender, receiver) = mpsc::unbounded();
        self.async_senders.lock().unwrap().push(sender);
        Box::pin(receiver)
    }

    pub fn has_peripheral(&self, addr: &BDAddr) -> bool {
        self.peripherals.contains_key(addr)
    }

    pub fn add_peripheral(&self, addr: BDAddr, peripheral: PeripheralType) {
        assert!(
            !self.peripherals.contains_key(&addr),
            "Adding a peripheral that's already in the map."
        );
        assert_eq!(peripheral.address(), addr, "Device has unexpected address."); // TODO remove addr argument
        self.peripherals.insert(addr, peripheral);
    }

    pub fn peripherals(&self) -> Vec<PeripheralType> {
        self.peripherals
            .iter()
            .map(|val| val.value().clone())
            .collect()
    }

    pub fn peripheral_mut(&self, address: BDAddr) -> Option<RefMut<BDAddr, PeripheralType>> {
        self.peripherals.get_mut(&address)
    }

    pub fn peripheral(&self, address: BDAddr) -> Option<PeripheralType> {
        self.peripherals
            .get(&address)
            .map(|val| val.value().clone())
    }
}
