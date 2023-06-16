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
    framework::{
        cb::{self, CBManagerAuthorization, CBPeripheralState},
        ns,
    },
    future::{BtlePlugFuture, BtlePlugFutureStateShared},
    utils::{
        core_bluetooth::{cbuuid_to_uuid, uuid_to_cbuuid},
        nsstring::nsstring_to_string,
        nsuuid_to_uuid,
    },
};
use crate::api::{
    bleuuid::uuid_from_u16, CharPropFlags, Characteristic, Descriptor, ScanFilter, Service,
    WriteType,
};
use crate::Error;
use cocoa::{
    base::{id, nil},
    foundation::NSArray,
};
use futures::channel::mpsc::{self, Receiver, Sender};
use futures::select;
use futures::sink::SinkExt;
use futures::stream::{Fuse, StreamExt};
use log::{error, trace, warn};
use objc::{rc::StrongPtr, runtime::YES};
use std::{
    collections::{BTreeSet, HashMap, VecDeque},
    fmt::{self, Debug, Formatter},
    ops::Deref,
    thread,
};
use tokio::runtime;
use uuid::Uuid;

struct CBDescriptor {
    pub descriptor: StrongPtr,
    pub uuid: Uuid,
}

impl CBDescriptor {
    pub fn new(descriptor: StrongPtr) -> Self {
        let uuid = cbuuid_to_uuid(cb::attribute_uuid(*descriptor));
        Self { descriptor, uuid }
    }
}

struct CBCharacteristic {
    pub characteristic: StrongPtr,
    pub uuid: Uuid,
    pub properties: CharPropFlags,
    pub descriptors: HashMap<Uuid, CBDescriptor>,
    pub read_future_state: VecDeque<CoreBluetoothReplyStateShared>,
    pub write_future_state: VecDeque<CoreBluetoothReplyStateShared>,
    pub subscribe_future_state: VecDeque<CoreBluetoothReplyStateShared>,
    pub unsubscribe_future_state: VecDeque<CoreBluetoothReplyStateShared>,
    pub discovered: bool,
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
        let descriptors_arr = cb::characteristic_descriptors(*characteristic);
        let mut descriptors = HashMap::new();
        for i in 0..ns::array_count(descriptors_arr) {
            let d = ns::array_objectatindex(descriptors_arr, i);
            let descriptor = CBDescriptor::new(unsafe { StrongPtr::retain(d) });
            descriptors.insert(descriptor.uuid, descriptor);
        }
        Self {
            characteristic,
            uuid,
            properties,
            descriptors,
            read_future_state: VecDeque::with_capacity(10),
            write_future_state: VecDeque::with_capacity(10),
            subscribe_future_state: VecDeque::with_capacity(10),
            unsubscribe_future_state: VecDeque::with_capacity(10),
            discovered: false,
        }
    }

    fn form_flags(characteristic: id) -> CharPropFlags {
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
        trace!("Flags: {:?}", v);
        v
    }
}

#[derive(Clone, Debug)]
pub enum CoreBluetoothReply {
    ReadResult(Vec<u8>),
    Connected(BTreeSet<Service>),
    State(CBPeripheralState),
    Ok,
    Err(String),
}

#[derive(Debug)]
pub enum CBPeripheralEvent {
    Disconnected,
    Notification(Uuid, Vec<u8>),
    ManufacturerData(u16, Vec<u8>, i16),
    ServiceData(HashMap<Uuid, Vec<u8>>, i16),
    Services(Vec<Uuid>, i16),
}

pub type CoreBluetoothReplyStateShared = BtlePlugFutureStateShared<CoreBluetoothReply>;
pub type CoreBluetoothReplyFuture = BtlePlugFuture<CoreBluetoothReply>;

struct ServiceInternal {
    cbservice: StrongPtr,
    characteristics: HashMap<Uuid, CBCharacteristic>,
    pub discovered: bool,
}

struct CBPeripheral {
    pub peripheral: StrongPtr,
    services: HashMap<Uuid, ServiceInternal>,
    pub event_sender: Sender<CBPeripheralEvent>,
    pub disconnected_future_state: Option<CoreBluetoothReplyStateShared>,
    pub connected_future_state: Option<CoreBluetoothReplyStateShared>,
}

