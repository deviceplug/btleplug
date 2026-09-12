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
    peripheral::Peripheral,
    utils::{
        core_bluetooth::{cbuuid_to_uuid, uuid_to_cbuuid},
        nsuuid_to_uuid,
    },
};
use crate::Error;
use crate::api::{
    CharPropFlags, Characteristic, Descriptor, RetrievePeripheralsOptions, ScanFilter, Service,
    WriteType,
};
use futures::channel::mpsc::{self, Receiver, Sender};
use futures::select;
use futures::sink::SinkExt;
use futures::stream::{Fuse, StreamExt};
use log::{debug, error, trace, warn};
use objc2::{AnyThread, msg_send};
use objc2::{rc::Retained, runtime::AnyObject};
use objc2_core_bluetooth::{
    CBCentralManager, CBCentralManagerScanOptionAllowDuplicatesKey, CBCharacteristic,
    CBCharacteristicProperties, CBCharacteristicWriteType, CBDescriptor, CBManager,
    CBManagerAuthorization, CBManagerState, CBPeripheral, CBPeripheralState, CBService, CBUUID,
};
use objc2_foundation::{NSArray, NSData, NSMutableDictionary, NSNumber, NSString, NSUUID};
use std::{
    collections::{BTreeSet, HashMap, VecDeque},
    ffi::CString,
    fmt::{self, Debug, Formatter},
    ops::Deref,
    thread,
};
use tokio::runtime;
use uuid::Uuid;

/// ATT Write Command PDUs reserve one byte for the opcode and two bytes for
/// the attribute handle (Bluetooth Core Specification, Vol 3, Part F, 3.4.5.3).
const ATT_WRITE_COMMAND_HEADER_LEN: usize = 3;

fn maximum_write_value_length_to_att_mtu(maximum_write_value_length: usize) -> Result<u16, String> {
    if maximum_write_value_length == 0 {
        return Ok(crate::api::DEFAULT_MTU_SIZE);
    }

    maximum_write_value_length
        .checked_add(ATT_WRITE_COMMAND_HEADER_LEN)
        .and_then(|mtu| u16::try_from(mtu).ok())
        .ok_or_else(|| {
            format!(
                "CoreBluetooth maximum write value length {maximum_write_value_length} cannot be represented as a u16 ATT MTU"
            )
        })
}

struct DescriptorInternal {
    pub descriptor: Retained<CBDescriptor>,
    pub uuid: Uuid,
    pub read_future_state: VecDeque<CoreBluetoothReplyStateShared>,
    pub write_future_state: VecDeque<CoreBluetoothReplyStateShared>,
}

impl DescriptorInternal {
    pub fn new(descriptor: Retained<CBDescriptor>) -> Self {
        let raw_uuid = unsafe { descriptor.UUID() };
        let uuid = cbuuid_to_uuid(&raw_uuid);
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
    // Invariant: the head is the sole notification request submitted to
    // CoreBluetooth; the tail has not been sent yet. Callbacks do not
    // identify the requested direction, so a notification-state callback
    // always completes the head request, and the next request is only
    // submitted once the head has been consumed.
    pub notification_requests: VecDeque<PendingNotificationRequest>,
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
            .field("notification_requests", &self.notification_requests)
            .finish()
    }
}

impl CharacteristicInternal {
    pub fn new(characteristic: Retained<CBCharacteristic>) -> Self {
        let properties = CharacteristicInternal::form_flags(&characteristic);
        let raw_uuid = unsafe { characteristic.UUID() };
        let uuid = cbuuid_to_uuid(&raw_uuid);
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
            notification_requests: VecDeque::with_capacity(4),
            discovered: false,
        }
    }

    fn form_flags(characteristic: &CBCharacteristic) -> CharPropFlags {
        let flags = unsafe { characteristic.properties() };
        let mut v = CharPropFlags::default();
        if flags.contains(CBCharacteristicProperties::Broadcast) {
            v |= CharPropFlags::BROADCAST;
        }
        if flags.contains(CBCharacteristicProperties::Read) {
            v |= CharPropFlags::READ;
        }
        if flags.contains(CBCharacteristicProperties::WriteWithoutResponse) {
            v |= CharPropFlags::WRITE_WITHOUT_RESPONSE;
        }
        if flags.contains(CBCharacteristicProperties::Write) {
            v |= CharPropFlags::WRITE;
        }
        if flags.contains(CBCharacteristicProperties::Notify) {
            v |= CharPropFlags::NOTIFY;
        }
        if flags.contains(CBCharacteristicProperties::Indicate) {
            v |= CharPropFlags::INDICATE;
        }
        if flags.contains(CBCharacteristicProperties::AuthenticatedSignedWrites) {
            v |= CharPropFlags::AUTHENTICATED_SIGNED_WRITES;
        }
        trace!("Flags: {:?}", v);
        v
    }
}

struct PendingWriteWithoutResponse {
    service_uuid: Uuid,
    characteristic_uuid: Uuid,
    data: Vec<u8>,
    fut: CoreBluetoothReplyStateShared,
}

#[derive(Debug)]
struct PendingNotificationRequest {
    enabled: bool,
    future: CoreBluetoothReplyStateShared,
}

#[derive(Clone, Debug)]
pub enum CoreBluetoothReply {
    AdapterState(CBManagerState),
    ReadResult(Vec<u8>),
    ReadRssi(i16),
    Connected,
    ServicesDiscovered(BTreeSet<Service>, u16),
    State(CBPeripheralState),
    Ok,
    Peripherals(Vec<Peripheral>),
    Err(String),
}

