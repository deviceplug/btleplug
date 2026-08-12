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
use crate::api::{CentralEvent, Peripheral};
use crate::platform::PeripheralId;
use dashmap::{DashMap, mapref::one::RefMut};
use futures::stream::{Stream, StreamExt};
use log::trace;
use std::pin::Pin;
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;

#[derive(Debug)]
pub struct AdapterManager<PeripheralType>
where
    PeripheralType: Peripheral,
{
    peripherals: DashMap<PeripheralId, PeripheralType>,
    events_channel: broadcast::Sender<CentralEvent>,
}

impl<PeripheralType: Peripheral + 'static> Default for AdapterManager<PeripheralType> {
    fn default() -> Self {
        let (broadcast_sender, _) = broadcast::channel(16);
        AdapterManager {
            peripherals: DashMap::new(),
            events_channel: broadcast_sender,
        }
    }
}

impl<PeripheralType> AdapterManager<PeripheralType>
where
    PeripheralType: Peripheral + 'static,
{
    pub fn emit(&self, event: CentralEvent) {
        if let CentralEvent::DeviceDisconnected(ref id) = event {
            self.peripherals.remove(id);
        }

        if let Err(lost) = self.events_channel.send(event) {
            trace!("Lost central event, while nothing subscribed: {:?}", lost);
        }
    }

    pub fn event_stream(&self) -> Pin<Box<dyn Stream<Item = CentralEvent> + Send>> {
        let receiver = self.events_channel.subscribe();
        Box::pin(BroadcastStream::new(receiver).filter_map(|x| async move { x.ok() }))
    }

    /// Idempotent: keeps the peripheral already in the map, if any.
    ///
    /// Was an `assert!` + `insert`. That is a check-then-act pair on a
    /// concurrent `DashMap`, and the callers that reach it are inherently
    /// racy — droidplug's `Adapter::report_scan_result` looks a peripheral
    /// up, finds nothing, and then adds, with no lock held across the two
    /// steps. Two scan results for the same device (which is what starting a
    /// second scan on the process-global adapter produces) could both take
    /// the `None` branch and the second `add` would abort the process.
    ///
    /// A panic here is especially hard to diagnose because the callers are
    /// spawned tasks: Tokio stores the payload in the `JoinHandle` nobody
    /// joins, and on Android the default hook writes to stderr, which is not
    /// in logcat — so the symptom is a task that silently stops existing.
    ///
    /// Keeping the existing entry (rather than replacing it) is deliberate:
    /// it may already carry connection state and characteristics that a
    /// freshly constructed wrapper for the same address would not.
    pub fn add_peripheral(&self, peripheral: PeripheralType) {
        let id = peripheral.id();
        self.peripherals.entry(id).or_insert(peripheral);
    }

    pub fn clear_peripherals(&self) {
        self.peripherals.clear();
    }

    pub fn peripherals(&self) -> Vec<PeripheralType> {
        self.peripherals
            .iter()
            .map(|val| val.value().clone())
            .collect()
    }

    // Only used on windows and macOS/iOS, so turn off deadcode so we don't get warnings on android/linux.
    #[allow(dead_code)]
    pub fn peripheral_mut(
        &self,
        id: &PeripheralId,
    ) -> Option<RefMut<'_, PeripheralId, PeripheralType>> {
        self.peripherals.get_mut(id)
    }

    pub fn peripheral(&self, id: &PeripheralId) -> Option<PeripheralType> {
        self.peripherals.get(id).map(|val| val.value().clone())
    }
}
