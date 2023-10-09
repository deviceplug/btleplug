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
    ffi,
    future::{BtlePlugFuture, BtlePlugFutureStateShared},
    utils::{
        core_bluetooth::{cbuuid_to_uuid, uuid_to_cbuuid},
        nsuuid_to_uuid,
    },
};
use crate::api::{CharPropFlags, Characteristic, Descriptor, ScanFilter, Service, WriteType};
use crate::Error;
use futures::channel::mpsc::{self, Receiver, Sender};
use futures::select;
use futures::sink::SinkExt;
use futures::stream::{Fuse, StreamExt};
use log::{error, trace, warn};
use objc2::{msg_send_id, ClassType};
use objc2::{rc::Retained, runtime::AnyObject};
use objc2_core_bluetooth::{
    CBCentralManager, CBCentralManagerScanOptionAllowDuplicatesKey, CBCharacteristic,
    CBCharacteristicProperties, CBCharacteristicWriteType, CBDescriptor, CBManager,
    CBManagerAuthorization, CBManagerState, CBPeripheral, CBPeripheralState, CBService, CBUUID,
};
use objc2_foundation::{NSArray, NSData, NSMutableDictionary, NSNumber};
use std::{
    collections::{BTreeSet, HashMap, VecDeque},
    ffi::CString,
    fmt::{self, Debug, Formatter},
    ops::Deref,
    thread,
};
use tokio::runtime;
use uuid::Uuid;

struct DescriptorInternal {
    pub descriptor: Retained<CBDescriptor>,
    pub uuid: Uuid,
    pub read_future_state: VecDeque<CoreBluetoothReplyStateShared>,
    pub write_future_state: VecDeque<CoreBluetoothReplyStateShared>,
}

impl DescriptorInternal {
    pub fn new(descriptor: Retained<CBDescriptor>) -> Self {
        let uuid = cbuuid_to_uuid(unsafe { &descriptor.UUID() });
        Self {
            descriptor,
            uuid,
            read_future_state: VecDeque::with_capacity(10),
            write_future_state: VecDeque::with_capacity(10),
        }
    }
}

struct CharacteristicInternal {
    pub characteristic: Retained<CBCharacteristic>,
    pub uuid: Uuid,
    pub properties: CharPropFlags,
    pub descriptors: HashMap<Uuid, DescriptorInternal>,
    pub read_future_state: VecDeque<CoreBluetoothReplyStateShared>,
    pub write_future_state: VecDeque<CoreBluetoothReplyStateShared>,
    pub subscribe_future_state: VecDeque<CoreBluetoothReplyStateShared>,
    pub unsubscribe_future_state: VecDeque<CoreBluetoothReplyStateShared>,
    pub discovered: bool,
}

