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

    fn drain_pending_operations(&mut self, error: &CoreBluetoothReply) {
        for queue in [&mut self.read_future_state, &mut self.write_future_state] {
            for state in queue.drain(..) {
                state.lock().unwrap().set_reply(error.clone());
            }
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

    fn drain_pending_operations(&mut self, error: &CoreBluetoothReply) {
        for queue in [&mut self.read_future_state, &mut self.write_future_state] {
            for state in queue.drain(..) {
                state.lock().unwrap().set_reply(error.clone());
            }
        }
        // Drains the submitted head along with the unsent tail; no further
        // request is submitted afterwards.
        for request in self.notification_requests.drain(..) {
            request.future.lock().unwrap().set_reply(error.clone());
        }
        for descriptor in self.descriptors.values_mut() {
            descriptor.drain_pending_operations(error);
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

impl ServiceInternal {
    fn drain_pending_operations(&mut self, error: &CoreBluetoothReply) {
        for characteristic in self.characteristics.values_mut() {
            characteristic.drain_pending_operations(error);
        }
    }
}

/// Error and remove queued write-without-response requests matching `matches`,
/// keeping the rest in queue order.
fn drain_write_without_response_matching(
    queue: &mut VecDeque<PendingWriteWithoutResponse>,
    error: &CoreBluetoothReply,
    mut matches: impl FnMut(&PendingWriteWithoutResponse) -> bool,
) {
    let mut remaining = VecDeque::with_capacity(queue.len());
    for pending in queue.drain(..) {
        if matches(&pending) {
            pending.fut.lock().unwrap().set_reply(error.clone());
        } else {
            remaining.push_back(pending);
        }
    }
    *queue = remaining;
}

struct PeripheralInternal {
    pub peripheral: Retained<CBPeripheral>,
    services: HashMap<Uuid, ServiceInternal>,
    pub event_sender: Sender<PeripheralEventInternal>,
    pub disconnected_future_state: VecDeque<CoreBluetoothReplyStateShared>,
    pub connected_future_state: VecDeque<CoreBluetoothReplyStateShared>,
    pub services_discovered_future_state: VecDeque<CoreBluetoothReplyStateShared>,
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
            connected_future_state: VecDeque::with_capacity(2),
            disconnected_future_state: VecDeque::with_capacity(2),
            services_discovered_future_state: VecDeque::with_capacity(2),
            read_rssi_future_state: VecDeque::with_capacity(4),
            write_without_response_queue: VecDeque::new(),
        }
    }

    pub fn set_discovered_services(
        &mut self,
        service_map: HashMap<Uuid, Retained<CBService>>,
        error: Option<String>,
    ) {
        if let Some(error) = error {
            let reply = CoreBluetoothReply::Err(error);
            for future_state in self.services_discovered_future_state.drain(..) {
                future_state.lock().unwrap().set_reply(reply.clone());
            }
            return;
        }
        let removed_uuids: Vec<Uuid> = self
            .services
            .keys()
            .filter(|uuid| !service_map.contains_key(uuid))
            .copied()
            .collect();
        self.remove_services(
            &removed_uuids,
            "Service no longer present after rediscovery",
        );

        for (service_uuid, cbservice) in service_map {
            match self.services.get_mut(&service_uuid) {
                Some(existing) => {
                    // Keep characteristics/descriptors (and their in-flight
                    // queues); only the discovery gate resets, since CB
                    // always redrives characteristic/descriptor discovery
                    // after didDiscoverServices.
                    existing.cbservice = cbservice;
                    existing.discovered = false;
                }
                None => {
                    self.services.insert(
                        service_uuid,
                        ServiceInternal {
                            cbservice,
                            characteristics: HashMap::new(),
                            discovered: false,
                        },
                    );
                }
            }
        }
        // Completes immediately when there are no services.
        self.check_discovered();
    }

    /// Remove the given services, erroring all their pending futures.
    /// Returns whether any removed service was still mid-round
    /// (`discovered == false`), i.e. its callers may be gating on it.
    fn remove_services(&mut self, removed_uuids: &[Uuid], message: &str) -> bool {
        if removed_uuids.is_empty() {
            return false;
        }
        let error = CoreBluetoothReply::Err(message.to_string());
        let mut any_undiscovered = false;
        for uuid in removed_uuids {
            if let Some(mut service) = self.services.remove(uuid) {
                any_undiscovered |= !service.discovered;
                service.drain_pending_operations(&error);
            }
        }
        drain_write_without_response_matching(
            &mut self.write_without_response_queue,
            &error,
            |pending| removed_uuids.contains(&pending.service_uuid),
        );
        any_undiscovered
    }

    pub fn set_characteristics(
        &mut self,
        service_uuid: Uuid,
        characteristics: HashMap<Uuid, Retained<CBCharacteristic>>,
        error: bool,
    ) {
        let Some(service) = self.services.get_mut(&service_uuid) else {
            debug!("Ignoring characteristics for unknown service {service_uuid}");
            return;
        };
        if error {
            // `characteristics` is an empty placeholder here, not the
            // service's real set (see #167 duplicate/late callbacks): leave
            // existing state untouched and just complete the service.
            service.discovered = true;
            self.check_discovered();
            return;
        }

        let removed_uuids: Vec<Uuid> = service
            .characteristics
            .keys()
            .filter(|uuid| !characteristics.contains_key(uuid))
            .copied()
            .collect();
        let removed_error = (!removed_uuids.is_empty()).then(|| {
            CoreBluetoothReply::Err(
                "Characteristic no longer present after rediscovery".to_string(),
            )
        });
        if let Some(removed_error) = &removed_error {
            for uuid in &removed_uuids {
                if let Some(mut characteristic) = service.characteristics.remove(uuid) {
                    characteristic.drain_pending_operations(removed_error);
                }
            }
        }
        for (characteristic_uuid, cb_characteristic) in characteristics {
            if let Some(existing) = service.characteristics.get_mut(&characteristic_uuid) {
                // Preserve in-flight future state and already-discovered
                // descriptors to avoid dropping pending operations during
                // late re-discovery events (see issue #167).
                existing.properties = CharacteristicInternal::form_flags(&cb_characteristic);
                existing.characteristic = cb_characteristic;
                // CB redrives descriptor discovery for each characteristic.
                existing.discovered = false;
            } else {
                service.characteristics.insert(
                    characteristic_uuid,
                    CharacteristicInternal::new(cb_characteristic),
                );
            }
        }
        let now_empty = service.characteristics.is_empty();
        if now_empty {
            service.discovered = true;
        }

        if let Some(removed_error) = &removed_error {
            drain_write_without_response_matching(
                &mut self.write_without_response_queue,
                removed_error,
                |pending| {
                    pending.service_uuid == service_uuid
                        && removed_uuids.contains(&pending.characteristic_uuid)
                },
            );
        }
        if now_empty {
            self.check_discovered();
        }
    }

    pub fn set_characteristic_descriptors(
        &mut self,
        service_uuid: Uuid,
        characteristic_uuid: Uuid,
        descriptors: HashMap<Uuid, Retained<CBDescriptor>>,
        error: bool,
    ) -> bool {
        let Some(service) = self.services.get_mut(&service_uuid) else {
            debug!("Ignoring descriptors for unknown service {service_uuid}");
            return true;
        };
        let Some(characteristic) = service.characteristics.get_mut(&characteristic_uuid) else {
            return false;
        };
        if error {
            // `descriptors` is an empty placeholder here, not the
            // characteristic's real set: leave existing state untouched.
            characteristic.discovered = true;
        } else {
            let removed_uuids: Vec<Uuid> = characteristic
                .descriptors
                .keys()
                .filter(|uuid| !descriptors.contains_key(uuid))
                .copied()
                .collect();
            if !removed_uuids.is_empty() {
                let removed_error = CoreBluetoothReply::Err(
                    "Descriptor no longer present after rediscovery".to_string(),
                );
                for uuid in &removed_uuids {
                    if let Some(mut descriptor) = characteristic.descriptors.remove(uuid) {
                        descriptor.drain_pending_operations(&removed_error);
                    }
                }
            }
            for (descriptor_uuid, cb_descriptor) in descriptors {
                if let Some(existing) = characteristic.descriptors.get_mut(&descriptor_uuid) {
                    // Update the CB object reference but preserve in-flight
                    // future state to avoid dropping pending operations
                    // during late re-discovery events (see issue #167).
                    existing.descriptor = cb_descriptor;
                } else {
                    characteristic
                        .descriptors
                        .insert(descriptor_uuid, DescriptorInternal::new(cb_descriptor));
                }
            }
            characteristic.discovered = true;
        }

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
            if self.services_discovered_future_state.is_empty() {
                trace!("Services discovered with no pending future; ignoring");
                return;
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
            for future_state in self.services_discovered_future_state.drain(..) {
                future_state.lock().unwrap().set_reply(reply.clone());
            }
        }
    }

    pub fn confirm_disconnect(&mut self) {
        self.drain_pending_operations("Device disconnected");
    }

    /// Resolve every queued connect waiter with the same outcome, since a
    /// connect/connect-failure callback carries no per-request identity to
    /// attach it to a single caller.
    pub fn complete_connect(&mut self, reply: CoreBluetoothReply) {
        for future in self.connected_future_state.drain(..) {
            future.lock().unwrap().set_reply(reply.clone());
        }
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
        for queue in [
            &mut self.disconnected_future_state,
            &mut self.connected_future_state,
            &mut self.services_discovered_future_state,
        ] {
            for future in queue.drain(..) {
                future.lock().unwrap().set_reply(error.clone());
            }
        }
        for state in self.read_rssi_future_state.drain(..) {
            state.lock().unwrap().set_reply(error.clone());
        }
        for pending in self.write_without_response_queue.drain(..) {
            pending.fut.lock().unwrap().set_reply(error.clone());
        }
        for service in self.services.values_mut() {
            service.drain_pending_operations(&error);
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
    // Gates DiscoveredPeripheral and advertisement-derived (ManufacturerData,
    // ServiceData, Services, TxPowerLevel) delegate events: CBqueue callbacks
    // queued before stopScan can still arrive after StopScanning/
    // ClearPeripherals are processed, so we drop them instead of re-adding a
    // peripheral or reporting stale advertisement data.
    scanning: bool,
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
            .field("scanning", &self.scanning)
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
            scanning: false,
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
            if let Some(mut p) = self.peripherals.remove(&peripheral_uuid) {
                p.drain_pending_operations("Peripheral event receiver is gone");
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
            if let Some(mut p) = self.peripherals.remove(&peripheral_uuid) {
                p.drain_pending_operations("Peripheral event receiver is gone");
            }
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
            if let Some(mut p) = self.peripherals.remove(&peripheral_uuid) {
                p.drain_pending_operations("Peripheral event receiver is gone");
            }
        }
    }

    async fn on_services_modified(
        &mut self,
        peripheral_uuid: Uuid,
        invalidated_services: Vec<Uuid>,
    ) {
        trace!(
            "Peripheral modified services and must be rediscovered! {:?}",
            peripheral_uuid
        );
        if let Some(p) = self.peripherals.get_mut(&peripheral_uuid) {
            let was_mid_round = p.remove_services(
                &invalidated_services,
                "Service invalidated; rediscovery required",
            );
            // Only complete a round already in progress; stale leftovers stay pending.
            if was_mid_round {
                p.check_discovered();
            }
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
        error: Option<String>,
    ) {
        trace!("Found services!");
        for id in service_map.keys() {
            trace!("{}", id);
        }
        if let Some(p) = self.peripherals.get_mut(&peripheral_uuid) {
            p.set_discovered_services(service_map, error);
        }
    }

    fn on_discovered_characteristics(
        &mut self,
        peripheral_uuid: Uuid,
        service_uuid: Uuid,
        characteristics: HashMap<Uuid, Retained<CBCharacteristic>>,
        error: bool,
    ) {
        trace!(
            "Found characteristics for peripheral {} service {}:",
            peripheral_uuid, service_uuid
        );
        for id in characteristics.keys() {
            trace!("{}", id);
        }
        if let Some(p) = self.peripherals.get_mut(&peripheral_uuid) {
            p.set_characteristics(service_uuid, characteristics, error);
        }
    }

    fn on_discovered_characteristic_descriptors(
        &mut self,
        peripheral_uuid: Uuid,
        service_uuid: Uuid,
        characteristic_uuid: Uuid,
        descriptors: HashMap<Uuid, Retained<CBDescriptor>>,
        error: bool,
    ) {
        trace!(
            "Found descriptors for peripheral {} service {} characteristic {}:",
            peripheral_uuid, service_uuid, characteristic_uuid,
        );
        for id in descriptors.keys() {
            trace!("{}", id);
        }
        if let Some(p) = self.peripherals.get_mut(&peripheral_uuid)
            && !p.set_characteristic_descriptors(
                service_uuid,
                characteristic_uuid,
                descriptors,
                error,
            )
        {
            let reply = CoreBluetoothReply::Err(format!(
                "Unknown descriptor relationship for service {service_uuid}, characteristic {characteristic_uuid}"
            ));
            for future in p.services_discovered_future_state.drain(..) {
                future.lock().unwrap().set_reply(reply.clone());
            }
        }
    }

    fn on_peripheral_connect(&mut self, peripheral_uuid: Uuid) {
        if let Some(peripheral) = self.peripherals.get_mut(&peripheral_uuid) {
            if peripheral.connected_future_state.is_empty() {
                debug!(
                    "Ignoring duplicate connection callback for peripheral {}",
                    peripheral_uuid
                );
            } else {
                peripheral.complete_connect(CoreBluetoothReply::Connected);
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
        if let Some(peripheral) = self.peripherals.get_mut(&peripheral_uuid) {
            if peripheral.connected_future_state.is_empty() {
                debug!(
                    "Ignoring duplicate connection failure callback for peripheral {}",
                    peripheral_uuid
                );
            } else {
                peripheral.complete_connect(CoreBluetoothReply::Err(error));
            }
        }
    }

    async fn on_adapter_powered_off(&mut self) {
        warn!("Adapter powered off, canceling all pending operations");
        self.scanning = false;
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
            p.connected_future_state.push_back(fut);
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
            p.disconnected_future_state.push_back(fut);
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
            p.services_discovered_future_state.push_back(fut);
            // This will trigger the delegate_peripheral_diddiscoverservices in central_delegate.rs
            unsafe { p.peripheral.discoverServices(None) };
        } else {
            fut.lock().unwrap().set_reply(CoreBluetoothReply::Err(
                "Peripheral no longer available".into(),
            ));
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
                    CentralDelegateEvent::DiscoveredPeripheral{..}
                    | CentralDelegateEvent::ManufacturerData{..}
                    | CentralDelegateEvent::ServiceData{..}
                    | CentralDelegateEvent::Services{..}
                    | CentralDelegateEvent::TxPowerLevel{..}
                        if !self.scanning =>
                    {
                        trace!("Ignoring discovery/advertisement delegate event while not scanning");
                    }
                    CentralDelegateEvent::DiscoveredPeripheral{cbperipheral, advertisement_name} => {
                        self.on_discovered_peripheral(cbperipheral, advertisement_name).await
                    }
                    CentralDelegateEvent::DiscoveredServices{peripheral_uuid, services, error} => {
                        self.on_discovered_services(peripheral_uuid, services, error)
                    }
                    CentralDelegateEvent::DiscoveredCharacteristics{peripheral_uuid, service_uuid, characteristics, error} => {
                        self.on_discovered_characteristics(peripheral_uuid, service_uuid, characteristics, error)
                    }
                    CentralDelegateEvent::DiscoveredCharacteristicDescriptors{peripheral_uuid, service_uuid, characteristic_uuid, descriptors, error} => {
                        self.on_discovered_characteristic_descriptors(peripheral_uuid, service_uuid, characteristic_uuid, descriptors, error)
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
                    CentralDelegateEvent::ServicesModified{peripheral_uuid, invalidated_services} => {
                        self.on_services_modified(peripheral_uuid, invalidated_services).await
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
                        for p in self.peripherals.values_mut() {
                            // Best-effort: the resulting disconnect callback,
                            // if any, will find no tracked peripheral and be
                            // ignored (see on_peripheral_disconnect).
                            unsafe { self.manager.cancelPeripheralConnection(&p.peripheral) };
                            p.drain_pending_operations("Peripheral cleared");
                        }
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
        self.scanning = true;
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
        self.scanning = false;
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

#[cfg(test)]
mod tests;
