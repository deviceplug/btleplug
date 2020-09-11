// btleplug Source Code File
//
// Copyright 2020 Nonpolynomial Labs LLC. All rights reserved.
//
// Licensed under the BSD 3-Clause license. See LICENSE file in the project root
// for full license information.
//
// For more info on handling CoreBluetooth Managers (and possibly having
// multiple), see https://forums.developer.apple.com/thread/20810

use super::{
    central_delegate::{CentralDelegate, CentralDelegateEvent},
    framework::{cb, ns},
    future::{BtlePlugFuture, BtlePlugFutureState, BtlePlugFutureStateShared},
    utils::{CoreBluetoothUtils, NSStringUtils},
};
use crate::api::{CharPropFlags, Characteristic, UUID};
use async_std::{
    prelude::{FutureExt, StreamExt},
    sync::{channel, Receiver, Sender},
    task,
};
use objc::{
    rc::StrongPtr,
    runtime::{Object, YES},
};
use std::{
    collections::{BTreeSet, HashMap, VecDeque},
    os::raw::c_uint,
    str::FromStr,
    thread,
};
use uuid::Uuid;

struct CBCharacteristic {
    pub characteristic: StrongPtr,
    pub uuid: Uuid,
    pub properties: CharPropFlags,
    pub read_future_state: VecDeque<CoreBluetoothReplyStateShared>,
    pub write_future_state: VecDeque<CoreBluetoothReplyStateShared>,
    pub subscribe_future_state: VecDeque<CoreBluetoothReplyStateShared>,
    pub unsubscribe_future_state: VecDeque<CoreBluetoothReplyStateShared>,
}