impl Debug for CBPeripheral {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.debug_struct("CBPeripheral")
            .field("peripheral", self.peripheral.deref())
            .field(
                "services",
                &self
                    .services
                    .iter()
                    .map(|(service_uuid, service)| (service_uuid, service.characteristics.len()))
                    .collect::<HashMap<_, _>>(),
            )
            .field("event_sender", &self.event_sender)
            .field("connected_future_state", &self.connected_future_state)
            .finish()
    }
}

impl CBPeripheral {
    pub fn new(peripheral: StrongPtr, event_sender: Sender<CBPeripheralEvent>) -> Self {
        Self {
            peripheral,
            services: HashMap::new(),
            event_sender,
            connected_future_state: None,
            disconnected_future_state: None,
        }
    }

    pub fn set_characteristics(
        &mut self,
        service_uuid: Uuid,
        characteristics: HashMap<Uuid, StrongPtr>,
    ) {
        let characteristics = characteristics
            .into_iter()
            .map(|(characteristic_uuid, characteristic)| {
                (characteristic_uuid, CBCharacteristic::new(characteristic))
            })
            .collect();
        let service = self
            .services
            .get_mut(&service_uuid)
            .expect("Got characteristics for a service we don't know about");
        service.characteristics = characteristics;
        if service.characteristics.is_empty() {
            service.discovered = true;
            self.check_discovered();
        }
    }

    pub fn set_characteristic_descriptors(
        &mut self,
        service_uuid: Uuid,
        characteristic_uuid: Uuid,
        descriptors: HashMap<Uuid, StrongPtr>,
    ) {
        let descriptors = descriptors
            .into_iter()
            .map(|(descriptor_uuid, descriptor)| (descriptor_uuid, CBDescriptor::new(descriptor)))
            .collect();
        let service = self
            .services
            .get_mut(&service_uuid)
            .expect("Got descriptors for a service we don't know about");
        let characteristic = service
            .characteristics
            .get_mut(&characteristic_uuid)
            .expect("Got descriptors for a characteristic we don't know about");
        characteristic.descriptors = descriptors;
        characteristic.discovered = true;

        if !service
            .characteristics
            .values()
            .any(|characteristic| !characteristic.discovered)
        {
            service.discovered = true;
            self.check_discovered()
        }
    }

    fn check_discovered(&mut self) {
        // It's time for QUESTIONABLE ASSUMPTIONS.
        //
        // For sake of being lazy, we don't want to fire device connection until
        // we have all of our services and characteristics. We assume that
        // set_characteristics should be called once for every entry in the
        // service map. Once that's done, we're filled out enough and can send
        // back a Connected reply to the waiting future with all of the
        // characteristic info in it.
        if !self.services.values().any(|service| !service.discovered) {
            if self.connected_future_state.is_none() {
                panic!("We should still have a future at this point!");
            }
            let services = self
                .services
                .iter()
                .map(|(&service_uuid, service)| Service {
                    uuid: service_uuid,
                    primary: cb::service_isprimary(*service.cbservice) != objc::runtime::NO,
                    characteristics: service
                        .characteristics
                        .iter()
                        .map(|(&characteristic_uuid, characteristic)| {
                            let descriptors = characteristic
                                .descriptors
                                .iter()
                                .map(|(&descriptor_uuid, _)| Descriptor {
                                    uuid: descriptor_uuid,
                                    service_uuid,
                                    characteristic_uuid,
                                })
                                .collect();
                            Characteristic {
                                uuid: characteristic_uuid,
                                service_uuid,
                                descriptors,
                                properties: characteristic.properties,
                            }
                        })
                        .collect(),
                })
                .collect();
            self.connected_future_state
                .take()
                .unwrap()
                .lock()
                .unwrap()
                .set_reply(CoreBluetoothReply::Connected(services));
        }
    }

