// btleplug Source Code File
//
// Copyright 2020 Nonpolynomial Labs LLC. All rights reserved.
//
// Licensed under the BSD 3-Clause license. See LICENSE file in the project root
// for full license information.

use super::{
    framework::cb::CBPeripheralState,
    internal::{
        CBPeripheralEvent, CoreBluetoothMessage, CoreBluetoothReply, CoreBluetoothReplyFuture,
    },
};
use crate::{
    api::{
        self, BDAddr, CentralEvent, CharPropFlags, Characteristic, Descriptor,
        PeripheralProperties, Service, ValueNotification, WriteType,
    },
    common::{adapter_manager::AdapterManager, util::notifications_stream_from_broadcast_receiver},
    Error, Result,
};
use async_trait::async_trait;
use futures::channel::mpsc::{Receiver, SendError, Sender};
use futures::sink::SinkExt;
use futures::stream::{Stream, StreamExt};
use log::*;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "serde")]
use serde_cr as serde;
use std::sync::Weak;
use std::{
    collections::{BTreeSet, HashMap},
    fmt::{self, Debug, Display, Formatter},
    pin::Pin,
    sync::{Arc, Mutex},
};
use tokio::sync::broadcast;
use tokio::task;
use uuid::Uuid;

#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(crate = "serde_cr")
)]
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PeripheralId(Uuid);

impl Display for PeripheralId {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        Display::fmt(&self.0, f)
    }
}

/// Implementation of [api::Peripheral](crate::api::Peripheral).
#[derive(Clone)]
pub struct Peripheral {
    shared: Arc<Shared>,
}

struct Shared {
    notifications_channel: broadcast::Sender<ValueNotification>,
    manager: Weak<AdapterManager<Peripheral>>,
    uuid: Uuid,
    services: Mutex<BTreeSet<Service>>,
    properties: Mutex<PeripheralProperties>,
    message_sender: Sender<CoreBluetoothMessage>,
    // We're not actually holding a peripheral object here, that's held out in
    // the objc thread. We'll just communicate with it through our
    // receiver/sender pair.
}

impl Shared {
    fn emit_event(&self, event: CentralEvent) {
        if let Some(manager) = self.manager.upgrade() {
            manager.emit(event);
        } else {
            trace!("Could not emit an event. AdapterManager has been dropped");
        }
    }
}

impl Peripheral {
    // This calls tokio::task::spawn, so it must be called from the context of a Tokio Runtime.
    pub(crate) fn new(
        uuid: Uuid,
        local_name: Option<String>,
        manager: Weak<AdapterManager<Self>>,
        event_receiver: Receiver<CBPeripheralEvent>,
        message_sender: Sender<CoreBluetoothMessage>,
    ) -> Self {
        // Since we're building the object, we have an active advertisement.
        // Build properties now.
        let properties = Mutex::from(PeripheralProperties {
            address: BDAddr::default(),
            address_type: None,
            local_name,
            tx_power_level: None,
            rssi: None,
            manufacturer_data: HashMap::new(),
            service_data: HashMap::new(),
            services: Vec::new(),
        });
        let (notifications_channel, _) = broadcast::channel(16);

        let shared = Arc::new(Shared {
            properties,
            manager,
            services: Mutex::new(BTreeSet::new()),
            notifications_channel,
            uuid,
            message_sender,
        });
        let shared_clone = shared.clone();
        task::spawn(async move {
            let mut event_receiver = event_receiver;
            let shared = shared_clone;

            loop {
                match event_receiver.next().await {
                    Some(CBPeripheralEvent::Notification(uuid, data)) => {
                        let notification = ValueNotification { uuid, value: data };

                        // Note: we ignore send errors here which may happen while there are no
                        // receivers...
                        let _ = shared.notifications_channel.send(notification);
                    }
                    Some(CBPeripheralEvent::ManufacturerData(manufacturer_id, data, rssi)) => {
                        let mut properties = shared.properties.lock().unwrap();
                        properties.rssi = Some(rssi);
                        properties
                            .manufacturer_data
                            .insert(manufacturer_id, data.clone());
                        shared.emit_event(CentralEvent::ManufacturerDataAdvertisement {
                            id: shared.uuid.into(),
                            manufacturer_data: properties.manufacturer_data.clone(),
                        });
                    }
                    Some(CBPeripheralEvent::ServiceData(service_data, rssi)) => {
                        let mut properties = shared.properties.lock().unwrap();
                        properties.rssi = Some(rssi);
                        properties.service_data.extend(service_data.clone());

                        shared.emit_event(CentralEvent::ServiceDataAdvertisement {
                            id: shared.uuid.into(),
                            service_data,
                        });
                    }
                    Some(CBPeripheralEvent::Services(services, rssi)) => {
                        let mut properties = shared.properties.lock().unwrap();
                        properties.rssi = Some(rssi);
                        properties.services = services.clone();

                        shared.emit_event(CentralEvent::ServicesAdvertisement {
                            id: shared.uuid.into(),
                            services,
                        });
                    }
                    Some(CBPeripheralEvent::Disconnected) => (),
                    None => {
                        info!("Event receiver died, breaking out of corebluetooth device loop.");
                        break;
                    }
                }
            }
        });
        Self { shared: shared }
    }