impl Debug for CharacteristicInternal {
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

impl CharacteristicInternal {
    pub fn new(characteristic: Retained<CBCharacteristic>) -> Self {
        let properties = CharacteristicInternal::form_flags(&*characteristic);
        let uuid = cbuuid_to_uuid(unsafe { &characteristic.UUID() });
        let descriptors_arr = unsafe { characteristic.descriptors() };
        let mut descriptors = HashMap::new();
        if let Some(descriptors_arr) = descriptors_arr {
            for d in descriptors_arr {
                let descriptor = DescriptorInternal::new(d);
                descriptors.insert(descriptor.uuid, descriptor);
            }
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

    fn form_flags(characteristic: &CBCharacteristic) -> CharPropFlags {
        let flags = unsafe { characteristic.properties() };
        let mut v = CharPropFlags::default();
        if flags.contains(CBCharacteristicProperties::CBCharacteristicPropertyBroadcast) {
            v |= CharPropFlags::BROADCAST;
        }
        if flags.contains(CBCharacteristicProperties::CBCharacteristicPropertyRead) {
            v |= CharPropFlags::READ;
        }
        if flags.contains(CBCharacteristicProperties::CBCharacteristicPropertyWriteWithoutResponse)
        {
            v |= CharPropFlags::WRITE_WITHOUT_RESPONSE;
        }
        if flags.contains(CBCharacteristicProperties::CBCharacteristicPropertyWrite) {
            v |= CharPropFlags::WRITE;
        }
        if flags.contains(CBCharacteristicProperties::CBCharacteristicPropertyNotify) {
            v |= CharPropFlags::NOTIFY;
        }
        if flags.contains(CBCharacteristicProperties::CBCharacteristicPropertyIndicate) {
            v |= CharPropFlags::INDICATE;
        }
        if flags
            .contains(CBCharacteristicProperties::CBCharacteristicPropertyAuthenticatedSignedWrites)
        {
            v |= CharPropFlags::AUTHENTICATED_SIGNED_WRITES;
        }
        trace!("Flags: {:?}", v);
        v
    }
}

#[derive(Clone, Debug)]
pub enum CoreBluetoothReply {
    AdapterState(CBManagerState),
    ReadResult(Vec<u8>),
    Connected(BTreeSet<Service>),
    State(CBPeripheralState),
    Ok,
    Err(String),
}

#[derive(Debug)]
pub enum PeripheralEventInternal {
    Disconnected,
    Notification(Uuid, Vec<u8>),
    ManufacturerData(u16, Vec<u8>, i16),
    ServiceData(HashMap<Uuid, Vec<u8>>, i16),
    Services(Vec<Uuid>, i16),
}

pub type CoreBluetoothReplyStateShared = BtlePlugFutureStateShared<CoreBluetoothReply>;
pub type CoreBluetoothReplyFuture = BtlePlugFuture<CoreBluetoothReply>;

struct ServiceInternal {
    cbservice: Retained<CBService>,
    characteristics: HashMap<Uuid, CharacteristicInternal>,
    pub discovered: bool,
}

struct PeripheralInternal {
    pub peripheral: Retained<CBPeripheral>,
    services: HashMap<Uuid, ServiceInternal>,
    pub event_sender: Sender<PeripheralEventInternal>,
    pub disconnected_future_state: Option<CoreBluetoothReplyStateShared>,
    pub connected_future_state: Option<CoreBluetoothReplyStateShared>,
}

impl Debug for PeripheralInternal {
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

impl PeripheralInternal {
    pub fn new(
        peripheral: Retained<CBPeripheral>,
        event_sender: Sender<PeripheralEventInternal>,
    ) -> Self {
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
        characteristics: HashMap<Uuid, Retained<CBCharacteristic>>,
    ) {
        let characteristics = characteristics.into_iter().fold(
            // Only consider the first characteristic of each UUID
            // This "should" be unique, but of course it's not enforced
            HashMap::<Uuid, CharacteristicInternal>::new(),
            |mut map, (characteristic_uuid, characteristic)| {
                if !map.contains_key(&characteristic_uuid) {
                    map.insert(
                        characteristic_uuid,
                        CharacteristicInternal::new(characteristic),
                    );
                }
                map
            },
        );
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
        descriptors: HashMap<Uuid, Retained<CBDescriptor>>,
    ) {
        let descriptors = descriptors
            .into_iter()
            .map(|(descriptor_uuid, descriptor)| {
                (descriptor_uuid, DescriptorInternal::new(descriptor))
            })
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
                    primary: unsafe { service.cbservice.isPrimary() },
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

        // Fulfill all pending futures
        let error = CoreBluetoothReply::Err(String::from("Device disconnected"));
        self.services.iter().for_each(|(_, service)| {
            service
                .characteristics
                .iter()
                .for_each(|(_, characteristic)| {
                    let CharacteristicInternal {
                        read_future_state,
                        write_future_state,
                        subscribe_future_state,
                        unsubscribe_future_state,
                        ..
                    } = characteristic;

                    let futures = read_future_state
                        .into_iter()
                        .chain(write_future_state.into_iter())
                        .chain(subscribe_future_state.into_iter())
                        .chain(unsubscribe_future_state.into_iter());
                    for state in futures {
                        state.lock().unwrap().set_reply(error.clone());
                    }
                });
        });
    }
}

// All of CoreBluetooth is basically async. It's all just waiting on delegate
// events/callbacks. Therefore, we should be able to round up all of our wacky
// ass mut *Object values, keep them in a single struct, in a single thread, and
// call it good. Right?
struct CoreBluetoothInternal {
    manager: Retained<CBCentralManager>,
    delegate: Retained<CentralDelegate>,
    // Map of identifiers to object pointers
    peripherals: HashMap<Uuid, PeripheralInternal>,
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
    GetAdapterState {
        future: CoreBluetoothReplyStateShared,
    },
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
    ReadDescriptorValue {
        peripheral_uuid: Uuid,
        service_uuid: Uuid,
        characteristic_uuid: Uuid,
        descriptor_uuid: Uuid,
        future: CoreBluetoothReplyStateShared,
    },
    WriteDescriptorValue {
        peripheral_uuid: Uuid,
        service_uuid: Uuid,
        characteristic_uuid: Uuid,
        descriptor_uuid: Uuid,
        data: Vec<u8>,
        future: CoreBluetoothReplyStateShared,
    },
}

#[derive(Debug)]
pub enum CoreBluetoothEvent {
    DidUpdateState {
        state: CBManagerState,
    },
    DeviceDiscovered {
        uuid: Uuid,
        name: Option<String>,
        event_receiver: Receiver<PeripheralEventInternal>,
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
        let (sender, receiver) = mpsc::channel::<CentralDelegateEvent>(256);
        let delegate = CentralDelegate::new(sender);

        let label = CString::new("CBqueue").unwrap();
        let queue =
            unsafe { ffi::dispatch_queue_create(label.as_ptr(), ffi::DISPATCH_QUEUE_SERIAL) };
        let queue: *mut AnyObject = queue.cast();

        let manager = unsafe {
            msg_send_id![CBCentralManager::alloc(), initWithDelegate: &*delegate, queue: queue]
        };

        Self {
            manager,
            peripherals: HashMap::new(),
            delegate_receiver: receiver.fuse(),
            event_sender,
            message_receiver: message_receiver.fuse(),
            delegate,
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
                .send(PeripheralEventInternal::ManufacturerData(
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
                .send(PeripheralEventInternal::ServiceData(service_data, rssi))
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
                .send(PeripheralEventInternal::Services(services, rssi))
                .await
            {
                error!("Error sending notification event: {}", e);
            }
        }
    }

    async fn on_discovered_peripheral(
        &mut self,
        peripheral: Retained<CBPeripheral>,
        local_name: Option<String>,
    ) {
        let uuid = nsuuid_to_uuid(unsafe { &peripheral.identifier() });
        let peripheral_name = unsafe { peripheral.name() };

        let name = match (peripheral_name.map(|n| n.to_string()), local_name) {
            (Some(p_name), Some(l_name)) if p_name != l_name => {
                Some(format!("{p_name} [{l_name}]"))
            }
            (Some(p_name), Some(_)) => Some(p_name),
            (Some(p_name), None) => Some(p_name),
            (None, Some(l_name)) => Some(l_name),
            (None, None) => None,
        };

        if self.peripherals.contains_key(&uuid) {
            if let Some(name) = name {
                self.dispatch_event(CoreBluetoothEvent::DeviceUpdated {
                    uuid,
                    name: name.to_string(),
                })
                .await;
            }
        } else {
            // Create our channels
            let (event_sender, event_receiver) = mpsc::channel(256);
            self.peripherals
                .insert(uuid, PeripheralInternal::new(peripheral, event_sender));
            self.dispatch_event(CoreBluetoothEvent::DeviceDiscovered {
                uuid,
                name: name.map(|name| name.to_string()),
                event_receiver,
            })
            .await;
        }
    }

    fn on_discovered_services(
        &mut self,
        peripheral_uuid: Uuid,
        service_map: HashMap<Uuid, Retained<CBService>>,
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
        characteristics: HashMap<Uuid, Retained<CBCharacteristic>>,
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
        descriptors: HashMap<Uuid, Retained<CBDescriptor>>,
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

    fn on_peripheral_connection_failed(
        &mut self,
        peripheral_uuid: Uuid,
        error_description: Option<String>,
    ) {
        trace!("Got connection fail event!");
        let error = error_description.unwrap_or(String::from("Connection failed"));
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
                .set_reply(CoreBluetoothReply::Err(error));
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
                .send(PeripheralEventInternal::Disconnected)
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
    ) -> Option<&mut CharacteristicInternal> {
        self.peripherals
            .get_mut(&peripheral_uuid)?
            .services
            .get_mut(&service_uuid)?
            .characteristics
            .get_mut(&characteristic_uuid)
    }

    /// Get the CBDescriptor for the given descriptor of the given peripheral, if it exists.
    fn get_descriptor(
        &mut self,
        peripheral_uuid: Uuid,
        service_uuid: Uuid,
        characteristic_uuid: Uuid,
        descriptor_uuid: Uuid,
    ) -> Option<&mut DescriptorInternal> {
        self.get_characteristic(peripheral_uuid, service_uuid, characteristic_uuid)?
            .descriptors
            .get_mut(&descriptor_uuid)
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
            if let Some(state) = characteristic.subscribe_future_state.pop_back() {
                state.lock().unwrap().set_reply(CoreBluetoothReply::Ok);
            }
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
            if let Some(state) = characteristic.unsubscribe_future_state.pop_back() {
                state.lock().unwrap().set_reply(CoreBluetoothReply::Ok);
            }
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
                        .send(PeripheralEventInternal::Notification(
                            characteristic_uuid,
                            data,
                        ))
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
            unsafe { self.manager.connectPeripheral_options(&p.peripheral, None) };
        }
    }

    fn disconnect_peripheral(&mut self, peripheral_uuid: Uuid, fut: CoreBluetoothReplyStateShared) {
        trace!("Trying to disconnect peripheral!");
        if let Some(p) = self.peripherals.get_mut(&peripheral_uuid) {
            trace!("Disconnecting peripheral!");
            p.disconnected_future_state = Some(fut);
            unsafe { self.manager.cancelPeripheralConnection(&p.peripheral) };
        }
    }

    fn is_connected(&mut self, peripheral_uuid: Uuid, fut: CoreBluetoothReplyStateShared) {
        if let Some(p) = self.peripherals.get_mut(&peripheral_uuid) {
            let state = unsafe { p.peripheral.state() };
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
                    unsafe {
                        peripheral.peripheral.writeValue_forCharacteristic_type(
                            &NSData::from_vec(data),
                            &characteristic.characteristic,
                            match kind {
                                WriteType::WithResponse => {
                                    CBCharacteristicWriteType::CBCharacteristicWriteWithResponse
                                }
                                WriteType::WithoutResponse => {
                                    CBCharacteristicWriteType::CBCharacteristicWriteWithoutResponse
                                }
                            },
                        );
                    }
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
                    unsafe {
                        peripheral
                            .peripheral
                            .readValueForCharacteristic(&characteristic.characteristic);
                    }
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
                    unsafe {
                        peripheral
                            .peripheral
                            .setNotifyValue_forCharacteristic(true, &characteristic.characteristic);
                    }
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
                    unsafe {
                        peripheral.peripheral.setNotifyValue_forCharacteristic(
                            false,
                            &characteristic.characteristic,
                        );
                    }
                    characteristic.unsubscribe_future_state.push_front(fut);
                }
            }
        }
    }

    fn write_descriptor_value(
        &mut self,
        peripheral_uuid: Uuid,
        service_uuid: Uuid,
        characteristic_uuid: Uuid,
        descriptor_uuid: Uuid,
        data: Vec<u8>,
        fut: CoreBluetoothReplyStateShared,
    ) {
        if let Some(peripheral) = self.peripherals.get_mut(&peripheral_uuid) {
            if let Some(service) = peripheral.services.get_mut(&service_uuid) {
                if let Some(characteristic) = service.characteristics.get_mut(&characteristic_uuid)
                {
                    if let Some(descriptor) = characteristic.descriptors.get_mut(&descriptor_uuid) {
                        trace!("Writing descriptor value!");
                        unsafe {
                            peripheral.peripheral.writeValue_forDescriptor(
                                &NSData::from_vec(data),
                                &descriptor.descriptor,
                            );
                        }
                        descriptor.write_future_state.push_front(fut);
                    }
                }
            }
        }
    }

    fn read_descriptor_value(
        &mut self,
        peripheral_uuid: Uuid,
        service_uuid: Uuid,
        characteristic_uuid: Uuid,
        descriptor_uuid: Uuid,
        fut: CoreBluetoothReplyStateShared,
    ) {
        if let Some(peripheral) = self.peripherals.get_mut(&peripheral_uuid) {
            if let Some(service) = peripheral.services.get_mut(&service_uuid) {
                if let Some(characteristic) = service.characteristics.get_mut(&characteristic_uuid)
                {
                    if let Some(descriptor) = characteristic.descriptors.get_mut(&descriptor_uuid) {
                        trace!("Reading descriptor value!");
                        unsafe {
                            peripheral
                                .peripheral
                                .readValueForDescriptor(&descriptor.descriptor);
                        }
                        descriptor.read_future_state.push_front(fut);
                    }
                }
            }
        }
    }

    async fn on_descriptor_read(
        &mut self,
        peripheral_uuid: Uuid,
        service_uuid: Uuid,
        characteristic_uuid: Uuid,
        descriptor_uuid: Uuid,
        data: Vec<u8>,
    ) {
        if let Some(peripheral) = self.peripherals.get_mut(&peripheral_uuid) {
            if let Some(service) = peripheral.services.get_mut(&service_uuid) {
                if let Some(characteristic) = service.characteristics.get_mut(&characteristic_uuid)
                {
                    if let Some(descriptor) = characteristic.descriptors.get_mut(&descriptor_uuid) {
                        trace!("Got read event!");

                        let mut data_clone = Vec::new();
                        for byte in data.iter() {
                            data_clone.push(*byte);
                        }
                        let state = descriptor.read_future_state.pop_back().unwrap();
                        state
                            .lock()
                            .unwrap()
                            .set_reply(CoreBluetoothReply::ReadResult(data_clone));
                    }
                }
            }
        }
    }

    fn on_descriptor_written(
        &mut self,
        peripheral_uuid: Uuid,
        service_uuid: Uuid,
        characteristic_uuid: Uuid,
        descriptor_uuid: Uuid,
    ) {
        if let Some(descriptor) = self.get_descriptor(
            peripheral_uuid,
            service_uuid,
            characteristic_uuid,
            descriptor_uuid,
        ) {
            trace!("Got written event!");
            let state = descriptor.write_future_state.pop_back().unwrap();
            state.lock().unwrap().set_reply(CoreBluetoothReply::Ok);
        }
    }

    async fn wait_for_message(&mut self) {
        select! {
            delegate_msg = self.delegate_receiver.select_next_some() => {
                match delegate_msg {
                    // TODO We should probably also register some sort of
                    // "ready" variable in our adapter that will cause scans/etc
                    // to fail if this hasn't updated.
                    CentralDelegateEvent::DidUpdateState{state} => {
                        self.dispatch_event(CoreBluetoothEvent::DidUpdateState{state}).await
                    }
                    CentralDelegateEvent::DiscoveredPeripheral{cbperipheral, local_name} => {
                        self.on_discovered_peripheral(cbperipheral, local_name).await
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
                    CentralDelegateEvent::ConnectionFailed{peripheral_uuid, error_description} => {
                        self.on_peripheral_connection_failed(peripheral_uuid, error_description)
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
                    CentralDelegateEvent::DescriptorNotified{
                        peripheral_uuid,
                        service_uuid,
                        characteristic_uuid,
                        descriptor_uuid,
                        data,
                     } => self.on_descriptor_read(peripheral_uuid, service_uuid, characteristic_uuid, descriptor_uuid, data).await,
                    CentralDelegateEvent::DescriptorWritten{
                        peripheral_uuid,
                        service_uuid,
                        characteristic_uuid,
                        descriptor_uuid,
                    } => self.on_descriptor_written(peripheral_uuid, service_uuid, characteristic_uuid, descriptor_uuid),
                };
            }
            adapter_msg = self.message_receiver.select_next_some() => {
                trace!("Adapter message!");
                match adapter_msg {
                    CoreBluetoothMessage::GetAdapterState { future } => {
                        self.get_adapter_state(future);
                    },
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
                    },
                    CoreBluetoothMessage::ReadDescriptorValue{peripheral_uuid, service_uuid, characteristic_uuid, descriptor_uuid, future} => {
                        self.read_descriptor_value(peripheral_uuid, service_uuid, characteristic_uuid, descriptor_uuid, future)
                    }
                    CoreBluetoothMessage::WriteDescriptorValue{
                        peripheral_uuid,service_uuid,
                        characteristic_uuid,
                        descriptor_uuid,
                        data,
                        future,
                    } => self.write_descriptor_value(peripheral_uuid, service_uuid, characteristic_uuid, descriptor_uuid, data, future),
                };
            }
        }
    }

    fn get_adapter_state(&mut self, fut: CoreBluetoothReplyStateShared) {
        let state = unsafe { self.manager.state() };
        fut.lock()
            .unwrap()
            .set_reply(CoreBluetoothReply::AdapterState(state))
    }

    fn start_discovery(&mut self, filter: ScanFilter) {
        trace!("BluetoothAdapter::start_discovery");
        let service_uuids = scan_filter_to_service_uuids(filter);
        let mut options = NSMutableDictionary::new();
        // NOTE: If duplicates are not allowed then a peripheral will not show
        // up again once connected and then disconnected.
        options.insert_id(
            unsafe { CBCentralManagerScanOptionAllowDuplicatesKey },
            Retained::into_super(Retained::into_super(Retained::into_super(
                NSNumber::new_bool(true),
            ))),
        );
        unsafe {
            self.manager
                .scanForPeripheralsWithServices_options(service_uuids.as_deref(), Some(&options))
        };
    }

    fn stop_discovery(&mut self) {
        trace!("BluetoothAdapter::stop_discovery");
        unsafe { self.manager.stopScan() };
    }
}

/// Convert a `ScanFilter` to the appropriate `NSArray<CBUUID *> *` to use for discovery. If the
/// filter has an empty list of services then this will return `nil`, to discover all devices.
fn scan_filter_to_service_uuids(filter: ScanFilter) -> Option<Retained<NSArray<CBUUID>>> {
    if filter.services.is_empty() {
        None
    } else {
        let service_uuids = filter
            .services
            .into_iter()
            .map(uuid_to_cbuuid)
            .collect::<Vec<_>>();
        Some(NSArray::from_vec(service_uuids))
    }
}

impl Drop for CoreBluetoothInternal {
    fn drop(&mut self) {
        trace!("BluetoothAdapter::drop");
        // NOTE: stop discovery only here instead of in BluetoothDiscoverySession
        self.stop_discovery();
    }
}

pub fn run_corebluetooth_thread(
    event_sender: Sender<CoreBluetoothEvent>,
) -> Result<Sender<CoreBluetoothMessage>, Error> {
    let authorization = unsafe { CBManager::authorization_class() };
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
