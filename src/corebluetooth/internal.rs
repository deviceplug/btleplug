// btleplug Source Code File
//
// Copyright 2020 Nonpolynomial Labs LLC. All rights reserved.
//
// Licensed under the BSD 3-Clause license. See LICENSE file in the project root
// for full license information.
//
// For more info on handling CoreBluetooth Managers (and possibly having
// multiple), see https://forums.developer.apple.com/thread/20810

use crate::api::CharPropFlags;
use super::{
    central_delegate::{CentralDelegate, CentralDelegateEvent},
    utils::NSStringUtils,
    future::{BtlePlugFutureState, BtlePlugFutureStateShared, BtlePlugFuture},
    framework::{cb, ns}
};
use async_std::{
    task,
};
use objc::{
    rc::{StrongPtr},
    runtime::{Object, YES},
};
use uuid::Uuid;
use std::{
    collections::HashMap,
    thread,
    str::FromStr,
};
use crossbeam::crossbeam_channel::{bounded, Receiver, Sender, select};

struct CBCharacteristic {
    characteristic: StrongPtr,
    properties: CharPropFlags,
}

impl CBCharacteristic {
    pub fn new(characteristic: StrongPtr) -> Self {
        let properties = CBCharacteristic::form_flags(*characteristic);
        Self {
            characteristic,
            properties,
        }
    }

    fn form_flags(characteristic: *mut Object) -> CharPropFlags {
        let flags = cb::characteristic_properties(characteristic);
        let mut v = CharPropFlags::new();
        if (flags & cb::CHARACTERISTICPROPERTY_BROADCAST) != 0 {
            v |= CharPropFlags::BROADCAST;
        }
        if (flags & cb::CHARACTERISTICPROPERTY_READ) != 0 {
            v |= CharPropFlags::READ;
        }
        if (flags & cb::CHARACTERISTICPROPERTY_WRITEWITHOUTRESPONSE) != 0 {
            v |= CharPropFlags::WRITE_WITHOUT_RESPONSE;
        }
        if (flags & cb::CHARACTERISTICPROPERTY_WRITE) != 0 {
            v |= CharPropFlags::WRITE;
        }
        if (flags & cb::CHARACTERISTICPROPERTY_NOTIFY) != 0 {
            v |= CharPropFlags::NOTIFY;
        }
        if (flags & cb::CHARACTERISTICPROPERTY_INDICATE) != 0 {
            v |= CharPropFlags::INDICATE;
        }
        if (flags & cb::CHARACTERISTICPROPERTY_AUTHENTICATEDSIGNEDWRITES) != 0 {
            v |= CharPropFlags::AUTHENTICATED_SIGNED_WRITES;
        }
        info!("Flags: {:?}", v);
        v
    }
}

pub enum CoreBluetoothReply {
    ReadResult(Vec<u8>),
    Ok(),
    Err(),
}

pub type CoreBluetoothReplyState = BtlePlugFutureState<CoreBluetoothReply>;
pub type CoreBluetoothReplyStateShared = BtlePlugFutureStateShared<CoreBluetoothReply>;
pub type CoreBluetoothReplyFuture = BtlePlugFuture<CoreBluetoothReply>;

pub enum CBPeripheralMessage {
    // uuid, future
    ReadValue(String, CoreBluetoothReplyStateShared),
    // uuid, data, future
    WriteValue(String, Vec<u8>, CoreBluetoothReplyStateShared),
    // uuid, future
    Subscribe(String, CoreBluetoothReplyStateShared),
    // uuid, future
    Unsubscribe(String, CoreBluetoothReplyStateShared),
}

struct CBPeripheral {
    pub(in super::internal) peripheral: StrongPtr,
    pub(in super::internal) services: HashMap<Uuid, StrongPtr>,
    pub(in super::internal) characteristics: HashMap<Uuid, CBCharacteristic>,
}

impl CBPeripheral {
    pub fn new(peripheral: StrongPtr) -> Self {
        unsafe {
            Self {
                peripheral,
                services: HashMap::new(),
                characteristics: HashMap::new(),
            }
        }
    }

    // Allows the manager to send an event in our place, which will let us line
    // up with peripheral event expectations.
    pub(in super::internal) fn send_event() {
    }
}

// All of CoreBluetooth is basically async. It's all just waiting on delegate
// events/callbacks. Therefore, we should be able to round up all of our wacky
// ass mut *Object values, keep them in a single struct, in a single thread, and
// call it good. Right?
struct CoreBluetoothInternal {
    // TODO Should this be a StrongPtr?
    manager: StrongPtr,
    // TODO Should this be a StrongPtr?
    delegate: StrongPtr,
    // Map of identifiers to object pointers
    peripherals: HashMap<Uuid, CBPeripheral>,
    //peripherals: HashMap<String, StrongPtr>,
    delegate_receiver: Receiver<CentralDelegateEvent>,
    // Out in the world beyond CoreBluetooth, we'll be async, so just
    // task::block this when sending even though it'll never actually block.
    event_sender: async_std::sync::Sender<CoreBluetoothEvent>,
    message_receiver: Receiver<CoreBluetoothMessage>,
}

