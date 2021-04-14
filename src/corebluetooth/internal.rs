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
    future::{BtlePlugFuture, BtlePlugFutureStateShared},
    utils::{core_bluetooth::cbuuid_to_uuid, nsstring::nsstring_to_string, nsuuid_to_uuid},
};
use crate::api::{CharPropFlags, Characteristic, WriteType};
use futures::channel::mpsc::{self, Receiver, Sender};
use futures::select;
use futures::sink::SinkExt;
use futures::stream::{Fuse, StreamExt};
use log::{error, info, trace};
use objc::{
    rc::StrongPtr,
    runtime::{Object, YES},
};
use std::{
    collections::{BTreeSet, HashMap, VecDeque},
    fmt::{self, Debug, Formatter},
    ops::Deref,
    os::raw::c_uint,
    thread,
};
use tokio::runtime;
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

impl Debug for CBCharacteristic {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.debug_struct("CBCharacteristic")
            .field("characteristic", self.characteristic.deref())
            .field("uuid", &self.uuid)
            .field("properties", &self.properties)
            .field("read_future_state", &self.read_future_state)
            .field("write_future_state", &self.write_future_state)
            .field("subscribe_future_state", &self.subscribe_future_state)
            .field("unsubscribe_future_state", &self.unsubscribe_future_state)
            .finish()
    }
}

impl CBCharacteristic {
    pub fn new(characteristic: StrongPtr) -> Self {
        let properties = CBCharacteristic::form_flags(*characteristic);
        let uuid = cbuuid_to_uuid(cb::attribute_uuid(*characteristic));
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
        let mut v = CharPropFlags::default();
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

#[derive(Clone, Debug)]
pub enum CoreBluetoothReply {
    ReadResult(Vec<u8>),
    Connected(BTreeSet<Characteristic>),
    Ok,
    Err(String),
}

#[derive(Debug)]
pub enum CBPeripheralEvent {
    Disconnected,
    Notification(Uuid, Vec<u8>),
    ManufacturerData(u16, Vec<u8>),
    ServiceData(HashMap<Uuid, Vec<u8>>),
    Services(Vec<Uuid>),
}

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

impl Debug for CBPeripheral {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.debug_struct("CBPeripheral")
            .field("peripheral", self.peripheral.deref())
            .field("services", &self.services.keys().collect::<Vec<_>>())
            .field("characteristics", &self.characteristics)
            .field("event_sender", &self.event_sender)
            .field("connected_future_state", &self.connected_future_state)
            .field(
                "characteristic_update_count",
                &self.characteristic_update_count,
            )
            .finish()
    }
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
            for (&uuid, c) in &self.characteristics {
                let char = Characteristic {
                    uuid,
                    properties: c.properties,
                };
                trace!("{:?}", char.uuid);
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
    delegate_receiver: Fuse<Receiver<CentralDelegateEvent>>,
    // Out in the world beyond CoreBluetooth, we'll be async, so just
    // task::block this when sending even though it'll never actually block.
    event_sender: Sender<CoreBluetoothEvent>,
    message_receiver: Fuse<Receiver<CoreBluetoothMessage>>,
}

impl Debug for CoreBluetoothInternal {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.debug_struct("CoreBluetoothInternal")
            .field("manager", self.manager.deref())
            .field("delegate", self.delegate.deref())
            .field("peripherals", &self.peripherals)
            .field("delegate_receiver", &self.delegate_receiver)
            .field("event_sender", &self.event_sender)
            .field("message_receiver", &self.message_receiver)
            .finish()
    }
}

#[derive(Debug)]
pub enum CoreBluetoothMessage {
    StartScanning,
    StopScanning,
    ConnectDevice(Uuid, CoreBluetoothReplyStateShared),
    DisconnectDevice(Uuid, CoreBluetoothReplyStateShared),
    // device uuid, characteristic uuid, future
    ReadValue(Uuid, Uuid, CoreBluetoothReplyStateShared),
    // device uuid, characteristic uuid, data, kind, future
    WriteValue(
        Uuid,
        Uuid,
        Vec<u8>,
        WriteType,
        CoreBluetoothReplyStateShared,
    ),
    // device uuid, characteristic uuid, future
    Subscribe(Uuid, Uuid, CoreBluetoothReplyStateShared),
    // device uuid, characteristic uuid, future
    Unsubscribe(Uuid, Uuid, CoreBluetoothReplyStateShared),
}

#[derive(Debug)]
pub enum CoreBluetoothEvent {
    AdapterConnected,
    // name, identifier, event receiver, message sender
    DeviceDiscovered(Uuid, Option<String>, Receiver<CBPeripheralEvent>),
    DeviceUpdated(Uuid, String),
    // identifier
    DeviceLost(Uuid),
}

impl CoreBluetoothInternal {
    pub fn new(
        message_receiver: Receiver<CoreBluetoothMessage>,
        event_sender: Sender<CoreBluetoothEvent>,
    ) -> Self {
        // Pretty sure these come preallocated?
        unsafe {
            let (delegate, delegate_receiver) = CentralDelegate::delegate();
            let delegate = StrongPtr::new(delegate);
            Self {
                manager: StrongPtr::new(cb::centralmanager(*delegate)),
                peripherals: HashMap::new(),
                delegate_receiver: delegate_receiver.fuse(),
                event_sender,
                message_receiver: message_receiver.fuse(),
                delegate,
            }
        }
    }