    pub(super) fn update_name(&self, name: &str) {
        self.shared.properties.lock().unwrap().local_name = Some(name.to_string());
    }
}

impl Display for Peripheral {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        // let connected = if self.is_connected() { " connected" } else { "" };
        // let properties = self.properties.lock().unwrap();
        // write!(f, "{} {}{}", self.address, properties.local_name.clone()
        //     .unwrap_or_else(|| "(unknown)".to_string()), connected)
        write!(f, "Peripheral")
    }
}

impl Debug for Peripheral {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.debug_struct("Peripheral")
            .field("uuid", &self.shared.uuid)
            .field("services", &self.shared.services)
            .field("properties", &self.shared.properties)
            .field("message_sender", &self.shared.message_sender)
            .finish()
    }
}

#[async_trait]
impl api::Peripheral for Peripheral {
    fn id(&self) -> PeripheralId {
        PeripheralId(self.shared.uuid)
    }

    fn address(&self) -> BDAddr {
        BDAddr::default()
    }

    async fn properties(&self) -> Result<Option<PeripheralProperties>> {
        Ok(Some(self.shared.properties.lock().unwrap().clone()))
    }

    fn services(&self) -> BTreeSet<Service> {
        self.shared.services.lock().unwrap().clone()
    }

    async fn is_connected(&self) -> Result<bool> {
        let fut = CoreBluetoothReplyFuture::default();
        self.shared
            .message_sender
            .to_owned()
            .send(CoreBluetoothMessage::IsConnected {
                peripheral_uuid: self.shared.uuid,
                future: fut.get_state_clone(),
            })
            .await?;
        match fut.await {
            CoreBluetoothReply::State(state) => match state {
                CBPeripheralState::Connected => Ok(true),
                _ => Ok(false),
            },
            _ => panic!("Shouldn't get anything but a State!"),
        }
    }

    async fn connect(&self) -> Result<()> {
        let fut = CoreBluetoothReplyFuture::default();
        self.shared
            .message_sender
            .to_owned()
            .send(CoreBluetoothMessage::ConnectDevice {
                peripheral_uuid: self.shared.uuid,
                future: fut.get_state_clone(),
            })
            .await?;
        match fut.await {
            CoreBluetoothReply::Connected(services) => {
                *(self.shared.services.lock().unwrap()) = services;
                self.shared
                    .emit_event(CentralEvent::DeviceConnected(self.shared.uuid.into()));
            }
            _ => panic!("Shouldn't get anything but connected!"),
        }
        trace!("Device connected!");
        Ok(())
    }

    async fn disconnect(&self) -> Result<()> {
        let fut = CoreBluetoothReplyFuture::default();
        self.shared
            .message_sender
            .to_owned()
            .send(CoreBluetoothMessage::DisconnectDevice {
                peripheral_uuid: self.shared.uuid,
                future: fut.get_state_clone(),
            })
            .await?;
        match fut.await {
            CoreBluetoothReply::Ok => {
                self.shared
                    .emit_event(CentralEvent::DeviceDisconnected(self.shared.uuid.into()));
                trace!("Device disconnected!");
            }
            _ => error!("Shouldn't get anything but Ok!"),
        }
        Ok(())
    }

    async fn discover_services(&self) -> Result<()> {
        // TODO: Actually discover on this, rather than on connection
        Ok(())
    }