pub enum CoreBluetoothMessage {
    StartScanning,
    StopScanning,
    ConnectDevice(Uuid, CoreBluetoothReplyStateShared),
    DisconnectDevice(Uuid, CoreBluetoothReplyStateShared),
}

pub enum CoreBluetoothEvent {
    AdapterConnected,
    AdapterError,
    // name, identifier
    DeviceDiscovered(String, String),
    // identifier
    DeviceLost(String),
    // identifier
    DeviceConnected(String),
    // identifier
    DeviceDisconnected(String),
    DeviceNotification,
}

impl CoreBluetoothInternal {
    pub fn new(message_receiver: Receiver<CoreBluetoothMessage>, event_sender: async_std::sync::Sender<CoreBluetoothEvent>) -> Self {
        // Pretty sure these come preallocated?
        unsafe {
            let delegate = StrongPtr::new(CentralDelegate::delegate());
            Self {
                manager: StrongPtr::new(cb::centralmanager(*delegate)),
                peripherals: HashMap::new(),
                delegate_receiver: CentralDelegate::delegate_receiver_clone(*delegate),
                event_sender,
                message_receiver,
                delegate
            }
        }
    }

    pub fn wait_for_message(&mut self) -> bool {
        select!(
            recv(self.delegate_receiver) -> msg => {
                let event = match msg.unwrap() {
                    // TODO DidUpdateState does not imply that the adapter is
                    // on, just that it updated state.
                    //
                    // TODO We should probably also register some sort of
                    // "ready" variable in our adapter that will cause scans/etc
                    // to fail if this hasn't updated.
                    CentralDelegateEvent::DidUpdateState => Some(CoreBluetoothEvent::AdapterConnected),
                    CentralDelegateEvent::DiscoveredPeripheral(peripheral) => {
                        let uuid_nsstring = ns::uuid_uuidstring(cb::peer_identifier(*peripheral));
                        let uuid_str = Uuid::from_str(&NSStringUtils::string_to_string(uuid_nsstring)).unwrap();
                        let name = NSStringUtils::string_to_string(cb::peripheral_name(*peripheral));
                        if self.peripherals.contains_key(&uuid_str) {
                            None
                        } else {
                            if name.contains("LVS") {
                                self.connect_peripheral(*peripheral);
                            }
                            self.peripherals.insert(uuid_str,
                                                    CBPeripheral::new(peripheral));
                            None
                            //Some(CoreBluetoothEvent::DeviceDiscovered(name, uuid_str))
                        }
                    },
                    CentralDelegateEvent::DiscoveredServices(peripheral_id, service_map) => {
                        info!("Found services!");
                        for id in service_map.keys() {
                            info!("{}", id);
                        }
                        if let Some(p) = self.peripherals.get_mut(&peripheral_id) {
                            p.services = service_map;
                        }
                        None
                    },
                    CentralDelegateEvent::DiscoveredCharacteristics(peripheral_id, char_map) => {
                        info!("Found chars!");
                        for id in char_map.keys() {
                            info!("{}", id);
                        }
                        if let Some(p) = self.peripherals.get_mut(&peripheral_id) {
                            for (c_uuid, c_obj) in char_map {
                                p.characteristics.insert(c_uuid, CBCharacteristic::new(c_obj));
                            }
                        }
                        None
                    },
                    _ => None,
                };
                if let Some(e) = event {
                    let s = self.event_sender.clone();
                    task::block_on(async {
                        s.send(e).await;
                    });
                }
                true
            },
            recv(self.message_receiver) -> msg => {
                // TODO If our receiver drops, this will fail
                if msg.is_err() {
                    return false;
                }
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
                true
            }
        )
    }

    fn start_discovery(&mut self) {
        trace!("BluetoothAdapter::start_discovery");
        let options = ns::mutabledictionary();
        // NOTE: If duplicates are not allowed then a peripheral will not show
        // up again once connected and then disconnected.
        ns::mutabledictionary_setobject_forkey(options,
                                               ns::number_withbool(YES),
                                               unsafe { cb::CENTRALMANAGERSCANOPTIONALLOWDUPLICATESKEY });
        cb::centralmanager_scanforperipherals_options(*self.manager, options);
    }

    fn stop_discovery(&mut self) {
        trace!("BluetoothAdapter::stop_discovery");
        cb::centralmanager_stopscan(*self.manager);
    }

    fn connect_peripheral(&mut self, peripheral: *mut Object) {
        cb::centralmanager_connectperipheral(*self.manager, peripheral);
    }
}

impl Drop for CoreBluetoothInternal {
    fn drop(&mut self) {
        trace!("BluetoothAdapter::drop");
        // NOTE: stop discovery only here instead of in BluetoothDiscoverySession
        self.stop_discovery();
        CentralDelegate::delegate_drop_channel(*self.delegate);
    }
}

pub fn run_corebluetooth_thread(event_sender: async_std::sync::Sender<CoreBluetoothEvent>) -> Sender<CoreBluetoothMessage> {
    let (sender, receiver) = bounded::<CoreBluetoothMessage>(256);
    thread::spawn(move || {
        let mut cbi = CoreBluetoothInternal::new(receiver, event_sender);
        loop {
            if !cbi.wait_for_message() {
                break;
            }
        }
    });
    sender
}