impl CBCharacteristic {
    pub fn new(characteristic: StrongPtr) -> Self {
        let properties = CBCharacteristic::form_flags(*characteristic);
        let uuid =
            CoreBluetoothUtils::uuid_to_canonical_uuid_string(cb::attribute_uuid(*characteristic));
        let uuid = Uuid::from_str(&uuid).unwrap();
        Self {
            characteristic,
            uuid,
            properties,
            read_future_state: VecDeque::with_capacity(10),
            write_future_state: VecDeque::with_capacity(10),
            subscribe_future_state: VecDeque::with_capacity(10),
            unsubscribe_future_state: VecDeque::with_capacity(10),
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
    Connected(BTreeSet<Characteristic>),
    Ok,
    Err(String),
}

pub enum CBPeripheralEvent {
    Disconnected,
    Notification(Uuid, Vec<u8>),
}

pub type CoreBluetoothReplyState = BtlePlugFutureState<CoreBluetoothReply>;
pub type CoreBluetoothReplyStateShared = BtlePlugFutureStateShared<CoreBluetoothReply>;
pub type CoreBluetoothReplyFuture = BtlePlugFuture<CoreBluetoothReply>;

struct CBPeripheral {
    pub peripheral: StrongPtr,
    services: HashMap<Uuid, StrongPtr>,
    pub characteristics: HashMap<Uuid, CBCharacteristic>,
    pub event_sender: Sender<CBPeripheralEvent>,
    pub connected_future_state: Option<CoreBluetoothReplyStateShared>,
    characteristic_update_count: u32,
}

impl CBPeripheral {
    pub fn new(peripheral: StrongPtr, event_sender: Sender<CBPeripheralEvent>) -> Self {
        Self {
            peripheral,
            services: HashMap::new(),
            characteristics: HashMap::new(),
            event_sender,
            connected_future_state: None,
            characteristic_update_count: 0,
        }
    }

    pub fn set_services(&mut self, services: HashMap<Uuid, StrongPtr>) {
        self.services = services;
    }

    pub fn set_characteristics(&mut self, characteristics: HashMap<Uuid, StrongPtr>) {
        for (c_uuid, c_obj) in characteristics {
            self.characteristics
                .insert(c_uuid, CBCharacteristic::new(c_obj));
        }
        // It's time for QUESTIONABLE ASSUMPTIONS.
        //
        // For sake of being lazy, we don't want to fire device connection until
        // we have all of our services and characteristics. We assume that
        // set_characteristics should be called once for every entry in the
        // service map. Once that's done, we're filled out enough and can send
        // back a Connected reply to the waiting future with all of the
        // characteristic info in it.
        self.characteristic_update_count += 1;
        if self.characteristic_update_count == (self.services.len() as u32) {
            if self.connected_future_state.is_none() {
                panic!("We should still have a future at this point!");
            }
            let mut char_set = BTreeSet::new();
            for (uuid, c) in &self.characteristics {
                let mut id = *uuid.as_bytes();
                id.reverse();
                let char = Characteristic {
                    // We can't get handles on macOS, just set them to 0.
                    start_handle: 0,
                    end_handle: 0,
                    value_handle: 0,
                    uuid: UUID::B128(id),
                    properties: c.properties,
                };
                info!("{:?}", char.uuid);
                char_set.insert(char);
            }
            self.connected_future_state
                .take()
                .unwrap()
                .lock()
                .unwrap()
                .set_reply(CoreBluetoothReply::Connected(char_set));
        }
    }

    // Allows the manager to send an event in our place, which will let us line
    // up with peripheral event expectations.
    pub(in super::internal) fn send_event() {}
}

// All of CoreBluetooth is basically async. It's all just waiting on delegate
// events/callbacks. Therefore, we should be able to round up all of our wacky
// ass mut *Object values, keep them in a single struct, in a single thread, and
// call it good. Right?
struct CoreBluetoothInternal {
    manager: StrongPtr,
    delegate: StrongPtr,
    // Map of identifiers to object pointers
    peripherals: HashMap<Uuid, CBPeripheral>,
    //peripherals: HashMap<String, StrongPtr>,
    delegate_receiver: Receiver<CentralDelegateEvent>,
    // Out in the world beyond CoreBluetooth, we'll be async, so just
    // task::block this when sending even though it'll never actually block.
    event_sender: Sender<CoreBluetoothEvent>,
    message_receiver: Receiver<CoreBluetoothMessage>,
}

pub enum CoreBluetoothMessage {
    StartScanning,
    StopScanning,
    ConnectDevice(Uuid, CoreBluetoothReplyStateShared),
    DisconnectDevice(Uuid, CoreBluetoothReplyStateShared),
    // device uuid, characteristic uuid, future
    ReadValue(Uuid, Uuid, CoreBluetoothReplyStateShared),
    // device uuid, characteristic uuid, data, future
    WriteValue(Uuid, Uuid, Vec<u8>, CoreBluetoothReplyStateShared),
    // device uuid, characteristic uuid, data, future
    WriteValueWithResponse(Uuid, Uuid, Vec<u8>, CoreBluetoothReplyStateShared),
    // device uuid, characteristic uuid, future
    Subscribe(Uuid, Uuid, CoreBluetoothReplyStateShared),
    // device uuid, characteristic uuid, future
    Unsubscribe(Uuid, Uuid, CoreBluetoothReplyStateShared),
}

pub enum CoreBluetoothEvent {
    AdapterConnected,
    AdapterError,
    // name, identifier, event receiver, message sender
    DeviceDiscovered(Uuid, String, async_std::sync::Receiver<CBPeripheralEvent>),
    DeviceUpdated(Uuid, String),
    // identifier
    DeviceLost(Uuid),
}

// Aggregate everything that can come in from different sources into a single
// enum type.
enum InternalLoopMessage {
    Delegate(CentralDelegateEvent),
    Adapter(CoreBluetoothMessage),
    // If the delegate or adapter go away, we're done.
    LoopFinished,
}

impl CoreBluetoothInternal {
    pub fn new(
        message_receiver: Receiver<CoreBluetoothMessage>,
        event_sender: async_std::sync::Sender<CoreBluetoothEvent>,
    ) -> Self {
        // Pretty sure these come preallocated?
        unsafe {
            let delegate = StrongPtr::new(CentralDelegate::delegate());
            Self {
                manager: StrongPtr::new(cb::centralmanager(*delegate)),
                peripherals: HashMap::new(),
                delegate_receiver: CentralDelegate::delegate_receiver_clone(*delegate),
                event_sender,
                message_receiver,
                delegate,
            }
        }
    }

    fn dispatch_event(&self, event: CoreBluetoothEvent) {
        let s = self.event_sender.clone();
        task::block_on(async {
            s.send(event).await;
        });
    }

    fn on_discovered_peripheral(&mut self, peripheral: StrongPtr) {
        let uuid_nsstring = ns::uuid_uuidstring(cb::peer_identifier(*peripheral));
        let uuid = Uuid::from_str(&NSStringUtils::string_to_string(uuid_nsstring)).unwrap();
        let name = NSStringUtils::string_to_string(cb::peripheral_name(*peripheral));
        if self.peripherals.contains_key(&uuid) {
            if name != String::from("nil") {
                self.dispatch_event(CoreBluetoothEvent::DeviceUpdated(uuid, name));
            }
        } else {
            // if name.contains("LVS") {
            //     self.connect_peripheral(*peripheral);
            // }
            // Create our channels
            let (event_sender, event_receiver) = async_std::sync::channel(256);
            self.peripherals
                .insert(uuid, CBPeripheral::new(peripheral, event_sender));
            self.dispatch_event(CoreBluetoothEvent::DeviceDiscovered(
                uuid,
                name,
                event_receiver,
            ));
        }
    }

    fn on_discovered_services(
        &mut self,
        peripheral_uuid: Uuid,
        service_map: HashMap<Uuid, StrongPtr>,
    ) {
        info!("Found services!");
        for id in service_map.keys() {
            info!("{}", id);
        }
        if let Some(p) = self.peripherals.get_mut(&peripheral_uuid) {
            p.set_services(service_map);
        }
    }

    fn on_discovered_characteristics(
        &mut self,
        peripheral_uuid: Uuid,
        char_map: HashMap<Uuid, StrongPtr>,
    ) {
        info!("Found chars!");
        for id in char_map.keys() {
            info!("{}", id);
        }
        if let Some(p) = self.peripherals.get_mut(&peripheral_uuid) {
            p.set_characteristics(char_map);
        }
    }

    fn on_peripheral_connect(&mut self, peripheral_uuid: Uuid) {
        // Don't actually do anything here. The peripheral will fire the future
        // itself when it receives all of its service/characteristic info.
    }

    fn on_peripheral_disconnect(&mut self, peripheral_uuid: Uuid) {
        self.peripherals.remove(&peripheral_uuid);
        self.dispatch_event(CoreBluetoothEvent::DeviceLost(peripheral_uuid));
    }

    fn on_characteristic_subscribed(&mut self, peripheral_uuid: Uuid, characteristic_uuid: Uuid) {
        if let Some(p) = self.peripherals.get_mut(&peripheral_uuid) {
            if let Some(c) = p.characteristics.get_mut(&characteristic_uuid) {
                info!("Got subscribed event!");
                let state = c.subscribe_future_state.pop_back().unwrap();
                state.lock().unwrap().set_reply(CoreBluetoothReply::Ok);
            }
        }
    }

    fn on_characteristic_unsubscribed(&mut self, peripheral_uuid: Uuid, characteristic_uuid: Uuid) {
        if let Some(p) = self.peripherals.get_mut(&peripheral_uuid) {
            if let Some(c) = p.characteristics.get_mut(&characteristic_uuid) {
                info!("Got unsubscribed event!");
                let state = c.unsubscribe_future_state.pop_back().unwrap();
                state.lock().unwrap().set_reply(CoreBluetoothReply::Ok);
            }
        }
    }

    fn on_characteristic_read(
        &mut self,
        peripheral_uuid: Uuid,
        characteristic_uuid: Uuid,
        data: Vec<u8>,
    ) {
        if let Some(p) = self.peripherals.get_mut(&peripheral_uuid) {
            if let Some(c) = p.characteristics.get_mut(&characteristic_uuid) {
                info!("Got read event!");
                let state = c.read_future_state.pop_back().unwrap();
                let mut data_clone = Vec::new();
                for byte in data.iter() {
                    data_clone.push(*byte);
                }
                state.lock().unwrap().set_reply(CoreBluetoothReply::ReadResult(data_clone));
                task::block_on(async {
                    p.event_sender
                        .send(CBPeripheralEvent::Notification(characteristic_uuid, data))
                        .await;
                });
            }
        }
    }

    fn on_characteristic_written(
        &mut self,
        peripheral_uuid: Uuid,
        characteristic_uuid: Uuid,
    ) {
        if let Some(p) = self.peripherals.get_mut(&peripheral_uuid) {
            if let Some(c) = p.characteristics.get_mut(&characteristic_uuid) {
                info!("Got written event!");
                let state = c.write_future_state.pop_back().unwrap();
                state.lock().unwrap().set_reply(CoreBluetoothReply::Ok);
            }
        }
    }

    fn connect_peripheral(&mut self, peripheral_uuid: Uuid, fut: CoreBluetoothReplyStateShared) {
        info!("Trying to connect peripheral!");
        if let Some(p) = self.peripherals.get_mut(&peripheral_uuid) {
            info!("Connecting peripheral!");
            p.connected_future_state = Some(fut);
            cb::centralmanager_connectperipheral(*self.manager, *p.peripheral);
        }
    }

    fn write_value(
        &mut self,
        peripheral_uuid: Uuid,
        characteristic_uuid: Uuid,
        data: Vec<u8>,
        fut: CoreBluetoothReplyStateShared,
        with_response: bool
    ) {
        if let Some(p) = self.peripherals.get_mut(&peripheral_uuid) {
            if let Some(c) = p.characteristics.get_mut(&characteristic_uuid) {
                info!("Writing value! With response: {}", with_response);
                cb::peripheral_writevalue_forcharacteristic(
                    *p.peripheral,
                    ns::data(data.as_ptr(), data.len() as c_uint),
                    *c.characteristic,
                    if with_response { 1 } else { 0 },
                );
                if with_response {
                    c.write_future_state.push_front(fut);
                } else {
                    fut.lock().unwrap().set_reply(CoreBluetoothReply::Ok);
                }
            }
        }
    }

    fn read_value(
        &mut self,
        peripheral_uuid: Uuid,
        characteristic_uuid: Uuid,
        fut: CoreBluetoothReplyStateShared,
    ) {
        if let Some(p) = self.peripherals.get_mut(&peripheral_uuid) {
            if let Some(c) = p.characteristics.get_mut(&characteristic_uuid) {
                info!("Reading value!");
                cb::peripheral_readvalue_forcharacteristic(
                    *p.peripheral,
                    *c.characteristic,
                );
                c.read_future_state.push_front(fut);
            }
        }
    }

    fn subscribe(
        &mut self,
        peripheral_uuid: Uuid,
        characteristic_uuid: Uuid,
        fut: CoreBluetoothReplyStateShared,
    ) {
        if let Some(p) = self.peripherals.get_mut(&peripheral_uuid) {
            if let Some(c) = p.characteristics.get_mut(&characteristic_uuid) {
                info!("Setting subscribe!");
                cb::peripheral_setnotifyvalue_forcharacteristic(
                    *p.peripheral,
                    objc::runtime::YES,
                    *c.characteristic,
                );
                c.subscribe_future_state.push_front(fut);
            }
        }
    }

    fn unsubscribe(
        &mut self,
        peripheral_uuid: Uuid,
        characteristic_uuid: Uuid,
        fut: CoreBluetoothReplyStateShared,
    ) {
        if let Some(p) = self.peripherals.get_mut(&peripheral_uuid) {
            if let Some(c) = p.characteristics.get_mut(&characteristic_uuid) {
                info!("Setting subscribe!");
                cb::peripheral_setnotifyvalue_forcharacteristic(
                    *p.peripheral,
                    objc::runtime::NO,
                    *c.characteristic,
                );
                c.unsubscribe_future_state.push_front(fut);
            }
        }
    }

    pub fn wait_for_message(&mut self) -> bool {
        let mut delegate_receiver_clone = self.delegate_receiver.clone();
        let delegate_future =
            async { InternalLoopMessage::Delegate(delegate_receiver_clone.next().await.unwrap()) };

        let mut adapter_receiver_clone = self.message_receiver.clone();

        let adapter_future =
            async { InternalLoopMessage::Adapter(adapter_receiver_clone.next().await.unwrap()) };

        let msg = task::block_on(async {
            let race_future = delegate_future.race(adapter_future);
            race_future.await
        });

        match msg {
            InternalLoopMessage::Delegate(delegate_msg) => {
                match delegate_msg {
                    // TODO DidUpdateState does not imply that the adapter is
                    // on, just that it updated state.
                    //
                    // TODO We should probably also register some sort of
                    // "ready" variable in our adapter that will cause scans/etc
                    // to fail if this hasn't updated.
                    CentralDelegateEvent::DidUpdateState => {
                        self.dispatch_event(CoreBluetoothEvent::AdapterConnected)
                    }
                    CentralDelegateEvent::DiscoveredPeripheral(peripheral) => {
                        self.on_discovered_peripheral(peripheral)
                    }
                    CentralDelegateEvent::DiscoveredServices(peripheral_id, service_map) => {
                        self.on_discovered_services(peripheral_id, service_map)
                    }
                    CentralDelegateEvent::DiscoveredCharacteristics(peripheral_id, char_map) => {
                        self.on_discovered_characteristics(peripheral_id, char_map)
                    }
                    CentralDelegateEvent::ConnectedDevice(peripheral_id) => {
                        self.on_peripheral_connect(peripheral_id)
                    }
                    CentralDelegateEvent::DisconnectedDevice(peripheral_id) => {
                        self.on_peripheral_disconnect(peripheral_id)
                    }
                    CentralDelegateEvent::CharacteristicSubscribed(
                        peripheral_id,
                        characteristic_id,
                    ) => self.on_characteristic_subscribed(peripheral_id, characteristic_id),
                    CentralDelegateEvent::CharacteristicUnsubscribed(
                        peripheral_id,
                        characteristic_id,
                    ) => self.on_characteristic_unsubscribed(peripheral_id, characteristic_id),
                    CentralDelegateEvent::CharacteristicNotified(
                        peripheral_id,
                        characteristic_id,
                        data,
                    ) => self.on_characteristic_read(peripheral_id, characteristic_id, data),
                    CentralDelegateEvent::CharacteristicWritten(
                        peripheral_id,
                        characteristic_id
                    ) => self.on_characteristic_written(peripheral_id, characteristic_id),
                    _ => info!("Unknown type!"),
                };
                true
            }
            InternalLoopMessage::Adapter(adapter_msg) => {
                info!("Adapter message!");
                match adapter_msg {
                    CoreBluetoothMessage::StartScanning => self.start_discovery(),
                    CoreBluetoothMessage::StopScanning => self.stop_discovery(),
                    CoreBluetoothMessage::ConnectDevice(peripheral_uuid, fut) => {
                        info!("got connectdevice msg!");
                        self.connect_peripheral(peripheral_uuid, fut);
                    }
                    CoreBluetoothMessage::DisconnectDevice(peripheral_uuid, fut) => {}
                    CoreBluetoothMessage::ReadValue(peripheral_uuid, char_uuid, fut) => {
                        self.read_value(peripheral_uuid, char_uuid, fut)
                    }
                    CoreBluetoothMessage::WriteValue(peripheral_uuid, char_uuid, data, fut) => {
                        self.write_value(peripheral_uuid, char_uuid, data, fut, false)
                    }
                    CoreBluetoothMessage::WriteValueWithResponse(peripheral_uuid, char_uuid, data, fut) => {
                        self.write_value(peripheral_uuid, char_uuid, data, fut, true)
                    }
                    CoreBluetoothMessage::Subscribe(peripheral_uuid, char_uuid, fut) => {
                        self.subscribe(peripheral_uuid, char_uuid, fut)
                    }
                    CoreBluetoothMessage::Unsubscribe(peripheral_uuid, char_uuid, fut) => {
                        self.unsubscribe(peripheral_uuid, char_uuid, fut)
                    }
                    _ => {}
                };
                true
            }
            LoopFinished => false,
        }
    }

    fn start_discovery(&mut self) {
        trace!("BluetoothAdapter::start_discovery");
        let options = ns::mutabledictionary();
        // NOTE: If duplicates are not allowed then a peripheral will not show
        // up again once connected and then disconnected.
        ns::mutabledictionary_setobject_forkey(options, ns::number_withbool(YES), unsafe {
            cb::CENTRALMANAGERSCANOPTIONALLOWDUPLICATESKEY
        });
        cb::centralmanager_scanforperipherals_options(*self.manager, options);
    }

    fn stop_discovery(&mut self) {
        trace!("BluetoothAdapter::stop_discovery");
        cb::centralmanager_stopscan(*self.manager);
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

pub fn run_corebluetooth_thread(
    event_sender: Sender<CoreBluetoothEvent>,
) -> Sender<CoreBluetoothMessage> {
    let (sender, receiver) = channel::<CoreBluetoothMessage>(256);
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