    async fn write(
        &self,
        characteristic: &Characteristic,
        data: &[u8],
        mut write_type: WriteType,
    ) -> Result<()> {
        let fut = CoreBluetoothReplyFuture::default();
        // If we get WriteWithoutResponse for a characteristic that only
        // supports WriteWithResponse, slam the type to WriteWithResponse.
        // Otherwise we won't handle the future correctly.
        if write_type == WriteType::WithoutResponse
            && !characteristic
                .properties
                .contains(CharPropFlags::WRITE_WITHOUT_RESPONSE)
        {
            write_type = WriteType::WithResponse
        }
        self.shared
            .message_sender
            .to_owned()
            .send(CoreBluetoothMessage::WriteValue {
                peripheral_uuid: self.shared.uuid,
                service_uuid: characteristic.service_uuid,
                characteristic_uuid: characteristic.uuid,
                data: Vec::from(data),
                write_type,
                future: fut.get_state_clone(),
            })
            .await?;
        match fut.await {
            CoreBluetoothReply::Ok => {}
            reply => panic!("Unexpected reply: {:?}", reply),
        }
        Ok(())
    }

    async fn read(&self, characteristic: &Characteristic) -> Result<Vec<u8>> {
        let fut = CoreBluetoothReplyFuture::default();
        self.shared
            .message_sender
            .to_owned()
            .send(CoreBluetoothMessage::ReadValue {
                peripheral_uuid: self.shared.uuid,
                service_uuid: characteristic.service_uuid,
                characteristic_uuid: characteristic.uuid,
                future: fut.get_state_clone(),
            })
            .await?;
        match fut.await {
            CoreBluetoothReply::ReadResult(chars) => Ok(chars),
            _ => {
                panic!("Shouldn't get anything but read result!");
            }
        }
    }

    async fn subscribe(&self, characteristic: &Characteristic) -> Result<()> {
        let fut = CoreBluetoothReplyFuture::default();
        self.shared
            .message_sender
            .to_owned()
            .send(CoreBluetoothMessage::Subscribe {
                peripheral_uuid: self.shared.uuid,
                service_uuid: characteristic.service_uuid,
                characteristic_uuid: characteristic.uuid,
                future: fut.get_state_clone(),
            })
            .await?;
        match fut.await {
            CoreBluetoothReply::Ok => trace!("subscribed!"),
            _ => panic!("Didn't subscribe!"),
        }
        Ok(())
    }

    async fn unsubscribe(&self, characteristic: &Characteristic) -> Result<()> {
        let fut = CoreBluetoothReplyFuture::default();
        self.shared
            .message_sender
            .to_owned()
            .send(CoreBluetoothMessage::Unsubscribe {
                peripheral_uuid: self.shared.uuid,
                service_uuid: characteristic.service_uuid,
                characteristic_uuid: characteristic.uuid,
                future: fut.get_state_clone(),
            })
            .await?;
        match fut.await {
            CoreBluetoothReply::Ok => {}
            _ => panic!("Didn't unsubscribe!"),
        }
        Ok(())
    }

    async fn notifications(&self) -> Result<Pin<Box<dyn Stream<Item = ValueNotification> + Send>>> {
        let receiver = self.shared.notifications_channel.subscribe();
        Ok(notifications_stream_from_broadcast_receiver(receiver))
    }

    async fn write_descriptor(&self, descriptor: &Descriptor, data: &[u8]) -> Result<()> {
        let fut = CoreBluetoothReplyFuture::default();
        self.shared
            .message_sender
            .to_owned()
            .send(CoreBluetoothMessage::WriteDescriptorValue {
                peripheral_uuid: self.shared.uuid,
                service_uuid: descriptor.service_uuid,
                characteristic_uuid: descriptor.characteristic_uuid,
                descriptor_uuid: descriptor.uuid,
                data: Vec::from(data),
                future: fut.get_state_clone(),
            })
            .await?;
        match fut.await {
            CoreBluetoothReply::Ok => {}
            reply => panic!("Unexpected reply: {:?}", reply),
        }
        Ok(())
    }

    async fn read_descriptor(&self, descriptor: &Descriptor) -> Result<Vec<u8>> {
        let fut = CoreBluetoothReplyFuture::default();
        self.shared
            .message_sender
            .to_owned()
            .send(CoreBluetoothMessage::ReadDescriptorValue {
                peripheral_uuid: self.shared.uuid,
                service_uuid: descriptor.service_uuid,
                characteristic_uuid: descriptor.characteristic_uuid,
                descriptor_uuid: descriptor.uuid,
                future: fut.get_state_clone(),
            })
            .await?;
        match fut.await {
            CoreBluetoothReply::ReadResult(chars) => Ok(chars),
            _ => {
                panic!("Shouldn't get anything but read result!");
            }
        }
    }
}

impl From<Uuid> for PeripheralId {
    fn from(uuid: Uuid) -> Self {
        PeripheralId(uuid)
    }
}

impl From<SendError> for Error {
    fn from(_: SendError) -> Self {
        Error::Other("Channel closed".to_string().into())
    }
}