    pub fn confirm_disconnect(&mut self) {
        // Fulfill the disconnected future, if there is one.
        // There might not be a future if the device disconnects unexpectedly.
        if let Some(future) = self.disconnected_future_state.take() {
            future.lock().unwrap().set_reply(CoreBluetoothReply::Ok)
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
    StartScanning {
        filter: ScanFilter,
    },
    StopScanning,
    ConnectDevice {
        peripheral_uuid: Uuid,
        future: CoreBluetoothReplyStateShared,
    },
    DisconnectDevice {
        peripheral_uuid: Uuid,
        future: CoreBluetoothReplyStateShared,
    },
    ReadValue {
        peripheral_uuid: Uuid,
        service_uuid: Uuid,
        characteristic_uuid: Uuid,
        future: CoreBluetoothReplyStateShared,
    },
    WriteValue {
        peripheral_uuid: Uuid,
        service_uuid: Uuid,
        characteristic_uuid: Uuid,
        data: Vec<u8>,
        write_type: WriteType,
        future: CoreBluetoothReplyStateShared,
    },
    Subscribe {
        peripheral_uuid: Uuid,
        service_uuid: Uuid,
        characteristic_uuid: Uuid,
        future: CoreBluetoothReplyStateShared,
    },
    Unsubscribe {
        peripheral_uuid: Uuid,
        service_uuid: Uuid,
        characteristic_uuid: Uuid,
        future: CoreBluetoothReplyStateShared,
    },
    IsConnected {
        peripheral_uuid: Uuid,
        future: CoreBluetoothReplyStateShared,
    },
}

#[derive(Debug)]
pub enum CoreBluetoothEvent {
    AdapterConnected,
    DeviceDiscovered {
        uuid: Uuid,
        name: Option<String>,
        event_receiver: Receiver<CBPeripheralEvent>,
    },
    DeviceUpdated {
        uuid: Uuid,
        name: String,
    },
    DeviceDisconnected {
        uuid: Uuid,
    },
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
        rssi: i16,
    ) {
        trace!(
            "Got manufacturer data advertisement! {}: {:?}",
            manufacturer_id,
            manufacturer_data
        );
        if let Some(p) = self.peripherals.get_mut(&peripheral_uuid) {
            if let Err(e) = p
                .event_sender
                .send(CBPeripheralEvent::ManufacturerData(
                    manufacturer_id,
                    manufacturer_data,
                    rssi,
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
        rssi: i16,
    ) {
        trace!("Got service data advertisement! {:?}", service_data);
        if let Some(p) = self.peripherals.get_mut(&peripheral_uuid) {
            if let Err(e) = p
                .event_sender
                .send(CBPeripheralEvent::ServiceData(service_data, rssi))
                .await
            {
                error!("Error sending notification event: {}", e);
            }
        }
    }

    async fn on_services(&mut self, peripheral_uuid: Uuid, services: Vec<Uuid>, rssi: i16) {
        trace!("Got service advertisement! {:?}", services);
        if let Some(p) = self.peripherals.get_mut(&peripheral_uuid) {
            if let Err(e) = p
                .event_sender
                .send(CBPeripheralEvent::Services(services, rssi))
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
                self.dispatch_event(CoreBluetoothEvent::DeviceUpdated { uuid, name })
                    .await;
            }
        } else {
            // Create our channels
            let (event_sender, event_receiver) = mpsc::channel(256);
            self.peripherals
                .insert(uuid, CBPeripheral::new(peripheral, event_sender));
            self.dispatch_event(CoreBluetoothEvent::DeviceDiscovered {
                uuid,
                name,
                event_receiver,
            })
            .await;
        }
    }

    fn on_discovered_services(
        &mut self,
        peripheral_uuid: Uuid,
        service_map: HashMap<Uuid, StrongPtr>,
    ) {
        trace!("Found services!");
        for id in service_map.keys() {
            trace!("{}", id);
        }
        if let Some(p) = self.peripherals.get_mut(&peripheral_uuid) {
            let services = service_map
                .into_iter()
                .map(|(service_uuid, cbservice)| {
                    (
                        service_uuid,
                        ServiceInternal {
                            cbservice,
                            characteristics: HashMap::new(),
                            discovered: false,
                        },
                    )
                })
                .collect();
            p.services = services;
        }
    }

    fn on_discovered_characteristics(
        &mut self,
        peripheral_uuid: Uuid,
        service_uuid: Uuid,
        characteristics: HashMap<Uuid, StrongPtr>,
    ) {
        trace!(
            "Found characteristics for peripheral {} service {}:",
            peripheral_uuid,
            service_uuid
        );
        for id in characteristics.keys() {
            trace!("{}", id);
        }
        if let Some(p) = self.peripherals.get_mut(&peripheral_uuid) {
            p.set_characteristics(service_uuid, characteristics);
        }
    }

    fn on_discovered_characteristic_descriptors(
        &mut self,
        peripheral_uuid: Uuid,
        service_uuid: Uuid,
        characteristic_uuid: Uuid,
        descriptors: HashMap<Uuid, StrongPtr>,
    ) {
        trace!(
            "Found descriptors for peripheral {} service {} characteristic {}:",
            peripheral_uuid,
            service_uuid,
            characteristic_uuid,
        );
        for id in descriptors.keys() {
            trace!("{}", id);
        }
        if let Some(p) = self.peripherals.get_mut(&peripheral_uuid) {
            p.set_characteristic_descriptors(service_uuid, characteristic_uuid, descriptors);
        }
    }

    fn on_peripheral_connect(&mut self, _peripheral_uuid: Uuid) {
        // Don't actually do anything here. The peripheral will fire the future
        // itself when it receives all of its service/characteristic info.
    }

    fn on_peripheral_connection_failed(&mut self, peripheral_uuid: Uuid) {
        trace!("Got connection fail event!");
        if self.peripherals.contains_key(&peripheral_uuid) {
            let peripheral = self
                .peripherals
                .get_mut(&peripheral_uuid)
                .expect("If we're here we should have an ID");
            peripheral
                .connected_future_state
                .take()
                .unwrap()
                .lock()
                .unwrap()
                .set_reply(CoreBluetoothReply::Err(String::from("Connection failed")));
        }
    }

    async fn on_peripheral_disconnect(&mut self, peripheral_uuid: Uuid) {
        trace!("Got disconnect event!");
        if self.peripherals.contains_key(&peripheral_uuid) {
            if let Err(e) = self
                .peripherals
                .get_mut(&peripheral_uuid)
                .expect("If we're here we should have an ID")
                .event_sender
                .send(CBPeripheralEvent::Disconnected)
                .await
            {
                error!("Error sending notification event: {}", e);
            }
            // Unlike connect, we'll want to fulfill our disconnect future here, which means grabbing
            // our peripheral and having it fire, then dropping it and dispatching our event.
            self.peripherals
                .get_mut(&peripheral_uuid)
                .expect("If we're here we should have an ID")
                .confirm_disconnect();
            self.peripherals.remove(&peripheral_uuid);
            self.dispatch_event(CoreBluetoothEvent::DeviceDisconnected {
                uuid: peripheral_uuid,
            })
            .await;
        }
    }

    /// Get the CBCharacteristic for the given characteristic of the given peripheral, if it exists.
    fn get_characteristic(
        &mut self,
        peripheral_uuid: Uuid,
        service_uuid: Uuid,
        characteristic_uuid: Uuid,
    ) -> Option<&mut CBCharacteristic> {
        self.peripherals
            .get_mut(&peripheral_uuid)?
            .services
            .get_mut(&service_uuid)?
            .characteristics
            .get_mut(&characteristic_uuid)
    }

    fn on_characteristic_subscribed(
        &mut self,
        peripheral_uuid: Uuid,
        service_uuid: Uuid,
        characteristic_uuid: Uuid,
    ) {
        if let Some(characteristic) =
            self.get_characteristic(peripheral_uuid, service_uuid, characteristic_uuid)
        {
            trace!("Got subscribed event!");
            let state = characteristic.subscribe_future_state.pop_back().unwrap();
            state.lock().unwrap().set_reply(CoreBluetoothReply::Ok);
        }
    }

    fn on_characteristic_unsubscribed(
        &mut self,
        peripheral_uuid: Uuid,
        service_uuid: Uuid,
        characteristic_uuid: Uuid,
    ) {
        if let Some(characteristic) =
            self.get_characteristic(peripheral_uuid, service_uuid, characteristic_uuid)
        {
            trace!("Got unsubscribed event!");
            let state = characteristic.unsubscribe_future_state.pop_back().unwrap();
            state.lock().unwrap().set_reply(CoreBluetoothReply::Ok);
        }
    }

    async fn on_characteristic_read(
        &mut self,
        peripheral_uuid: Uuid,
        service_uuid: Uuid,
        characteristic_uuid: Uuid,
        data: Vec<u8>,
    ) {
        if let Some(peripheral) = self.peripherals.get_mut(&peripheral_uuid) {
            if let Some(service) = peripheral.services.get_mut(&service_uuid) {
                if let Some(characteristic) = service.characteristics.get_mut(&characteristic_uuid)
                {
                    trace!("Got read event!");

                    let mut data_clone = Vec::new();
                    for byte in data.iter() {
                        data_clone.push(*byte);
                    }
                    // Reads and notifications both return the same callback. If
                    // we're trying to do a read, we'll have a future we can
                    // fulfill. Otherwise, just treat the returned value as a
                    // notification and use the event system.
                    if !characteristic.read_future_state.is_empty() {
                        let state = characteristic.read_future_state.pop_back().unwrap();
                        state
                            .lock()
                            .unwrap()
                            .set_reply(CoreBluetoothReply::ReadResult(data_clone));
                    } else if let Err(e) = peripheral
                        .event_sender
                        .send(CBPeripheralEvent::Notification(characteristic_uuid, data))
                        .await
                    {
                        error!("Error sending notification event: {}", e);
                    }
                }
            }
        }
    }

    fn on_characteristic_written(
        &mut self,
        peripheral_uuid: Uuid,
        service_uuid: Uuid,
        characteristic_uuid: Uuid,
    ) {
        if let Some(characteristic) =
            self.get_characteristic(peripheral_uuid, service_uuid, characteristic_uuid)
        {
            trace!("Got written event!");
            let state = characteristic.write_future_state.pop_back().unwrap();
            state.lock().unwrap().set_reply(CoreBluetoothReply::Ok);
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

    fn disconnect_peripheral(&mut self, peripheral_uuid: Uuid, fut: CoreBluetoothReplyStateShared) {
        trace!("Trying to disconnect peripheral!");
        if let Some(p) = self.peripherals.get_mut(&peripheral_uuid) {
            trace!("Disconnecting peripheral!");
            p.disconnected_future_state = Some(fut);
            cb::centralmanager_cancelperipheralconnection(*self.manager, *p.peripheral);
        }
    }

    fn is_connected(&mut self, peripheral_uuid: Uuid, fut: CoreBluetoothReplyStateShared) {
        if let Some(p) = self.peripherals.get_mut(&peripheral_uuid) {
            let state = cb::peripheral_state(*p.peripheral);
            trace!("Connected state {:?} ", state);
            fut.lock()
                .unwrap()
                .set_reply(CoreBluetoothReply::State(state));
        }
    }

    fn write_value(
        &mut self,
        peripheral_uuid: Uuid,
        service_uuid: Uuid,
        characteristic_uuid: Uuid,
        data: Vec<u8>,
        kind: WriteType,
        fut: CoreBluetoothReplyStateShared,
    ) {
        if let Some(peripheral) = self.peripherals.get_mut(&peripheral_uuid) {
            if let Some(service) = peripheral.services.get_mut(&service_uuid) {
                if let Some(characteristic) = service.characteristics.get_mut(&characteristic_uuid)
                {
                    trace!("Writing value! With kind {:?}", kind);
                    cb::peripheral_writevalue_forcharacteristic(
                        *peripheral.peripheral,
                        ns::data(&data),
                        *characteristic.characteristic,
                        match kind {
                            WriteType::WithResponse => 0,
                            WriteType::WithoutResponse => 1,
                        },
                    );
                    // WriteWithoutResponse does not call the corebluetooth
                    // callback, it just always succeeds silently.
                    if kind == WriteType::WithoutResponse {
                        fut.lock().unwrap().set_reply(CoreBluetoothReply::Ok);
                    } else {
                        characteristic.write_future_state.push_front(fut);
                    }
                }
            }
        }
    }

    fn read_value(
        &mut self,
        peripheral_uuid: Uuid,
        service_uuid: Uuid,
        characteristic_uuid: Uuid,
        fut: CoreBluetoothReplyStateShared,
    ) {
        if let Some(peripheral) = self.peripherals.get_mut(&peripheral_uuid) {
            if let Some(service) = peripheral.services.get_mut(&service_uuid) {
                if let Some(characteristic) = service.characteristics.get_mut(&characteristic_uuid)
                {
                    trace!("Reading value!");
                    cb::peripheral_readvalue_forcharacteristic(
                        *peripheral.peripheral,
                        *characteristic.characteristic,
                    );
                    characteristic.read_future_state.push_front(fut);
                }
            }
        }
    }

    fn subscribe(
        &mut self,
        peripheral_uuid: Uuid,
        service_uuid: Uuid,
        characteristic_uuid: Uuid,
        fut: CoreBluetoothReplyStateShared,
    ) {
        if let Some(peripheral) = self.peripherals.get_mut(&peripheral_uuid) {
            if let Some(service) = peripheral.services.get_mut(&service_uuid) {
                if let Some(characteristic) = service.characteristics.get_mut(&characteristic_uuid)
                {
                    trace!("Setting subscribe!");
                    cb::peripheral_setnotifyvalue_forcharacteristic(
                        *peripheral.peripheral,
                        objc::runtime::YES,
                        *characteristic.characteristic,
                    );
                    characteristic.subscribe_future_state.push_front(fut);
                }
            }
        }
    }

    fn unsubscribe(
        &mut self,
        peripheral_uuid: Uuid,
        service_uuid: Uuid,
        characteristic_uuid: Uuid,
        fut: CoreBluetoothReplyStateShared,
    ) {
        if let Some(peripheral) = self.peripherals.get_mut(&peripheral_uuid) {
            if let Some(service) = peripheral.services.get_mut(&service_uuid) {
                if let Some(characteristic) = service.characteristics.get_mut(&characteristic_uuid)
                {
                    trace!("Setting subscribe!");
                    cb::peripheral_setnotifyvalue_forcharacteristic(
                        *peripheral.peripheral,
                        objc::runtime::NO,
                        *characteristic.characteristic,
                    );
                    characteristic.unsubscribe_future_state.push_front(fut);
                }
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
                    CentralDelegateEvent::DiscoveredPeripheral{cbperipheral} => {
                        self.on_discovered_peripheral(cbperipheral).await
                    }
                    CentralDelegateEvent::DiscoveredServices{peripheral_uuid, services} => {
                        self.on_discovered_services(peripheral_uuid, services)
                    }
                    CentralDelegateEvent::DiscoveredCharacteristics{peripheral_uuid, service_uuid, characteristics} => {
                        self.on_discovered_characteristics(peripheral_uuid, service_uuid, characteristics)
                    }
                    CentralDelegateEvent::DiscoveredCharacteristicDescriptors{peripheral_uuid, service_uuid, characteristic_uuid, descriptors} => {
                        self.on_discovered_characteristic_descriptors(peripheral_uuid, service_uuid, characteristic_uuid, descriptors)
                    }
                    CentralDelegateEvent::ConnectedDevice{peripheral_uuid} => {
                            self.on_peripheral_connect(peripheral_uuid)
                    },
                    CentralDelegateEvent::ConnectionFailed{peripheral_uuid} => {
                        self.on_peripheral_connection_failed(peripheral_uuid)
                    },
                    CentralDelegateEvent::DisconnectedDevice{peripheral_uuid} => {
                        self.on_peripheral_disconnect(peripheral_uuid).await
                    }
                    CentralDelegateEvent::CharacteristicSubscribed{
                        peripheral_uuid,
                        service_uuid,
                        characteristic_uuid,
                     } => self.on_characteristic_subscribed(peripheral_uuid, service_uuid, characteristic_uuid),
                    CentralDelegateEvent::CharacteristicUnsubscribed{
                        peripheral_uuid,
                        service_uuid,
                        characteristic_uuid,
                     } => self.on_characteristic_unsubscribed(peripheral_uuid, service_uuid,characteristic_uuid),
                    CentralDelegateEvent::CharacteristicNotified{
                        peripheral_uuid,
                        service_uuid,
                        characteristic_uuid,
                        data,
                     } => self.on_characteristic_read(peripheral_uuid, service_uuid,characteristic_uuid, data).await,
                    CentralDelegateEvent::CharacteristicWritten{
                        peripheral_uuid,
                        service_uuid,
                        characteristic_uuid,
                    } => self.on_characteristic_written(peripheral_uuid, service_uuid, characteristic_uuid),
                    CentralDelegateEvent::ManufacturerData{peripheral_uuid, manufacturer_id, data, rssi} => {
                        self.on_manufacturer_data(peripheral_uuid, manufacturer_id, data, rssi).await
                    },
                    CentralDelegateEvent::ServiceData{peripheral_uuid, service_data, rssi} => {
                        self.on_service_data(peripheral_uuid, service_data, rssi).await
                    },
                    CentralDelegateEvent::Services{peripheral_uuid, service_uuids, rssi} => {
                        self.on_services(peripheral_uuid, service_uuids, rssi).await
                    },
                };
            }
            adapter_msg = self.message_receiver.select_next_some() => {
                trace!("Adapter message!");
                match adapter_msg {
                    CoreBluetoothMessage::StartScanning{filter} => self.start_discovery(filter),
                    CoreBluetoothMessage::StopScanning => self.stop_discovery(),
                    CoreBluetoothMessage::ConnectDevice{peripheral_uuid, future} => {
                        trace!("got connectdevice msg!");
                        self.connect_peripheral(peripheral_uuid, future);
                    }
                    CoreBluetoothMessage::DisconnectDevice{peripheral_uuid, future} => {
                        self.disconnect_peripheral(peripheral_uuid, future);
                    }
                    CoreBluetoothMessage::ReadValue{peripheral_uuid, service_uuid,characteristic_uuid, future} => {
                        self.read_value(peripheral_uuid, service_uuid,characteristic_uuid, future)
                    }
                    CoreBluetoothMessage::WriteValue{
                        peripheral_uuid,service_uuid,
                        characteristic_uuid,
                        data,
                        write_type,
                        future,
                    } => self.write_value(peripheral_uuid, service_uuid,characteristic_uuid, data, write_type, future),
                    CoreBluetoothMessage::Subscribe{peripheral_uuid, service_uuid,characteristic_uuid, future} => {
                        self.subscribe(peripheral_uuid, service_uuid,characteristic_uuid, future)
                    }
                    CoreBluetoothMessage::Unsubscribe{peripheral_uuid, service_uuid,characteristic_uuid, future} => {
                        self.unsubscribe(peripheral_uuid, service_uuid,characteristic_uuid, future)
                    }
                    CoreBluetoothMessage::IsConnected{peripheral_uuid, future} => {
                        self.is_connected(peripheral_uuid, future);
                    }
                };
            }
        }
    }

    fn start_discovery(&mut self, filter: ScanFilter) {
        trace!("BluetoothAdapter::start_discovery");
        let service_uuids = scan_filter_to_service_uuids(filter);
        let options = ns::mutabledictionary();
        // NOTE: If duplicates are not allowed then a peripheral will not show
        // up again once connected and then disconnected.
        ns::mutabledictionary_setobject_forkey(options, ns::number_withbool(YES), unsafe {
            cb::CENTRALMANAGERSCANOPTIONALLOWDUPLICATESKEY
        });
        cb::centralmanager_scanforperipheralswithservices_options(
            *self.manager,
            service_uuids,
            options,
        );
    }

    fn stop_discovery(&mut self) {
        trace!("BluetoothAdapter::stop_discovery");
        cb::centralmanager_stopscan(*self.manager);
    }
}

/// Convert a `ScanFilter` to the appropriate `NSArray<CBUUID *> *` to use for discovery. If the
/// filter has an empty list of services then this will return `nil`, to discover all devices.
fn scan_filter_to_service_uuids(filter: ScanFilter) -> id {
    if filter.services.is_empty() {
        nil
    } else {
        let service_uuids = filter
            .services
            .into_iter()
            .map(uuid_to_cbuuid)
            .collect::<Vec<_>>();
        unsafe { NSArray::arrayWithObjects(nil, &service_uuids) }
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
) -> Result<Sender<CoreBluetoothMessage>, Error> {
    let authorization = cb::manager_authorization();
    if authorization != CBManagerAuthorization::AllowedAlways
        && authorization != CBManagerAuthorization::NotDetermined
    {
        warn!("Authorization status {:?}", authorization);
        return Err(Error::PermissionDenied);
    } else {
        trace!("Authorization status {:?}", authorization);
    }
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
    Ok(sender)
}
