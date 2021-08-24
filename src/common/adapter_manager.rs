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
use crate::api::{BDAddr, CentralEvent, Peripheral};
use dashmap::{mapref::one::RefMut, DashMap};
use futures::stream::{Stream, StreamExt};
use log::trace;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;

#[derive(Clone, Debug)]
pub struct AdapterManager<PeripheralType>
where
    PeripheralType: Peripheral,
{
    shared: Arc<Shared<PeripheralType>>,
}

#[derive(Debug)]
struct Shared<PeripheralType> {
    peripherals: DashMap<BDAddr, PeripheralType>,
    events_channel: broadcast::Sender<CentralEvent>,
}

impl<PeripheralType: Peripheral + 'static> Default for AdapterManager<PeripheralType> {
    fn default() -> Self {
        let (broadcast_sender, _) = broadcast::channel(16);
        AdapterManager {
            shared: Arc::new(Shared {
                peripherals: DashMap::new(),
                events_channel: broadcast_sender,
            }),
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
                self.shared.peripherals.remove(&addr);
            }
            _ => {}
        }

        if let Err(lost) = self.shared.events_channel.send(event) {
            trace!("Lost central event, while nothing subscribed: {:?}", lost);
        }
    }

    pub fn event_stream(&self) -> Pin<Box<dyn Stream<Item = CentralEvent> + Send>> {
        let receiver = self.shared.events_channel.subscribe();
        Box::pin(BroadcastStream::new(receiver).filter_map(|x| async move {
            if x.is_ok() {
                Some(x.unwrap())
            } else {
                None
            }
        }))
    }

    #[allow(dead_code)]
    pub fn has_peripheral(&self, addr: &BDAddr) -> bool {
        self.shared.peripherals.contains_key(addr)
    }

    pub fn add_peripheral(&self, addr: BDAddr, peripheral: PeripheralType) {
        assert!(
            !self.shared.peripherals.contains_key(&addr),
            "Adding a peripheral that's already in the map."
        );
        assert_eq!(peripheral.address(), addr, "Device has unexpected address."); // TODO remove addr argument
        self.shared.peripherals.insert(addr, peripheral);
    }

    pub fn peripherals(&self) -> Vec<PeripheralType> {
        self.shared
            .peripherals
            .iter()
            .map(|val| val.value().clone())
            .collect()
    }

    pub fn peripheral_mut(&self, address: BDAddr) -> Option<RefMut<BDAddr, PeripheralType>> {
        self.shared.peripherals.get_mut(&address)
    }

    pub fn peripheral(&self, address: BDAddr) -> Option<PeripheralType> {
        self.shared
            .peripherals
            .get(&address)
            .map(|val| val.value().clone())
    }
}
