// btleplug Source Code File
//
// Copyright 2020 Nonpolynomial Labs LLC. All rights reserved.
//
// Licensed under the BSD 3-Clause license. See LICENSE file in the project root
// for full license information.

use crate::api::CharPropFlags;
use super::central_delegate::{CentralDelegate, CentralDelegateEvent};
use async_std::{
    task,
};

use objc::{
    runtime::{Object, YES},
};
use super::framework::{cb, ns};
use uuid::Uuid;
use std::{
    error::Error,
    collections::HashMap,
    thread
};
use crossbeam::crossbeam_channel::{bounded, Receiver, Sender, select};

struct CBCharacteristic {
    characteristic: *mut Object,
    properties: CharPropFlags,
}

struct CBDevice {
    delegate: *mut Object,
    peripheral: *mut Object,
    services: HashMap<Uuid, *mut Object>,
    characteristics: HashMap<Uuid, CBCharacteristic>,
}

// All of CoreBluetooth is basically async. It's all just waiting on delegate
// events/callbacks. Therefore, we should be able to round up all of our wacky
// ass mut *Object values, keep them in a single struct, in a single thread, and
// call it good. Right?
struct CoreBluetoothInternal {
    manager: *mut Object,
    delegate: *mut Object,
    // Map of identifiers to object pointers
    peripherals: HashMap<String, CBDevice>,
    delegate_receiver: Receiver<CentralDelegateEvent>,
    // Out in the world beyond CoreBluetooth, we'll be async, so just
    // task::block this when sending even though it'll never actually block.
    event_sender: async_std::sync::Sender<CoreBluetoothEvent>,
    message_receiver: Receiver<CoreBluetoothMessage>,
}

pub enum CoreBluetoothMessage {
    StartScanning,
    StopScanning,
    ConnectDevice,
    DisconnectDevice,
    ReadValue,
    WriteValue,
    Subscribe,
    Unsubscribe,
}

pub enum CoreBluetoothEvent {
    AdapterConnected,
    AdapterError,
    DeviceDiscovered,
    DeviceLost,
    DeviceConnected,
    DeviceDisconnected,
    DeviceNotification,
}

impl CoreBluetoothInternal {
    // TODO this should throw if we don't have an adapter available or powered
    // up.
    pub fn try_new(message_receiver: Receiver<CoreBluetoothMessage>, event_sender: async_std::sync::Sender<CoreBluetoothEvent>) -> Result<Self, Box<dyn Error>> {
        info!("BluetoothAdapter::init");
        let delegate = CentralDelegate::delegate();
        let manager = cb::centralmanager(delegate);
        info!("Done with init");
        let mut recv = CentralDelegate::delegate_receiver_clone(delegate);
        info!("Waiting for event!");
        info!("Got event!");
        let adapter = Self {
            manager: manager,
            delegate: delegate,
            peripherals: HashMap::new(),
            delegate_receiver: CentralDelegate::delegate_receiver_clone(delegate),
            event_sender,
            message_receiver
        };
        Ok(adapter)
    }

    pub fn wait_for_message(&mut self) {
        select!(
            recv(self.delegate_receiver) -> msg => {
                let event = match msg.unwrap() {
                    // TODO DidUpdateState does not imply that the adapter is
                    // on, just that it updated state.
                    CentralDelegateEvent::DidUpdateState => Some(CoreBluetoothEvent::AdapterConnected),
                    _ => None,
                };
                if let Some(e) = event {
                    let s = self.event_sender.clone();
                    task::block_on(async {
                        s.send(e).await;
                    });
                }
            },
            recv(self.message_receiver) -> msg => {
                match msg.unwrap() {
                    CoreBluetoothMessage::StartScanning => {
                        self.start_discovery();
                    },
                    CoreBluetoothMessage::StopScanning => {
                        self.stop_discovery();
                    },
                    _ => {
                    },
                };
            }
        );
    }

    fn start_discovery(&mut self) {
        trace!("BluetoothAdapter::start_discovery");
        let options = ns::mutabledictionary();
        // NOTE: If duplicates are not allowed then a peripheral will not show
        // up again once connected and then disconnected.
        ns::mutabledictionary_setobject_forkey(options,
                                               ns::number_withbool(YES),
                                               unsafe { cb::CENTRALMANAGERSCANOPTIONALLOWDUPLICATESKEY });
        cb::centralmanager_scanforperipherals_options(self.manager, options);
    }

    fn stop_discovery(&mut self) {
        trace!("BluetoothAdapter::stop_discovery");
        cb::centralmanager_stopscan(self.manager);
    }

    fn connect_device(&mut self) {
    }

    fn disconnect_device(&mut self) {
    }
}

impl Drop for CoreBluetoothInternal {
    fn drop(&mut self) {
        trace!("BluetoothAdapter::drop");
        // NOTE: stop discovery only here instead of in BluetoothDiscoverySession
        // self.stop_discovery().unwrap();
        CentralDelegate::delegate_drop_channel(self.delegate);
    }
}

pub fn run_corebluetooth_thread(event_sender: async_std::sync::Sender<CoreBluetoothEvent>) -> Sender<CoreBluetoothMessage> {
    let (sender, receiver) = bounded::<CoreBluetoothMessage>(256);
    thread::spawn(move || {
        let mut cbi = CoreBluetoothInternal::try_new(receiver, event_sender).unwrap();
        loop {
            cbi.wait_for_message();
        }
    });
    sender
}