#[derive(Debug)]
pub enum PeripheralEventInternal {
    Disconnected,
    Notification(Uuid, Uuid, Vec<u8>),
    ManufacturerData(u16, Vec<u8>, i16),
    ServiceData(HashMap<Uuid, Vec<u8>>, i16),
    Services(Vec<Uuid>, i16),
    ServicesModified,
    TxPowerLevel(i16),
    RssiRead(i16),
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
    pub services_discovered_future_state: Option<CoreBluetoothReplyStateShared>,
    pub read_rssi_future_state: VecDeque<CoreBluetoothReplyStateShared>,
    pub write_without_response_queue: VecDeque<PendingWriteWithoutResponse>,
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
            .field(
                "services_discovered_future_state",
                &self.services_discovered_future_state,
            )
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
            services_discovered_future_state: None,
            read_rssi_future_state: VecDeque::with_capacity(4),
            write_without_response_queue: VecDeque::new(),
        }
    }

    pub fn set_characteristics(
        &mut self,
        service_uuid: Uuid,
        characteristics: HashMap<Uuid, Retained<CBCharacteristic>>,
    ) {
        let service = self
            .services
            .get_mut(&service_uuid)
            .expect("Got characteristics for a service we don't know about");
        for (characteristic_uuid, cb_characteristic) in characteristics {
            if let Some(existing) = service.characteristics.get_mut(&characteristic_uuid) {
                // Update the CB object reference and properties, but preserve
                // in-flight future state and already-discovered descriptors to
                // avoid dropping pending operations during late re-discovery
                // events (see issue #167).
                existing.properties = CharacteristicInternal::form_flags(&cb_characteristic);
                existing.characteristic = cb_characteristic;
            } else {
                service.characteristics.insert(
                    characteristic_uuid,
                    CharacteristicInternal::new(cb_characteristic),
                );
            }
        }
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
    ) -> bool {
        let Some(service) = self.services.get_mut(&service_uuid) else {
            return false;
        };
        let Some(characteristic) = service.characteristics.get_mut(&characteristic_uuid) else {
            return false;
        };
        for (descriptor_uuid, cb_descriptor) in descriptors {
            if let Some(existing) = characteristic.descriptors.get_mut(&descriptor_uuid) {
                // Update the CB object reference but preserve in-flight future
                // state to avoid dropping pending operations during late
                // re-discovery events (see issue #167).
                existing.descriptor = cb_descriptor;
            } else {
                characteristic
                    .descriptors
                    .insert(descriptor_uuid, DescriptorInternal::new(cb_descriptor));
            }
        }
        characteristic.discovered = true;

        if !service
            .characteristics
            .values()
            .any(|characteristic| !characteristic.discovered)
        {
            service.discovered = true;
            self.check_discovered()
        }
        true
    }

    fn check_discovered(&mut self) {
        // It's time for QUESTIONABLE ASSUMPTIONS.
        //
        // For sake of being lazy, we don't want to fire device connection until
        // we have all of our services and characteristics. We assume that
        // set_characteristics should be called once for every entry in the
        // service map. Once that's done, we're filled out enough and can send
        // back a ServicesDiscovered reply to the waiting future with all of
        // the characteristic info in it.
        if !self.services.values().any(|service| !service.discovered) {
            if self.services_discovered_future_state.is_none() {
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
            // CoreBluetooth exposes the maximum characteristic value length for
            // a write, not the ATT MTU. Sample it after discovery, then account
            // for the ATT Write Command header to infer the full ATT MTU.
            let maximum_write_value_length = unsafe {
                self.peripheral
                    .maximumWriteValueLengthForType(CBCharacteristicWriteType::WithoutResponse)
            };
            let reply = match maximum_write_value_length_to_att_mtu(maximum_write_value_length) {
                Ok(mtu) => CoreBluetoothReply::ServicesDiscovered(services, mtu),
                Err(error) => CoreBluetoothReply::Err(error),
            };
            self.services_discovered_future_state
                .take()
                .unwrap()
                .lock()
                .unwrap()
                .set_reply(reply);
        }
    }

    pub fn confirm_disconnect(&mut self) {
        self.drain_pending_operations("Device disconnected");
    }

    /// Queue a subscribe or unsubscribe request for one characteristic.
    ///
    /// Requests are processed in queue order: only the head is ever
    /// submitted to CoreBluetooth, and the next request is submitted when
    /// the head is completed by its notification-state callback. This keeps
    /// every callback result attached to exactly one request.
    pub fn queue_notification_request(
        &mut self,
        service_uuid: Uuid,
        characteristic_uuid: Uuid,
        enabled: bool,
        fut: CoreBluetoothReplyStateShared,
    ) {
        let Some(service) = self.services.get_mut(&service_uuid) else {
            complete_missing(fut, "Service");
            return;
        };
        let Some(characteristic) = service.characteristics.get_mut(&characteristic_uuid) else {
            complete_missing(fut, "Characteristic");
            return;
        };
        // Enqueue before submitting so the request is owned by the queue
        // before CoreBluetooth can report on it.
        let was_empty = characteristic.notification_requests.is_empty();
        characteristic
            .notification_requests
            .push_back(PendingNotificationRequest {
                enabled,
                future: fut,
            });
        if was_empty {
            Self::submit_notification_request(&self.peripheral, characteristic);
        }
    }

    /// Complete the head notification request with a notification-state
    /// callback result, then submit the next queued request.
    ///
    /// Events for characteristics without a pending request are ignored:
    /// the callback identifies no request, so there is nothing to attach it
    /// to.
    pub fn on_notification_state_updated(
        &mut self,
        service_uuid: Uuid,
        characteristic_uuid: Uuid,
        error: Option<String>,
    ) {
        let Some(service) = self.services.get_mut(&service_uuid) else {
            return;
        };
        let Some(characteristic) = service.characteristics.get_mut(&characteristic_uuid) else {
            return;
        };
        let Some(request) = characteristic.notification_requests.pop_front() else {
            return;
        };
        request.future.lock().unwrap().set_reply(match error {
            Some(error) => CoreBluetoothReply::Err(error),
            None => CoreBluetoothReply::Ok,
        });
        Self::submit_notification_request(&self.peripheral, characteristic);
    }

    fn submit_notification_request(
        peripheral: &CBPeripheral,
        characteristic: &mut CharacteristicInternal,
    ) {
        if let Some(request) = characteristic.notification_requests.front() {
            unsafe {
                peripheral.setNotifyValue_forCharacteristic(
                    request.enabled,
                    &characteristic.characteristic,
                );
            }
        }
    }

    /// Complete every operation that cannot receive a callback after the
    /// peripheral disappears.  Keep this centralized: adding a future-bearing
    /// operation must also add its queue here.
    fn drain_pending_operations(&mut self, message: &str) {
        let error = CoreBluetoothReply::Err(message.to_string());
        for future in [
            self.disconnected_future_state.take(),
            self.connected_future_state.take(),
            self.services_discovered_future_state.take(),
        ]
        .into_iter()
        .flatten()
        {
            future.lock().unwrap().set_reply(error.clone());
        }
        for state in self.read_rssi_future_state.drain(..) {
            state.lock().unwrap().set_reply(error.clone());
        }
        for pending in self.write_without_response_queue.drain(..) {
            pending.fut.lock().unwrap().set_reply(error.clone());
        }
        for service in self.services.values_mut() {
            for characteristic in service.characteristics.values_mut() {
                for queue in [
                    &mut characteristic.read_future_state,
                    &mut characteristic.write_future_state,
                ] {
                    for state in queue.drain(..) {
                        state.lock().unwrap().set_reply(error.clone());
                    }
                }
                // Drain the submitted head along with the unsent tail. No
                // further request is submitted afterwards.
                for request in characteristic.notification_requests.drain(..) {
                    request.future.lock().unwrap().set_reply(error.clone());
                }
                for descriptor in characteristic.descriptors.values_mut() {
                    for queue in [
                        &mut descriptor.read_future_state,
                        &mut descriptor.write_future_state,
                    ] {
                        for state in queue.drain(..) {
                            state.lock().unwrap().set_reply(error.clone());
                        }
                    }
                }
            }
        }
    }
}

fn complete_missing(fut: CoreBluetoothReplyStateShared, object: &str) {
    fut.lock()
        .unwrap()
        .set_reply(CoreBluetoothReply::Err(format!(
            "{object} no longer available"
        )));
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
    DiscoverServices {
        peripheral_uuid: Uuid,
        future: CoreBluetoothReplyStateShared,
    },
    ReadRssi {
        peripheral_uuid: Uuid,
        future: CoreBluetoothReplyStateShared,
    },
    RetrievePeripherals {
        options: RetrievePeripheralsOptions,
        future: CoreBluetoothReplyStateShared,
    },
    ClearPeripherals {
        future: CoreBluetoothReplyStateShared,
    },
}

#[derive(Debug)]
pub struct RetrievedPeripheral {
    pub uuid: Uuid,
    pub local_name: Option<String>,
    pub advertisement_name: Option<String>,
    pub event_receiver: Option<Receiver<PeripheralEventInternal>>,
}