    async fn dispatch_event(&self, event: CoreBluetoothEvent) {
        let mut s = self.event_sender.clone();
        if let Err(e) = s.send(event).await {
            error!("Error dispatching event: {:?}", e);
        }
    }

    async fn on_manufacturer_data(
        &mut self,
        peripheral_uuid: Uuid,
        manufacturer_id: u16,
        manufacturer_data: Vec<u8>,
    ) {
        info!(
            "Got manufacturer data advertisement! {}: {:?}",
            manufacturer_id, manufacturer_data
        );
        if let Some(p) = self.peripherals.get_mut(&peripheral_uuid) {
            if let Err(e) = p
                .event_sender
                .send(CBPeripheralEvent::ManufacturerData(
                    manufacturer_id,
                    manufacturer_data,
                ))
                .await
            {
                error!("Error sending notification event: {}", e);
            }
        }
    }

    async fn on_service_data(
        &mut self,
        peripheral_uuid: Uuid,
        service_data: HashMap<Uuid, Vec<u8>>,
    ) {
        info!("Got service data advertisement! {:?}", service_data);
        if let Some(p) = self.peripherals.get_mut(&peripheral_uuid) {
            if let Err(e) = p
                .event_sender
                .send(CBPeripheralEvent::ServiceData(service_data))
                .await
            {
                error!("Error sending notification event: {}", e);
            }
        }
    }

    async fn on_services(&mut self, peripheral_uuid: Uuid, services: Vec<Uuid>) {
        info!("Got service advertisement! {:?}", services);
        if let Some(p) = self.peripherals.get_mut(&peripheral_uuid) {
            if let Err(e) = p
                .event_sender
                .send(CBPeripheralEvent::Services(services))
                .await
            {
                error!("Error sending notification event: {}", e);
            }
        }
    }