#[derive(Debug)]
pub enum CoreBluetoothEvent {
    DidUpdateState {
        state: CBManagerState,
    },
    DeviceDiscovered {
        uuid: Uuid,
        local_name: Option<String>,
        advertisement_name: Option<String>,
        event_receiver: Receiver<PeripheralEventInternal>,
    },
    RetrievedPeripherals {
        peripherals: Vec<RetrievedPeripheral>,
        future: CoreBluetoothReplyStateShared,
    },
    DeviceUpdated {
        uuid: Uuid,
        local_name: Option<String>,
        advertisement_name: Option<String>,
    },
    DeviceDisconnected {
        uuid: Uuid,
    },
    PeripheralsCleared {
        future: CoreBluetoothReplyStateShared,
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
            msg_send![CBCentralManager::alloc(), initWithDelegate: &*delegate, queue: queue]
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
            manufacturer_id, manufacturer_data
        );
        let dead = if let Some(p) = self.peripherals.get_mut(&peripheral_uuid) {
            p.event_sender
                .send(PeripheralEventInternal::ManufacturerData(
                    manufacturer_id,
                    manufacturer_data,
                    rssi,
                ))
                .await
                .is_err()
        } else {
            false
        };
        if dead {
            error!("Removing CoreBluetooth peripheral {peripheral_uuid}: event receiver is gone");
            self.peripherals.remove(&peripheral_uuid);
        }
    }

    async fn on_service_data(
        &mut self,
        peripheral_uuid: Uuid,
        service_data: HashMap<Uuid, Vec<u8>>,
        rssi: i16,
    ) {
        trace!("Got service data advertisement! {:?}", service_data);
        let dead = if let Some(p) = self.peripherals.get_mut(&peripheral_uuid) {
            p.event_sender
                .send(PeripheralEventInternal::ServiceData(service_data, rssi))
                .await
                .is_err()
        } else {
            false
        };
        if dead {
            error!("Removing CoreBluetooth peripheral {peripheral_uuid}: event receiver is gone");
            self.peripherals.remove(&peripheral_uuid);
        }
    }

    async fn on_services(&mut self, peripheral_uuid: Uuid, services: Vec<Uuid>, rssi: i16) {
        trace!("Got service advertisement! {:?}", services);
        let dead = if let Some(p) = self.peripherals.get_mut(&peripheral_uuid) {
            p.event_sender
                .send(PeripheralEventInternal::Services(services, rssi))
                .await
                .is_err()
        } else {
            false
        };
        if dead {
            error!("Removing CoreBluetooth peripheral {peripheral_uuid}: event receiver is gone");
            self.peripherals.remove(&peripheral_uuid);
        }
    }

    async fn on_services_modified(&mut self, peripheral_uuid: Uuid) {
        trace!(
            "Peripheral modified services and must be rediscovered! {:?}",
            peripheral_uuid
        );
        if let Some(p) = self.peripherals.get_mut(&peripheral_uuid) {
            p.services.clear();
            if let Err(e) = p
                .event_sender
                .send(PeripheralEventInternal::ServicesModified)
                .await
            {
                error!("Error sending notification event: {}", e);
            }
        }
    }

    async fn on_discovered_peripheral(
        &mut self,
        peripheral: Retained<CBPeripheral>,
        advertisement_name: Option<String>,
    ) {
        let id = unsafe { peripheral.identifier() };
        let uuid = nsuuid_to_uuid(&id);
        let peripheral_name = unsafe { peripheral.name() };
        // Prefer advertisement_name (from scan response, usually COMPLETE_LOCAL_NAME)
        // over peripheral.name() (GAP cache, often the truncated SHORT_LOCAL_NAME)
        let local_name = advertisement_name
            .clone()
            .or_else(|| peripheral_name.map(|n| n.to_string()));

        if let std::collections::hash_map::Entry::Vacant(e) = self.peripherals.entry(uuid) {
            // Create our channels
            let (event_sender, event_receiver) = mpsc::channel(256);
            e.insert(PeripheralInternal::new(peripheral, event_sender));
            self.dispatch_event(CoreBluetoothEvent::DeviceDiscovered {
                uuid,
                local_name,
                advertisement_name,
                event_receiver,
            })
            .await;
        } else {
            if local_name.is_some() || advertisement_name.is_some() {
                self.dispatch_event(CoreBluetoothEvent::DeviceUpdated {
                    uuid,
                    local_name,
                    advertisement_name,
                })
                .await;
            }
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
            peripheral_uuid, service_uuid
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
            peripheral_uuid, service_uuid, characteristic_uuid,
        );
        for id in descriptors.keys() {
            trace!("{}", id);
        }
        if let Some(p) = self.peripherals.get_mut(&peripheral_uuid)
            && !p.set_characteristic_descriptors(service_uuid, characteristic_uuid, descriptors)
            && let Some(future) = p.services_discovered_future_state.take()
        {
            future.lock().unwrap().set_reply(CoreBluetoothReply::Err(
                        format!("Unknown descriptor relationship for service {service_uuid}, characteristic {characteristic_uuid}"),
                    ));
        }
    }

    fn on_peripheral_connect(&mut self, peripheral_uuid: Uuid) {
        if self.peripherals.contains_key(&peripheral_uuid) {
            let peripheral = self
                .peripherals
                .get_mut(&peripheral_uuid)
                .expect("If we're here we should have an ID");
            if let Some(future) = peripheral.connected_future_state.take() {
                future
                    .lock()
                    .unwrap()
                    .set_reply(CoreBluetoothReply::Connected);
            } else {
                debug!(
                    "Ignoring duplicate connection callback for peripheral {}",
                    peripheral_uuid
                );
            }
        }
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
            if let Some(future) = peripheral.connected_future_state.take() {
                future
                    .lock()
                    .unwrap()
                    .set_reply(CoreBluetoothReply::Err(error));
            } else {
                debug!(
                    "Ignoring duplicate connection failure callback for peripheral {}",
                    peripheral_uuid
                );
            }
        }
    }

    async fn on_adapter_powered_off(&mut self) {
        warn!("Adapter powered off, canceling all pending operations");
        let peripheral_uuids: Vec<Uuid> = self.peripherals.keys().cloned().collect();
        for uuid in peripheral_uuids {
            if let Err(e) = self
                .peripherals
                .get_mut(&uuid)
                .unwrap()
                .event_sender
                .send(PeripheralEventInternal::Disconnected)
                .await
            {
                error!("Error sending disconnect event for {}: {}", uuid, e);
            }
            self.peripherals
                .get_mut(&uuid)
                .unwrap()
                .confirm_disconnect();
            self.dispatch_event(CoreBluetoothEvent::DeviceDisconnected { uuid })
                .await;
        }
        self.peripherals.clear();
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

    fn on_characteristic_notification_state_updated(
        &mut self,
        peripheral_uuid: Uuid,
        service_uuid: Uuid,
        characteristic_uuid: Uuid,
        error: Option<String>,
    ) {
        if let Some(peripheral) = self.peripherals.get_mut(&peripheral_uuid) {
            peripheral.on_notification_state_updated(service_uuid, characteristic_uuid, error);
        }
    }

    async fn on_characteristic_read(
        &mut self,
        peripheral_uuid: Uuid,
        service_uuid: Uuid,
        characteristic_uuid: Uuid,
        data: Vec<u8>,
        error: Option<String>,
    ) {
        if let Some(peripheral) = self.peripherals.get_mut(&peripheral_uuid)
            && let Some(service) = peripheral.services.get_mut(&service_uuid)
            && let Some(characteristic) = service.characteristics.get_mut(&characteristic_uuid)
        {
            trace!("Got read event!");
            if let Some(error) = error {
                if let Some(state) = characteristic.read_future_state.pop_front() {
                    state
                        .lock()
                        .unwrap()
                        .set_reply(CoreBluetoothReply::Err(error));
                }
                return;
            }

            let mut data_clone = Vec::new();
            for byte in data.iter() {
                data_clone.push(*byte);
            }
            // Reads and notifications both return the same callback. If
            // we're trying to do a read, we'll have a future we can
            // fulfill. Otherwise, just treat the returned value as a
            // notification and use the event system.
            if !characteristic.read_future_state.is_empty() {
                let state = characteristic.read_future_state.pop_front().unwrap();
                state
                    .lock()
                    .unwrap()
                    .set_reply(CoreBluetoothReply::ReadResult(data_clone));
            } else if let Err(e) = peripheral
                .event_sender
                .send(PeripheralEventInternal::Notification(
                    characteristic_uuid,
                    service_uuid,
                    data,
                ))
                .await
            {
                error!("Error sending notification event: {}", e);
            }
        }
    }

    fn on_characteristic_written(
        &mut self,
        peripheral_uuid: Uuid,
        service_uuid: Uuid,
        characteristic_uuid: Uuid,
        error: Option<String>,
    ) {
        if let Some(characteristic) =
            self.get_characteristic(peripheral_uuid, service_uuid, characteristic_uuid)
        {
            trace!("Got written event!");
            if let Some(state) = characteristic.write_future_state.pop_front() {
                state.lock().unwrap().set_reply(match error {
                    Some(error) => CoreBluetoothReply::Err(error),
                    None => CoreBluetoothReply::Ok,
                });
            }
        }
    }

    fn connect_peripheral(&mut self, peripheral_uuid: Uuid, fut: CoreBluetoothReplyStateShared) {
        trace!("Trying to connect peripheral!");
        if let Some(p) = self.peripherals.get_mut(&peripheral_uuid) {
            trace!("Connecting peripheral!");
            p.connected_future_state = Some(fut);
            unsafe { self.manager.connectPeripheral_options(&p.peripheral, None) };
        } else {
            fut.lock().unwrap().set_reply(CoreBluetoothReply::Err(
                "Peripheral no longer available".into(),
            ));
        }
    }

    fn disconnect_peripheral(&mut self, peripheral_uuid: Uuid, fut: CoreBluetoothReplyStateShared) {
        trace!("Trying to disconnect peripheral!");
        if let Some(p) = self.peripherals.get_mut(&peripheral_uuid) {
            trace!("Disconnecting peripheral!");
            p.disconnected_future_state = Some(fut);
            unsafe { self.manager.cancelPeripheralConnection(&p.peripheral) };
        } else {
            fut.lock().unwrap().set_reply(CoreBluetoothReply::Ok);
        }
    }

    fn is_connected(&mut self, peripheral_uuid: Uuid, fut: CoreBluetoothReplyStateShared) {
        if let Some(p) = self.peripherals.get_mut(&peripheral_uuid) {
            let state = unsafe { p.peripheral.state() };
            trace!("Connected state {:?} ", state);
            fut.lock()
                .unwrap()
                .set_reply(CoreBluetoothReply::State(state));
        } else {
            // Peripheral was removed after disconnect — report as disconnected
            // rather than hanging the future forever.
            fut.lock()
                .unwrap()
                .set_reply(CoreBluetoothReply::State(CBPeripheralState::Disconnected));
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
        let Some(peripheral) = self.peripherals.get_mut(&peripheral_uuid) else {
            complete_missing(fut, "Peripheral");
            return;
        };
        let Some(service) = peripheral.services.get_mut(&service_uuid) else {
            complete_missing(fut, "Service");
            return;
        };
        let Some(characteristic) = service.characteristics.get_mut(&characteristic_uuid) else {
            complete_missing(fut, "Characteristic");
            return;
        };
        {
            trace!("Writing value! With kind {:?}", kind);
            match kind {
                WriteType::WithoutResponse => {
                    if unsafe { peripheral.peripheral.canSendWriteWithoutResponse() } {
                        unsafe {
                            peripheral.peripheral.writeValue_forCharacteristic_type(
                                &NSData::from_vec(data),
                                &characteristic.characteristic,
                                CBCharacteristicWriteType::WithoutResponse,
                            );
                        }
                        fut.lock().unwrap().set_reply(CoreBluetoothReply::Ok);
                    } else {
                        trace!("Queueing write-without-response (peripheral not ready)");
                        peripheral.write_without_response_queue.push_back(
                            PendingWriteWithoutResponse {
                                service_uuid,
                                characteristic_uuid,
                                data,
                                fut,
                            },
                        );
                    }
                }
                WriteType::WithResponse => {
                    unsafe {
                        peripheral.peripheral.writeValue_forCharacteristic_type(
                            &NSData::from_vec(data),
                            &characteristic.characteristic,
                            CBCharacteristicWriteType::WithResponse,
                        );
                    }
                    characteristic.write_future_state.push_back(fut);
                }
            }
        }
    }

    fn drain_write_without_response_queue(&mut self, peripheral_uuid: Uuid) {
        if let Some(peripheral) = self.peripherals.get_mut(&peripheral_uuid) {
            while let Some(pending) = peripheral.write_without_response_queue.pop_front() {
                if !unsafe { peripheral.peripheral.canSendWriteWithoutResponse() } {
                    peripheral.write_without_response_queue.push_front(pending);
                    break;
                }
                if let Some(service) = peripheral.services.get(&pending.service_uuid) {
                    if let Some(characteristic) =
                        service.characteristics.get(&pending.characteristic_uuid)
                    {
                        unsafe {
                            peripheral.peripheral.writeValue_forCharacteristic_type(
                                &NSData::from_vec(pending.data),
                                &characteristic.characteristic,
                                CBCharacteristicWriteType::WithoutResponse,
                            );
                        }
                        pending
                            .fut
                            .lock()
                            .unwrap()
                            .set_reply(CoreBluetoothReply::Ok);
                    } else {
                        pending
                            .fut
                            .lock()
                            .unwrap()
                            .set_reply(CoreBluetoothReply::Err(
                                "Characteristic no longer available".into(),
                            ));
                    }
                } else {
                    pending
                        .fut
                        .lock()
                        .unwrap()
                        .set_reply(CoreBluetoothReply::Err(
                            "Service no longer available".into(),
                        ));
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
        let Some(peripheral) = self.peripherals.get_mut(&peripheral_uuid) else {
            complete_missing(fut, "Peripheral");
            return;
        };
        let Some(service) = peripheral.services.get_mut(&service_uuid) else {
            complete_missing(fut, "Service");
            return;
        };
        let Some(characteristic) = service.characteristics.get_mut(&characteristic_uuid) else {
            complete_missing(fut, "Characteristic");
            return;
        };
        {
            trace!("Reading value!");
            unsafe {
                peripheral
                    .peripheral
                    .readValueForCharacteristic(&characteristic.characteristic);
            }
            characteristic.read_future_state.push_back(fut);
        }
    }

    fn subscribe(
        &mut self,
        peripheral_uuid: Uuid,
        service_uuid: Uuid,
        characteristic_uuid: Uuid,
        fut: CoreBluetoothReplyStateShared,
    ) {
        let Some(peripheral) = self.peripherals.get_mut(&peripheral_uuid) else {
            complete_missing(fut, "Peripheral");
            return;
        };
        peripheral.queue_notification_request(service_uuid, characteristic_uuid, true, fut);
    }

    fn unsubscribe(
        &mut self,
        peripheral_uuid: Uuid,
        service_uuid: Uuid,
        characteristic_uuid: Uuid,
        fut: CoreBluetoothReplyStateShared,
    ) {
        let Some(peripheral) = self.peripherals.get_mut(&peripheral_uuid) else {
            complete_missing(fut, "Peripheral");
            return;
        };
        peripheral.queue_notification_request(service_uuid, characteristic_uuid, false, fut);
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
        let Some(peripheral) = self.peripherals.get_mut(&peripheral_uuid) else {
            complete_missing(fut, "Peripheral");
            return;
        };
        let Some(service) = peripheral.services.get_mut(&service_uuid) else {
            complete_missing(fut, "Service");
            return;
        };
        let Some(characteristic) = service.characteristics.get_mut(&characteristic_uuid) else {
            complete_missing(fut, "Characteristic");
            return;
        };
        let Some(descriptor) = characteristic.descriptors.get_mut(&descriptor_uuid) else {
            complete_missing(fut, "Descriptor");
            return;
        };
        {
            trace!("Writing descriptor value!");
            unsafe {
                peripheral
                    .peripheral
                    .writeValue_forDescriptor(&NSData::from_vec(data), &descriptor.descriptor);
            }
            descriptor.write_future_state.push_back(fut);
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
        let Some(peripheral) = self.peripherals.get_mut(&peripheral_uuid) else {
            complete_missing(fut, "Peripheral");
            return;
        };
        let Some(service) = peripheral.services.get_mut(&service_uuid) else {
            complete_missing(fut, "Service");
            return;
        };
        let Some(characteristic) = service.characteristics.get_mut(&characteristic_uuid) else {
            complete_missing(fut, "Characteristic");
            return;
        };
        let Some(descriptor) = characteristic.descriptors.get_mut(&descriptor_uuid) else {
            complete_missing(fut, "Descriptor");
            return;
        };
        {
            trace!("Reading descriptor value!");
            unsafe {
                peripheral
                    .peripheral
                    .readValueForDescriptor(&descriptor.descriptor);
            }
            descriptor.read_future_state.push_back(fut);
        }
    }

    fn read_rssi(&mut self, peripheral_uuid: Uuid, fut: CoreBluetoothReplyStateShared) {
        let Some(peripheral) = self.peripherals.get_mut(&peripheral_uuid) else {
            complete_missing(fut, "Peripheral");
            return;
        };
        {
            trace!("Reading RSSI!");
            unsafe {
                peripheral.peripheral.readRSSI();
            }
            peripheral.read_rssi_future_state.push_back(fut);
        }
    }

    async fn on_read_rssi(&mut self, peripheral_uuid: Uuid, rssi: i16, error: Option<String>) {
        if let Some(peripheral) = self.peripherals.get_mut(&peripheral_uuid) {
            trace!("Got RSSI read event: {}", rssi);
            if let Some(state) = peripheral.read_rssi_future_state.pop_front() {
                state.lock().unwrap().set_reply(match error {
                    Some(error) => CoreBluetoothReply::Err(error),
                    None => CoreBluetoothReply::ReadRssi(rssi),
                });
            }
            // Also send as a peripheral event for CentralEvent emission
            if let Err(e) = peripheral
                .event_sender
                .send(PeripheralEventInternal::RssiRead(rssi))
                .await
            {
                error!("Error sending RSSI event: {}", e);
            }
        }
    }

    async fn on_tx_power_level(&mut self, peripheral_uuid: Uuid, tx_power_level: i16) {
        if let Some(peripheral) = self.peripherals.get_mut(&peripheral_uuid)
            && let Err(e) = peripheral
                .event_sender
                .send(PeripheralEventInternal::TxPowerLevel(tx_power_level))
                .await
        {
            error!("Error sending tx_power_level event: {}", e);
        }
    }

    async fn on_descriptor_read(
        &mut self,
        peripheral_uuid: Uuid,
        service_uuid: Uuid,
        characteristic_uuid: Uuid,
        descriptor_uuid: Uuid,
        data: Vec<u8>,
        error: Option<String>,
    ) {
        if let Some(peripheral) = self.peripherals.get_mut(&peripheral_uuid)
            && let Some(service) = peripheral.services.get_mut(&service_uuid)
            && let Some(characteristic) = service.characteristics.get_mut(&characteristic_uuid)
            && let Some(descriptor) = characteristic.descriptors.get_mut(&descriptor_uuid)
        {
            trace!("Got read event!");
            if let Some(error) = error {
                if let Some(state) = descriptor.read_future_state.pop_front() {
                    state
                        .lock()
                        .unwrap()
                        .set_reply(CoreBluetoothReply::Err(error));
                }
                return;
            }

            let mut data_clone = Vec::new();
            for byte in data.iter() {
                data_clone.push(*byte);
            }
            if let Some(state) = descriptor.read_future_state.pop_front() {
                state
                    .lock()
                    .unwrap()
                    .set_reply(CoreBluetoothReply::ReadResult(data_clone));
            }
        }
    }

    fn on_descriptor_written(
        &mut self,
        peripheral_uuid: Uuid,
        service_uuid: Uuid,
        characteristic_uuid: Uuid,
        descriptor_uuid: Uuid,
        error: Option<String>,
    ) {
        if let Some(descriptor) = self.get_descriptor(
            peripheral_uuid,
            service_uuid,
            characteristic_uuid,
            descriptor_uuid,
        ) {
            trace!("Got written event!");
            if let Some(state) = descriptor.write_future_state.pop_front() {
                state.lock().unwrap().set_reply(match error {
                    Some(error) => CoreBluetoothReply::Err(error),
                    None => CoreBluetoothReply::Ok,
                });
            }
        }
    }

    fn discover_services(&mut self, peripheral_uuid: Uuid, fut: CoreBluetoothReplyStateShared) {
        if let Some(p) = self.peripherals.get_mut(&peripheral_uuid) {
            trace!("Discovering services!");
            p.services_discovered_future_state = Some(fut);
            // This will trigger the delegate_peripheral_diddiscoverservices in central_delegate.rs
            unsafe { p.peripheral.discoverServices(None) };
        }
    }

    async fn retrieve_peripherals(
        &mut self,
        options: RetrievePeripheralsOptions,
        future: CoreBluetoothReplyStateShared,
    ) {
        if options.identifiers.is_none() && options.services.is_none() {
            future.lock().unwrap().set_reply(CoreBluetoothReply::Err(
                "retrieve_peripherals requires an identifier or service selector".to_string(),
            ));
            return;
        }
        let mut retrieved = Vec::new();
        if let Some(services) = options.services.filter(|services| !services.is_empty()) {
            let services = NSArray::from_retained_slice(
                &services.into_iter().map(uuid_to_cbuuid).collect::<Vec<_>>(),
            );
            retrieved.extend(unsafe {
                self.manager
                    .retrieveConnectedPeripheralsWithServices(&services)
            });
        }
        if let Some(identifiers) = options
            .identifiers
            .filter(|identifiers| !identifiers.is_empty())
        {
            let identifiers = NSArray::from_retained_slice(
                &identifiers
                    .into_iter()
                    .map(|id| {
                        NSUUID::from_string(&objc2_foundation::NSString::from_str(&id.to_string()))
                            .unwrap()
                    })
                    .collect::<Vec<_>>(),
            );
            retrieved.extend(unsafe {
                self.manager
                    .retrievePeripheralsWithIdentifiers(&identifiers)
            });
        }
        let mut peripherals = Vec::new();
        for peripheral in retrieved {
            let identifier = unsafe { peripheral.identifier() };
            let uuid = nsuuid_to_uuid(&identifier);
            if peripherals
                .iter()
                .any(|retrieved: &RetrievedPeripheral| retrieved.uuid == uuid)
            {
                continue;
            }

            let peripheral_name = unsafe { peripheral.name() };
            let local_name = peripheral_name.map(|name| name.to_string());
            let event_receiver = if let Some(existing) = self.peripherals.get_mut(&uuid) {
                existing.peripheral = peripheral;
                None
            } else {
                let (event_sender, event_receiver) = mpsc::channel(256);
                self.peripherals
                    .insert(uuid, PeripheralInternal::new(peripheral, event_sender));
                Some(event_receiver)
            };
            peripherals.push(RetrievedPeripheral {
                uuid,
                local_name,
                advertisement_name: None,
                event_receiver,
            });
        }
        self.dispatch_event(CoreBluetoothEvent::RetrievedPeripherals {
            peripherals,
            future,
        })
        .await;
    }

    async fn wait_for_message(&mut self) {
        select! {
            delegate_msg = self.delegate_receiver.select_next_some() => {
                match delegate_msg {
                    // TODO We should probably also register some sort of
                    // "ready" variable in our adapter that will cause scans/etc
                    // to fail if this hasn't updated.
                    CentralDelegateEvent::DidUpdateState{state} => {
                        if state == CBManagerState::PoweredOff {
                            self.on_adapter_powered_off().await;
                        }
                        self.dispatch_event(CoreBluetoothEvent::DidUpdateState{state}).await
                    }
                    CentralDelegateEvent::DiscoveredPeripheral{cbperipheral, advertisement_name} => {
                        self.on_discovered_peripheral(cbperipheral, advertisement_name).await
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
                    CentralDelegateEvent::CharacteristicNotificationStateUpdated{
                        peripheral_uuid,
                        service_uuid,
                        characteristic_uuid,
                        error,
                     } => self.on_characteristic_notification_state_updated(peripheral_uuid, service_uuid, characteristic_uuid, error),
                    CentralDelegateEvent::CharacteristicNotified{
                        peripheral_uuid,
                        service_uuid,
                        characteristic_uuid,
                        data,
                        error,
                     } => self.on_characteristic_read(peripheral_uuid, service_uuid,characteristic_uuid, data, error).await,
                    CentralDelegateEvent::CharacteristicWritten{
                        peripheral_uuid,
                        service_uuid,
                        characteristic_uuid,
                        error,
                    } => self.on_characteristic_written(peripheral_uuid, service_uuid, characteristic_uuid, error),
                    CentralDelegateEvent::ManufacturerData{peripheral_uuid, manufacturer_id, data, rssi} => {
                        self.on_manufacturer_data(peripheral_uuid, manufacturer_id, data, rssi).await
                    },
                    CentralDelegateEvent::ServiceData{peripheral_uuid, service_data, rssi} => {
                        self.on_service_data(peripheral_uuid, service_data, rssi).await
                    },
                    CentralDelegateEvent::Services{peripheral_uuid, service_uuids, rssi} => {
                        self.on_services(peripheral_uuid, service_uuids, rssi).await
                    },
                    CentralDelegateEvent::ServicesModified{peripheral_uuid} => {
                        self.on_services_modified(peripheral_uuid).await
                    },
                    CentralDelegateEvent::DescriptorNotified{
                        peripheral_uuid,
                        service_uuid,
                        characteristic_uuid,
                        descriptor_uuid,
                        data,
                        error,
                     } => self.on_descriptor_read(peripheral_uuid, service_uuid, characteristic_uuid, descriptor_uuid, data, error).await,
                    CentralDelegateEvent::DescriptorWritten{
                        peripheral_uuid,
                        service_uuid,
                        characteristic_uuid,
                        descriptor_uuid,
                        error,
                    } => self.on_descriptor_written(peripheral_uuid, service_uuid, characteristic_uuid, descriptor_uuid, error),
                    CentralDelegateEvent::TxPowerLevel{peripheral_uuid, tx_power_level} => {
                        self.on_tx_power_level(peripheral_uuid, tx_power_level).await
                    },
                    CentralDelegateEvent::DidReadRssi{peripheral_uuid, rssi, error} => {
                        self.on_read_rssi(peripheral_uuid, rssi, error).await
                    },
                    CentralDelegateEvent::ReadyToSendWriteWithoutResponse{peripheral_uuid} => {
                        self.drain_write_without_response_queue(peripheral_uuid)
                    },
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
                    CoreBluetoothMessage::DiscoverServices{peripheral_uuid, future} => {
                        self.discover_services(peripheral_uuid, future);
                    }
                    CoreBluetoothMessage::ReadRssi{peripheral_uuid, future} => {
                        self.read_rssi(peripheral_uuid, future)
                    }
                    CoreBluetoothMessage::RetrievePeripherals { options, future } => {
                        self.retrieve_peripherals(options, future).await
                    }
                    CoreBluetoothMessage::ClearPeripherals { future } => {
                        self.peripherals.clear();
                        self.dispatch_event(CoreBluetoothEvent::PeripheralsCleared { future })
                            .await;
                    }
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
        let options: Retained<NSMutableDictionary<NSString, AnyObject>> =
            NSMutableDictionary::new();
        // NOTE: If duplicates are not allowed then a peripheral will not show
        // up again once connected and then disconnected.
        options.insert(
            unsafe { CBCentralManagerScanOptionAllowDuplicatesKey },
            &*Retained::into_super(Retained::into_super(NSNumber::new_bool(true))),
        );
        unsafe {
            self.manager.scanForPeripheralsWithServices_options(
                service_uuids.as_deref(),
                Some(&*Retained::into_super(options)),
            )
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
        Some(NSArray::from_retained_slice(&service_uuids))
    }
}

impl Drop for CoreBluetoothInternal {
    fn drop(&mut self) {
        trace!("BluetoothAdapter::drop");
        // NOTE: stop discovery only here instead of in BluetoothDiscoverySession
        self.stop_discovery();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;
    use objc2::{DefinedClass, define_class};
    use objc2_core_bluetooth::{
        CBAttributePermissions, CBMutableCharacteristic, CBMutableService, CBPeripheralDelegate,
    };
    use objc2_foundation::{NSError, NSObjectProtocol, NSString, ns_string};
    use std::sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    };
    use std::task::Poll;
    use std::time::Duration;

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct RecordedSetNotify {
        characteristic_uuid: Uuid,
        enabled: bool,
    }

    struct TestPeripheralState {
        identifier: Retained<NSUUID>,
        set_notify_calls: Mutex<Vec<RecordedSetNotify>>,
    }

    define_class!(
        #[unsafe(super(CBPeripheral))]
        #[thread_kind = AnyThread]
        #[ivars = TestPeripheralState]
        struct TestPeripheral;

        unsafe impl NSObjectProtocol for TestPeripheral {}

        impl TestPeripheral {
            #[unsafe(method_id(identifier))]
            fn identifier(&self) -> Retained<NSUUID> {
                self.ivars().identifier.clone()
            }

            #[unsafe(method_id(name))]
            fn name(&self) -> Option<Retained<NSString>> {
                None
            }

            #[unsafe(method(maximumWriteValueLengthForType:))]
            fn maximum_write_value_length_for_type(
                &self,
                _write_type: CBCharacteristicWriteType,
            ) -> usize {
                0
            }

            #[unsafe(method(setNotifyValue:forCharacteristic:))]
            fn set_notify_value_for_characteristic(
                &self,
                enabled: bool,
                characteristic: &CBCharacteristic,
            ) {
                let raw_uuid = unsafe { characteristic.UUID() };
                self.ivars()
                    .set_notify_calls
                    .lock()
                    .unwrap()
                    .push(RecordedSetNotify {
                        characteristic_uuid: cbuuid_to_uuid(&raw_uuid),
                        enabled,
                    });
            }
        }
    );

    impl TestPeripheral {
        fn new(identifier: Retained<NSUUID>) -> Retained<Self> {
            let this = Self::alloc().set_ivars(TestPeripheralState {
                identifier,
                set_notify_calls: Mutex::new(Vec::new()),
            });
            unsafe { msg_send![super(this), init] }
        }

        fn set_notify_calls(&self) -> Vec<RecordedSetNotify> {
            self.ivars().set_notify_calls.lock().unwrap().clone()
        }
    }

    define_class!(
        #[unsafe(super(CBMutableCharacteristic))]
        #[thread_kind = AnyThread]
        #[ivars = AtomicBool]
        struct TestCharacteristic;

        unsafe impl NSObjectProtocol for TestCharacteristic {}

        impl TestCharacteristic {
            #[unsafe(method(isNotifying))]
            fn is_notifying(&self) -> bool {
                self.ivars().load(Ordering::SeqCst)
            }
        }
    );

    impl TestCharacteristic {
        fn new(uuid: &CBUUID, properties: CBCharacteristicProperties) -> Retained<Self> {
            let this = Self::alloc().set_ivars(AtomicBool::new(false));
            unsafe {
                msg_send![super(this),
                    initWithType: uuid,
                    properties: properties,
                    value: None::<&NSData>,
                    permissions: CBAttributePermissions::Readable
                ]
            }
        }

        fn set_notifying(&self, notifying: bool) {
            self.ivars().store(notifying, Ordering::SeqCst);
        }
    }

    const CALLBACK_TIMEOUT: Duration = Duration::from_secs(2);

    struct FixtureCharacteristic {
        uuid: Uuid,
        characteristic: Retained<TestCharacteristic>,
    }

    struct NotificationFixture {
        peripheral_uuid: Uuid,
        service_uuid: Uuid,
        peripheral: Retained<TestPeripheral>,
        internal: PeripheralInternal,
        delegate: Retained<CentralDelegate>,
        delegate_receiver: Receiver<CentralDelegateEvent>,
        characteristics: Vec<FixtureCharacteristic>,
    }

    impl NotificationFixture {
        fn new(characteristic_uuids: &[Uuid]) -> Self {
            let peripheral_uuid = Uuid::from_u128(0x12345678_1234_5678_1234_567812345678);
            let service_uuid = Uuid::from_u128(0x0000180f_0000_1000_8000_00805f9b34fb);
            let peripheral_uuid_string = NSString::from_str(&peripheral_uuid.to_string());
            let peripheral_identifier =
                NSUUID::initWithUUIDString(NSUUID::alloc(), &peripheral_uuid_string)
                    .expect("valid peripheral UUID");
            let peripheral = TestPeripheral::new(peripheral_identifier);
            // Permanently retain the test peripheral so the fixture can drop
            // at any point, including during a panicking assertion, without
            // ever running CBPeripheral's private destruction path.
            std::mem::forget(peripheral.clone());

            let characteristics: Vec<FixtureCharacteristic> = characteristic_uuids
                .iter()
                .map(|&uuid| {
                    let characteristic_cbuuid = uuid_to_cbuuid(uuid);
                    let characteristic = TestCharacteristic::new(
                        &characteristic_cbuuid,
                        CBCharacteristicProperties::Notify | CBCharacteristicProperties::Read,
                    );
                    FixtureCharacteristic {
                        uuid,
                        characteristic,
                    }
                })
                .collect();
            let native_characteristics = characteristics
                .iter()
                .map(|fixture_characteristic| {
                    Retained::into_super(Retained::into_super(
                        fixture_characteristic.characteristic.clone(),
                    ))
                })
                .collect::<Vec<Retained<CBCharacteristic>>>();

            let service_cbuuid = uuid_to_cbuuid(service_uuid);
            let service = unsafe {
                CBMutableService::initWithType_primary(
                    CBMutableService::alloc(),
                    &service_cbuuid,
                    true,
                )
            };
            let attached_characteristics = NSArray::from_retained_slice(&native_characteristics);
            unsafe { service.setCharacteristics(Some(&attached_characteristics)) };

            let (event_sender, _) = mpsc::channel(1);
            let mut internal =
                PeripheralInternal::new(Retained::into_super(peripheral.clone()), event_sender);
            internal.services.insert(
                service_uuid,
                ServiceInternal {
                    cbservice: Retained::into_super(service),
                    characteristics: native_characteristics
                        .into_iter()
                        .zip(characteristic_uuids)
                        .map(|(characteristic, &uuid)| {
                            (uuid, CharacteristicInternal::new(characteristic))
                        })
                        .collect(),
                    discovered: true,
                },
            );

            let (delegate_sender, delegate_receiver) = mpsc::channel(4);
            let delegate = CentralDelegate::new(delegate_sender);

            Self {
                peripheral_uuid,
                service_uuid,
                peripheral,
                internal,
                delegate,
                delegate_receiver,
                characteristics,
            }
        }

        fn characteristic(&self, index: usize) -> &TestCharacteristic {
            &self.characteristics[index].characteristic
        }

        fn set_notify_calls(&self) -> Vec<RecordedSetNotify> {
            self.peripheral.set_notify_calls()
        }
    }

    /// Run the real Objective-C notification-state delegate method, drain the
    /// resulting delegate event from the channel, and dispatch it through the
    /// production handler. Returns the callback's localized error text, if any.
    async fn deliver_notification_state_callback(
        fixture: &mut NotificationFixture,
        characteristic_index: usize,
        error: Option<&NSError>,
        context: &str,
    ) -> Option<String> {
        let peripheral = &fixture.peripheral;
        let characteristic = &fixture.characteristics[characteristic_index].characteristic;
        unsafe {
            fixture
                .delegate
                .peripheral_didUpdateNotificationStateForCharacteristic_error(
                    peripheral,
                    characteristic,
                    error,
                );
        }
        let event = tokio::time::timeout(CALLBACK_TIMEOUT, fixture.delegate_receiver.next())
            .await
            .unwrap_or_else(|_| {
                panic!("{context}: notification state callback did not emit an event")
            })
            .unwrap_or_else(|| panic!("{context}: delegate event channel closed"));
        let CentralDelegateEvent::CharacteristicNotificationStateUpdated {
            peripheral_uuid,
            service_uuid,
            characteristic_uuid,
            error: event_error,
        } = event
        else {
            panic!("{context}: unexpected delegate event: {event:?}");
        };
        assert_eq!(peripheral_uuid, fixture.peripheral_uuid, "{context}");
        assert_eq!(service_uuid, fixture.service_uuid, "{context}");
        assert_eq!(
            characteristic_uuid, fixture.characteristics[characteristic_index].uuid,
            "{context}"
        );
        let expected_error = error.map(|error| error.localizedDescription().to_string());
        assert_eq!(event_error, expected_error, "{context}");
        fixture.internal.on_notification_state_updated(
            service_uuid,
            characteristic_uuid,
            event_error,
        );
        expected_error
    }

    async fn assert_pending(future: &mut CoreBluetoothReplyFuture, context: &str) {
        assert!(
            matches!(futures::poll!(future), Poll::Pending),
            "{context}: future unexpectedly completed"
        );
    }

    fn notification_error(att_error: bool) -> Retained<NSError> {
        if att_error {
            // Error code 15 mirrors the ATT "Insufficient Encryption" refusal
            // from the issue report.
            NSError::new(15, ns_string!("ATT"))
        } else {
            NSError::new(1, ns_string!("BtlePlugCoreBluetoothTests"))
        }
    }

    #[test]
    fn maximum_write_value_length_is_converted_to_att_mtu() {
        assert_eq!(maximum_write_value_length_to_att_mtu(20), Ok(23));
        assert_eq!(maximum_write_value_length_to_att_mtu(512), Ok(515));
        assert_eq!(
            maximum_write_value_length_to_att_mtu(u16::MAX as usize - 3),
            Ok(u16::MAX)
        );
    }

    #[test]
    fn zero_maximum_write_value_length_uses_default_mtu() {
        assert_eq!(
            maximum_write_value_length_to_att_mtu(0),
            Ok(crate::api::DEFAULT_MTU_SIZE)
        );
    }

    #[test]
    fn unrepresentable_maximum_write_value_length_is_rejected() {
        assert!(maximum_write_value_length_to_att_mtu(u16::MAX as usize).is_err());
        assert!(maximum_write_value_length_to_att_mtu(usize::MAX).is_err());
    }

    #[tokio::test]
    async fn descriptor_discovery_error_completes_service_discovery_without_descriptors() {
        let peripheral_uuid = Uuid::from_u128(0x12345678_1234_5678_1234_567812345678);
        let peripheral_uuid_string = NSString::from_str(&peripheral_uuid.to_string());
        let peripheral_identifier =
            NSUUID::initWithUUIDString(NSUUID::alloc(), &peripheral_uuid_string)
                .expect("valid peripheral UUID");
        let peripheral = TestPeripheral::new(peripheral_identifier);
        let service_uuid = Uuid::from_u128(0x0000180f_0000_1000_8000_00805f9b34fb);
        let characteristic_uuid = Uuid::from_u128(0x00002a19_0000_1000_8000_00805f9b34fb);
        let service_cbuuid = uuid_to_cbuuid(service_uuid);
        let characteristic_cbuuid = uuid_to_cbuuid(characteristic_uuid);
        let characteristic = unsafe {
            CBMutableCharacteristic::initWithType_properties_value_permissions(
                CBMutableCharacteristic::alloc(),
                &characteristic_cbuuid,
                CBCharacteristicProperties::Read,
                None,
                CBAttributePermissions::Readable,
            )
        };
        let service = unsafe {
            CBMutableService::initWithType_primary(CBMutableService::alloc(), &service_cbuuid, true)
        };
        let characteristic: Retained<CBCharacteristic> = Retained::into_super(characteristic);
        let characteristics = NSArray::from_retained_slice(&[characteristic.clone()]);
        unsafe { service.setCharacteristics(Some(&characteristics)) };
        let service: Retained<CBService> = Retained::into_super(service);

        let (event_sender, _) = mpsc::channel(1);
        let mut internal =
            PeripheralInternal::new(Retained::into_super(peripheral.clone()), event_sender);
        internal.services.insert(
            service_uuid,
            ServiceInternal {
                cbservice: service,
                characteristics: HashMap::from([(
                    characteristic_uuid,
                    CharacteristicInternal::new(characteristic.clone()),
                )]),
                discovered: false,
            },
        );
        let discovery = CoreBluetoothReplyFuture::default();
        internal.services_discovered_future_state = Some(discovery.get_state_clone());

        let (delegate_sender, mut delegate_receiver) = mpsc::channel(1);
        let delegate = CentralDelegate::new(delegate_sender);
        let error = NSError::new(1, ns_string!("BtlePlugCoreBluetoothTests"));
        unsafe {
            delegate.peripheral_didDiscoverDescriptorsForCharacteristic_error(
                &peripheral,
                &characteristic,
                Some(&error),
            );
        }

        let event = tokio::time::timeout(Duration::from_secs(1), delegate_receiver.next())
            .await
            .expect("descriptor error callback did not emit an event")
            .expect("delegate event channel closed");
        let CentralDelegateEvent::DiscoveredCharacteristicDescriptors {
            peripheral_uuid: event_peripheral_uuid,
            service_uuid: event_service_uuid,
            characteristic_uuid: event_characteristic_uuid,
            descriptors,
        } = event
        else {
            panic!("unexpected delegate event: {event:?}");
        };
        assert_eq!(event_peripheral_uuid, peripheral_uuid);
        assert_eq!(event_service_uuid, service_uuid);
        assert_eq!(event_characteristic_uuid, characteristic_uuid);
        assert!(descriptors.is_empty());

        internal.set_characteristic_descriptors(
            event_service_uuid,
            event_characteristic_uuid,
            descriptors,
        );
        let reply = tokio::time::timeout(Duration::from_secs(1), discovery)
            .await
            .expect("service discovery remained pending after descriptor error");
        let CoreBluetoothReply::ServicesDiscovered(services, mtu) = reply else {
            panic!("unexpected discovery reply: {reply:?}");
        };
        assert_eq!(mtu, crate::api::DEFAULT_MTU_SIZE);
        let characteristic = services
            .iter()
            .find(|service| service.uuid == service_uuid)
            .and_then(|service| {
                service
                    .characteristics
                    .iter()
                    .find(|characteristic| characteristic.uuid == characteristic_uuid)
            })
            .expect("discovered characteristic");
        assert!(characteristic.descriptors.is_empty());

        // CBPeripheral has no public initializer suitable for tests, so this
        // subclass must not run CoreBluetooth's private destruction path.
        std::mem::forget(internal);
        std::mem::forget(peripheral);
    }

    #[tokio::test]
    async fn notification_callback_error_completes_requested_operation() {
        for enabled in [true, false] {
            for notifying in [true, false] {
                for att_error in [false, true] {
                    let context =
                        format!("enabled={enabled} notifying={notifying} att_error={att_error}");
                    let characteristic_uuid =
                        Uuid::from_u128(0x00002a19_0000_1000_8000_00805f9b34fb);
                    let mut fixture = NotificationFixture::new(&[characteristic_uuid]);
                    let future = CoreBluetoothReplyFuture::default();
                    fixture.internal.queue_notification_request(
                        fixture.service_uuid,
                        characteristic_uuid,
                        enabled,
                        future.get_state_clone(),
                    );
                    assert_eq!(
                        fixture.set_notify_calls(),
                        vec![RecordedSetNotify {
                            characteristic_uuid,
                            enabled,
                        }],
                        "{context}"
                    );
                    fixture.characteristic(0).set_notifying(notifying);
                    let error = notification_error(att_error);
                    let expected_error = deliver_notification_state_callback(
                        &mut fixture,
                        0,
                        Some(&error),
                        &context,
                    )
                    .await
                    .expect("{context}: error callback carried no error");
                    let reply = tokio::time::timeout(CALLBACK_TIMEOUT, future)
                        .await
                        .unwrap_or_else(|_| {
                            panic!("{context}: notification error did not complete the request")
                        });
                    match reply {
                        CoreBluetoothReply::Err(actual) => {
                            assert_eq!(actual, expected_error, "{context}")
                        }
                        reply => panic!("{context}: unexpected reply: {reply:?}"),
                    }
                }
            }
        }
    }

    #[tokio::test]
    async fn notification_callback_success_completes_requested_operation() {
        let characteristic_uuid = Uuid::from_u128(0x00002a19_0000_1000_8000_00805f9b34fb);
        let mut fixture = NotificationFixture::new(&[characteristic_uuid]);

        let enable = CoreBluetoothReplyFuture::default();
        fixture.internal.queue_notification_request(
            fixture.service_uuid,
            characteristic_uuid,
            true,
            enable.get_state_clone(),
        );
        deliver_notification_state_callback(&mut fixture, 0, None, "enable").await;
        assert!(
            matches!(
                tokio::time::timeout(CALLBACK_TIMEOUT, enable)
                    .await
                    .expect("enable did not complete"),
                CoreBluetoothReply::Ok
            ),
            "enable reply was not Ok"
        );

        let disable = CoreBluetoothReplyFuture::default();
        fixture.internal.queue_notification_request(
            fixture.service_uuid,
            characteristic_uuid,
            false,
            disable.get_state_clone(),
        );
        deliver_notification_state_callback(&mut fixture, 0, None, "disable").await;
        assert!(
            matches!(
                tokio::time::timeout(CALLBACK_TIMEOUT, disable)
                    .await
                    .expect("disable did not complete"),
                CoreBluetoothReply::Ok
            ),
            "disable reply was not Ok"
        );

        // Repeated same-direction requests stay independent native calls and
        // futures; no coalescing.
        let first_repeat = CoreBluetoothReplyFuture::default();
        let mut second_repeat = CoreBluetoothReplyFuture::default();
        fixture.internal.queue_notification_request(
            fixture.service_uuid,
            characteristic_uuid,
            true,
            first_repeat.get_state_clone(),
        );
        fixture.internal.queue_notification_request(
            fixture.service_uuid,
            characteristic_uuid,
            true,
            second_repeat.get_state_clone(),
        );
        assert_eq!(fixture.set_notify_calls().len(), 3);
        deliver_notification_state_callback(&mut fixture, 0, None, "first repeat").await;
        assert!(
            matches!(
                tokio::time::timeout(CALLBACK_TIMEOUT, first_repeat)
                    .await
                    .expect("first repeat did not complete"),
                CoreBluetoothReply::Ok
            ),
            "first repeat reply was not Ok"
        );
        assert_pending(&mut second_repeat, "second repeat").await;
        deliver_notification_state_callback(&mut fixture, 0, None, "second repeat").await;
        assert!(
            matches!(
                tokio::time::timeout(CALLBACK_TIMEOUT, second_repeat)
                    .await
                    .expect("second repeat did not complete"),
                CoreBluetoothReply::Ok
            ),
            "second repeat reply was not Ok"
        );
        assert_eq!(
            fixture.set_notify_calls(),
            vec![
                RecordedSetNotify {
                    characteristic_uuid,
                    enabled: true
                },
                RecordedSetNotify {
                    characteristic_uuid,
                    enabled: false
                },
                RecordedSetNotify {
                    characteristic_uuid,
                    enabled: true
                },
                RecordedSetNotify {
                    characteristic_uuid,
                    enabled: true
                },
            ]
        );
    }

    #[tokio::test]
    async fn notification_requests_are_serialized_per_characteristic() {
        let characteristic_uuid = Uuid::from_u128(0x00002a19_0000_1000_8000_00805f9b34fb);
        let mut fixture = NotificationFixture::new(&[characteristic_uuid]);

        let first = CoreBluetoothReplyFuture::default();
        let mut second = CoreBluetoothReplyFuture::default();
        let mut third = CoreBluetoothReplyFuture::default();
        fixture.internal.queue_notification_request(
            fixture.service_uuid,
            characteristic_uuid,
            true,
            first.get_state_clone(),
        );
        assert_eq!(
            fixture.set_notify_calls(),
            vec![RecordedSetNotify {
                characteristic_uuid,
                enabled: true
            }]
        );
        fixture.internal.queue_notification_request(
            fixture.service_uuid,
            characteristic_uuid,
            false,
            second.get_state_clone(),
        );
        fixture.internal.queue_notification_request(
            fixture.service_uuid,
            characteristic_uuid,
            true,
            third.get_state_clone(),
        );
        assert_eq!(
            fixture.set_notify_calls().len(),
            1,
            "only the queue head may be submitted"
        );
        assert_pending(&mut second, "second").await;
        assert_pending(&mut third, "third").await;

        deliver_notification_state_callback(&mut fixture, 0, None, "first").await;
        assert!(
            matches!(
                tokio::time::timeout(CALLBACK_TIMEOUT, first)
                    .await
                    .expect("first did not complete"),
                CoreBluetoothReply::Ok
            ),
            "first reply was not Ok"
        );
        assert_eq!(
            fixture.set_notify_calls(),
            vec![
                RecordedSetNotify {
                    characteristic_uuid,
                    enabled: true
                },
                RecordedSetNotify {
                    characteristic_uuid,
                    enabled: false
                },
            ]
        );
        assert_pending(&mut third, "third").await;

        let error = notification_error(true);
        let expected_error =
            deliver_notification_state_callback(&mut fixture, 0, Some(&error), "second")
                .await
                .expect("second callback carried no error");
        let second_reply = tokio::time::timeout(CALLBACK_TIMEOUT, second)
            .await
            .expect("second did not complete");
        assert!(
            matches!(second_reply, CoreBluetoothReply::Err(ref actual) if *actual == expected_error),
            "second reply was not the callback error: {second_reply:?}"
        );
        assert_eq!(
            fixture.set_notify_calls().len(),
            3,
            "completing the head must submit exactly the next request"
        );
        assert_pending(&mut third, "third after second callback").await;

        deliver_notification_state_callback(&mut fixture, 0, None, "third").await;
        assert!(
            matches!(
                tokio::time::timeout(CALLBACK_TIMEOUT, third)
                    .await
                    .expect("third did not complete"),
                CoreBluetoothReply::Ok
            ),
            "third reply was not Ok"
        );
        assert_eq!(fixture.set_notify_calls().len(), 3);
    }

    #[tokio::test]
    async fn notification_requests_on_distinct_characteristics_are_independent() {
        let characteristic_a = Uuid::from_u128(0x00002a19_0000_1000_8000_00805f9b34fb);
        let characteristic_b = Uuid::from_u128(0x00002a37_0000_1000_8000_00805f9b34fb);
        let mut fixture = NotificationFixture::new(&[characteristic_a, characteristic_b]);

        let future_a = CoreBluetoothReplyFuture::default();
        let mut future_b = CoreBluetoothReplyFuture::default();
        let mut future_b_second = CoreBluetoothReplyFuture::default();
        fixture.internal.queue_notification_request(
            fixture.service_uuid,
            characteristic_a,
            true,
            future_a.get_state_clone(),
        );
        assert_eq!(
            fixture.set_notify_calls(),
            vec![RecordedSetNotify {
                characteristic_uuid: characteristic_a,
                enabled: true
            }]
        );
        fixture.internal.queue_notification_request(
            fixture.service_uuid,
            characteristic_b,
            true,
            future_b.get_state_clone(),
        );
        assert_eq!(
            fixture.set_notify_calls(),
            vec![
                RecordedSetNotify {
                    characteristic_uuid: characteristic_a,
                    enabled: true
                },
                RecordedSetNotify {
                    characteristic_uuid: characteristic_b,
                    enabled: true
                },
            ]
        );
        fixture.internal.queue_notification_request(
            fixture.service_uuid,
            characteristic_b,
            false,
            future_b_second.get_state_clone(),
        );
        assert_eq!(
            fixture.set_notify_calls().len(),
            2,
            "each characteristic must own its queue"
        );

        let error = notification_error(true);
        let expected_error =
            deliver_notification_state_callback(&mut fixture, 0, Some(&error), "a")
                .await
                .expect("a callback carried no error");
        let reply_a = tokio::time::timeout(CALLBACK_TIMEOUT, future_a)
            .await
            .expect("a did not complete");
        assert!(
            matches!(reply_a, CoreBluetoothReply::Err(ref actual) if *actual == expected_error),
            "a reply was not the callback error: {reply_a:?}"
        );
        assert_pending(&mut future_b, "b").await;
        assert_pending(&mut future_b_second, "b second").await;
        assert_eq!(
            fixture.set_notify_calls().len(),
            2,
            "a callback for one characteristic must not advance another"
        );

        deliver_notification_state_callback(&mut fixture, 1, None, "b").await;
        assert!(
            matches!(
                tokio::time::timeout(CALLBACK_TIMEOUT, future_b)
                    .await
                    .expect("b did not complete"),
                CoreBluetoothReply::Ok
            ),
            "b reply was not Ok"
        );
        assert_eq!(fixture.set_notify_calls().len(), 3);
        assert_pending(&mut future_b_second, "b second after first b callback").await;
        deliver_notification_state_callback(&mut fixture, 1, None, "b second").await;
        assert!(
            matches!(
                tokio::time::timeout(CALLBACK_TIMEOUT, future_b_second)
                    .await
                    .expect("b second did not complete"),
                CoreBluetoothReply::Ok
            ),
            "b second reply was not Ok"
        );
    }

    #[tokio::test]
    async fn cancelled_notification_waiter_does_not_steal_next_reply() {
        let characteristic_uuid = Uuid::from_u128(0x00002a19_0000_1000_8000_00805f9b34fb);
        let mut fixture = NotificationFixture::new(&[characteristic_uuid]);

        let dropped_waiter = CoreBluetoothReplyFuture::default();
        let dropped_state = dropped_waiter.get_state_clone();
        drop(dropped_waiter);
        fixture.internal.queue_notification_request(
            fixture.service_uuid,
            characteristic_uuid,
            true,
            dropped_state,
        );

        let mut survivor = CoreBluetoothReplyFuture::default();
        fixture.internal.queue_notification_request(
            fixture.service_uuid,
            characteristic_uuid,
            false,
            survivor.get_state_clone(),
        );
        assert_eq!(
            fixture.set_notify_calls(),
            vec![RecordedSetNotify {
                characteristic_uuid,
                enabled: true
            }],
            "the second request must not be submitted while the first is in flight"
        );

        deliver_notification_state_callback(&mut fixture, 0, None, "dropped waiter").await;
        assert_eq!(
            fixture.set_notify_calls(),
            vec![
                RecordedSetNotify {
                    characteristic_uuid,
                    enabled: true
                },
                RecordedSetNotify {
                    characteristic_uuid,
                    enabled: false
                },
            ],
            "the dropped waiter's callback must submit the next request"
        );
        assert_pending(&mut survivor, "survivor").await;

        deliver_notification_state_callback(&mut fixture, 0, None, "survivor").await;
        assert!(
            matches!(
                tokio::time::timeout(CALLBACK_TIMEOUT, survivor)
                    .await
                    .expect("survivor did not complete"),
                CoreBluetoothReply::Ok
            ),
            "survivor reply was not Ok"
        );
    }

    #[tokio::test]
    async fn disconnect_drains_active_and_queued_notification_requests() {
        let characteristic_uuid = Uuid::from_u128(0x00002a19_0000_1000_8000_00805f9b34fb);
        let mut fixture = NotificationFixture::new(&[characteristic_uuid]);

        let first = CoreBluetoothReplyFuture::default();
        let second = CoreBluetoothReplyFuture::default();
        let third = CoreBluetoothReplyFuture::default();
        fixture.internal.queue_notification_request(
            fixture.service_uuid,
            characteristic_uuid,
            true,
            first.get_state_clone(),
        );
        fixture.internal.queue_notification_request(
            fixture.service_uuid,
            characteristic_uuid,
            false,
            second.get_state_clone(),
        );
        fixture.internal.queue_notification_request(
            fixture.service_uuid,
            characteristic_uuid,
            true,
            third.get_state_clone(),
        );
        assert_eq!(fixture.set_notify_calls().len(), 1);

        fixture
            .internal
            .drain_pending_operations("Device disconnected");

        for (name, future) in [("first", first), ("second", second), ("third", third)] {
            let reply = tokio::time::timeout(CALLBACK_TIMEOUT, future)
                .await
                .unwrap_or_else(|_| panic!("{name} did not drain"));
            assert!(
                matches!(reply, CoreBluetoothReply::Err(ref message) if message == "Device disconnected"),
                "{name}: unexpected drain reply: {reply:?}"
            );
        }
        assert_eq!(
            fixture.set_notify_calls().len(),
            1,
            "draining must not submit more requests"
        );

        deliver_notification_state_callback(&mut fixture, 0, None, "late callback after drain")
            .await;
        assert_eq!(
            fixture.set_notify_calls().len(),
            1,
            "a callback after draining must submit nothing"
        );
    }

    #[tokio::test]
    async fn unexpected_notification_callback_is_ignored() {
        let characteristic_a = Uuid::from_u128(0x00002a19_0000_1000_8000_00805f9b34fb);
        let mut fixture = NotificationFixture::new(&[characteristic_a]);
        let error = notification_error(true);
        deliver_notification_state_callback(
            &mut fixture,
            0,
            Some(&error),
            "callback with empty queue",
        )
        .await;
        assert!(fixture.set_notify_calls().is_empty());

        let characteristic_b = Uuid::from_u128(0x00002a37_0000_1000_8000_00805f9b34fb);
        let mut fixture = NotificationFixture::new(&[characteristic_a, characteristic_b]);
        let mut future_a = CoreBluetoothReplyFuture::default();
        fixture.internal.queue_notification_request(
            fixture.service_uuid,
            characteristic_a,
            true,
            future_a.get_state_clone(),
        );
        deliver_notification_state_callback(
            &mut fixture,
            1,
            None,
            "callback for characteristic without a pending request",
        )
        .await;
        assert_eq!(
            fixture.set_notify_calls().len(),
            1,
            "unexpected callback must not consume another characteristic's request"
        );
        assert_pending(&mut future_a, "a after unrelated callback").await;
        deliver_notification_state_callback(&mut fixture, 0, None, "a").await;
        assert!(
            matches!(
                tokio::time::timeout(CALLBACK_TIMEOUT, future_a)
                    .await
                    .expect("a did not complete"),
                CoreBluetoothReply::Ok
            ),
            "a reply was not Ok"
        );
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