    async fn on_discovered_peripheral(&mut self, peripheral: StrongPtr) {
        let uuid = nsuuid_to_uuid(cb::peer_identifier(*peripheral));
        let name = nsstring_to_string(cb::peripheral_name(*peripheral));
        if self.peripherals.contains_key(&uuid) {
            if let Some(name) = name {
                self.dispatch_event(CoreBluetoothEvent::DeviceUpdated(uuid, name))
                    .await;
            }
        } else {
            // Create our channels
            let (event_sender, event_receiver) = mpsc::channel(256);
            self.peripherals
                .insert(uuid, CBPeripheral::new(peripheral, event_sender));
            self.dispatch_event(CoreBluetoothEvent::DeviceDiscovered(
                uuid,
                name,
                event_receiver,
            ))
            .await;
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

    fn on_peripheral_connect(&mut self, _peripheral_uuid: Uuid) {
        // Don't actually do anything here. The peripheral will fire the future
        // itself when it receives all of its service/characteristic info.
    }

    async fn on_peripheral_disconnect(&mut self, peripheral_uuid: Uuid) {
        self.peripherals.remove(&peripheral_uuid);
        self.dispatch_event(CoreBluetoothEvent::DeviceLost(peripheral_uuid))
            .await;
    }

    fn on_characteristic_subscribed(&mut self, peripheral_uuid: Uuid, characteristic_uuid: Uuid) {
        if let Some(p) = self.peripherals.get_mut(&peripheral_uuid) {
            if let Some(c) = p.characteristics.get_mut(&characteristic_uuid) {
                trace!("Got subscribed event!");
                let state = c.subscribe_future_state.pop_back().unwrap();
                state.lock().unwrap().set_reply(CoreBluetoothReply::Ok);
            }
        }
    }

    fn on_characteristic_unsubscribed(&mut self, peripheral_uuid: Uuid, characteristic_uuid: Uuid) {
        if let Some(p) = self.peripherals.get_mut(&peripheral_uuid) {
            if let Some(c) = p.characteristics.get_mut(&characteristic_uuid) {
                trace!("Got unsubscribed event!");
                let state = c.unsubscribe_future_state.pop_back().unwrap();
                state.lock().unwrap().set_reply(CoreBluetoothReply::Ok);
            }
        }
    }

    async fn on_characteristic_read(
        &mut self,
        peripheral_uuid: Uuid,
        characteristic_uuid: Uuid,
        data: Vec<u8>,
    ) {
        if let Some(p) = self.peripherals.get_mut(&peripheral_uuid) {
            if let Some(c) = p.characteristics.get_mut(&characteristic_uuid) {
                trace!("Got read event!");

                let mut data_clone = Vec::new();
                for byte in data.iter() {
                    data_clone.push(*byte);
                }
                // Reads and notifications both return the same callback. If
                // we're trying to do a read, we'll have a future we can
                // fulfill. Otherwise, just treat the returned value as a
                // notification and use the event system.
                if !c.read_future_state.is_empty() {
                    let state = c.read_future_state.pop_back().unwrap();
                    state
                        .lock()
                        .unwrap()
                        .set_reply(CoreBluetoothReply::ReadResult(data_clone));
                } else if let Err(e) = p
                    .event_sender
                    .send(CBPeripheralEvent::Notification(characteristic_uuid, data))
                    .await
                {
                    error!("Error sending notification event: {}", e);
                }
            }
        }
    }

    fn on_characteristic_written(&mut self, peripheral_uuid: Uuid, characteristic_uuid: Uuid) {
        if let Some(p) = self.peripherals.get_mut(&peripheral_uuid) {
            if let Some(c) = p.characteristics.get_mut(&characteristic_uuid) {
                trace!("Got written event!");
                let state = c.write_future_state.pop_back().unwrap();
                state.lock().unwrap().set_reply(CoreBluetoothReply::Ok);
            }
        }
    }

    fn connect_peripheral(&mut self, peripheral_uuid: Uuid, fut: CoreBluetoothReplyStateShared) {
        trace!("Trying to connect peripheral!");
        if let Some(p) = self.peripherals.get_mut(&peripheral_uuid) {
            trace!("Connecting peripheral!");
            p.connected_future_state = Some(fut);
            cb::centralmanager_connectperipheral(*self.manager, *p.peripheral);
        }
    }

    fn write_value(
        &mut self,
        peripheral_uuid: Uuid,
        characteristic_uuid: Uuid,
        data: Vec<u8>,
        kind: WriteType,
        fut: CoreBluetoothReplyStateShared,
    ) {
        if let Some(p) = self.peripherals.get_mut(&peripheral_uuid) {
            if let Some(c) = p.characteristics.get_mut(&characteristic_uuid) {
                info!("Writing value! With kind {:?}", kind);
                cb::peripheral_writevalue_forcharacteristic(
                    *p.peripheral,
                    ns::data(data.as_ptr(), data.len() as c_uint),
                    *c.characteristic,
                    match kind {
                        WriteType::WithResponse => 0,
                        WriteType::WithoutResponse => 1,
                    },
                );
                c.write_future_state.push_front(fut);
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
                cb::peripheral_readvalue_forcharacteristic(*p.peripheral, *c.characteristic);
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

    async fn wait_for_message(&mut self) {
        select! {
            delegate_msg = self.delegate_receiver.select_next_some() => {
                match delegate_msg {
                    // TODO DidUpdateState does not imply that the adapter is
                    // on, just that it updated state.
                    //
                    // TODO We should probably also register some sort of
                    // "ready" variable in our adapter that will cause scans/etc
                    // to fail if this hasn't updated.
                    CentralDelegateEvent::DidUpdateState => {
                        self.dispatch_event(CoreBluetoothEvent::AdapterConnected).await
                    }
                    CentralDelegateEvent::DiscoveredPeripheral(peripheral) => {
                        self.on_discovered_peripheral(peripheral).await
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
                        self.on_peripheral_disconnect(peripheral_id).await
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
                    ) => self.on_characteristic_read(peripheral_id, characteristic_id, data).await,
                    CentralDelegateEvent::CharacteristicWritten(
                        peripheral_id,
                        characteristic_id,
                    ) => self.on_characteristic_written(peripheral_id, characteristic_id),
                    CentralDelegateEvent::ManufacturerData(peripheral_id, manufacturer_id, manufacturer_data) => {
                        self.on_manufacturer_data(peripheral_id, manufacturer_id, manufacturer_data).await
                    },
                    CentralDelegateEvent::ServiceData(peripheral_id, service_data) => {
                        self.on_service_data(peripheral_id, service_data).await
                    },
                    CentralDelegateEvent::Services(peripheral_id, services) => {
                        self.on_services(peripheral_id, services).await
                    },
                };
            }
            adapter_msg = self.message_receiver.select_next_some() => {
                info!("Adapter message!");
                match adapter_msg {
                    CoreBluetoothMessage::StartScanning => self.start_discovery(),
                    CoreBluetoothMessage::StopScanning => self.stop_discovery(),
                    CoreBluetoothMessage::ConnectDevice(peripheral_uuid, fut) => {
                        info!("got connectdevice msg!");
                        self.connect_peripheral(peripheral_uuid, fut);
                    }
                    CoreBluetoothMessage::DisconnectDevice(_peripheral_uuid, _fut) => {}
                    CoreBluetoothMessage::ReadValue(peripheral_uuid, char_uuid, fut) => {
                        self.read_value(peripheral_uuid, char_uuid, fut)
                    }
                    CoreBluetoothMessage::WriteValue(
                        peripheral_uuid,
                        char_uuid,
                        data,
                        kind,
                        fut,
                    ) => self.write_value(peripheral_uuid, char_uuid, data, kind, fut),
                    CoreBluetoothMessage::Subscribe(peripheral_uuid, char_uuid, fut) => {
                        self.subscribe(peripheral_uuid, char_uuid, fut)
                    }
                    CoreBluetoothMessage::Unsubscribe(peripheral_uuid, char_uuid, fut) => {
                        self.unsubscribe(peripheral_uuid, char_uuid, fut)
                    }
                };
            }
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
    let (sender, receiver) = mpsc::channel::<CoreBluetoothMessage>(256);
    // CoreBluetoothInternal is !Send, so we need to keep it on a single thread.
    thread::spawn(move || {
        let runtime = runtime::Builder::new_current_thread().build().unwrap();
        runtime.block_on(async move {
            let mut cbi = CoreBluetoothInternal::new(receiver, event_sender);
            loop {
                cbi.wait_for_message().await;
            }
        })
    });
    sender
}
