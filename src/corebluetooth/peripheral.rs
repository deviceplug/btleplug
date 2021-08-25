// btleplug Source Code File
//
// Copyright 2020 Nonpolynomial Labs LLC. All rights reserved.
//
// Licensed under the BSD 3-Clause license. See LICENSE file in the project root
// for full license information.

use super::{
    adapter::uuid_to_bdaddr,
    framework::cb::CBPeripheralState,
    internal::{
        CBPeripheralEvent, CoreBluetoothMessage, CoreBluetoothReply, CoreBluetoothReplyFuture,
    },
};
use crate::{
    api::{
        self, BDAddr, CentralEvent, CharPropFlags, Characteristic, PeripheralProperties, Service,
        ValueNotification, WriteType,
    },
    common::{adapter_manager::AdapterManager, util::notifications_stream_from_broadcast_receiver},
    Error, Result,
};
use async_trait::async_trait;
use futures::channel::mpsc::{Receiver, SendError, Sender};
use futures::sink::SinkExt;
use futures::stream::{Stream, StreamExt};
use log::*;
use std::{
    collections::{BTreeSet, HashMap},
    fmt::{self, Debug, Display, Formatter},
    pin::Pin,
    sync::{Arc, Mutex},
};
use tokio::sync::broadcast;
use tokio::task;
use uuid::Uuid;

/// Implementation of [api::Peripheral](crate::api::Peripheral).
#[derive(Clone)]
pub struct Peripheral {
    shared: Arc<Shared>,
}

struct Shared {
    notifications_channel: broadcast::Sender<ValueNotification>,
    manager: AdapterManager<Peripheral>,
    uuid: Uuid,
    services: Mutex<BTreeSet<Service>>,
    properties: Mutex<PeripheralProperties>,
    message_sender: Sender<CoreBluetoothMessage>,
    // We're not actually holding a peripheral object here, that's held out in
    // the objc thread. We'll just communicate with it through our
    // receiver/sender pair.
}

impl Peripheral {
    // This calls tokio::task::spawn, so it must be called from the context of a Tokio Runtime.
    pub(crate) fn new(
        uuid: Uuid,
        local_name: Option<String>,
        manager: AdapterManager<Self>,
        event_receiver: Receiver<CBPeripheralEvent>,
        message_sender: Sender<CoreBluetoothMessage>,
    ) -> Self {
        // Since we're building the object, we have an active advertisement.
        // Build properties now.
        let properties = Mutex::from(PeripheralProperties {
            // Rumble required ONLY a BDAddr, not something you can get from
            // MacOS, so we make it up for now. This sucks.
            address: uuid_to_bdaddr(&uuid.to_string()),
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
                    Some(CBPeripheralEvent::ManufacturerData(manufacturer_id, data)) => {
                        let mut properties = shared.properties.lock().unwrap();
                        properties
                            .manufacturer_data
                            .insert(manufacturer_id, data.clone());
                        shared
                            .manager
                            .emit(CentralEvent::ManufacturerDataAdvertisement {
                                address: properties.address,
                                manufacturer_data: properties.manufacturer_data.clone(),
                            });
                    }
                    Some(CBPeripheralEvent::ServiceData(service_data)) => {
                        let mut properties = shared.properties.lock().unwrap();
                        properties.service_data.extend(service_data.clone());

                        shared.manager.emit(CentralEvent::ServiceDataAdvertisement {
                            address: properties.address,
                            service_data,
                        });
                    }
                    Some(CBPeripheralEvent::Services(services)) => {
                        let mut properties = shared.properties.lock().unwrap();
                        properties.services = services.clone();

                        shared.manager.emit(CentralEvent::ServicesAdvertisement {
                            address: properties.address,
                            services,
                        });
                    }
                    Some(CBPeripheralEvent::Disconnected) => (),
                    None => {
                        error!("Event receiver died, breaking out of corebluetooth device loop.");
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
    fn address(&self) -> BDAddr {
        // TODO: look at moving/copying address out of properties so we don't have to
        // take a lock here! (the address for the peripheral won't ever change)
        self.shared.properties.lock().unwrap().address
    }

    async fn properties(&self) -> Result<Option<PeripheralProperties>> {
        Ok(Some(self.shared.properties.lock().unwrap().clone()))
    }

    fn services(&self) -> BTreeSet<Service> {
        self.shared.services.lock().unwrap().clone()
    }

    fn characteristics(&self) -> BTreeSet<Characteristic> {
        self.shared
            .services
            .lock()
            .unwrap()
            .iter()
            .flat_map(|service| service.characteristics.clone().into_iter())
            .collect()
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
                self.shared.manager.emit(CentralEvent::DeviceConnected(
                    // TODO: look at moving/copying address out of properties so we don't have to
                    // take a lock here! (the address for the peripheral won't ever change)
                    self.shared.properties.lock().unwrap().address,
                ));
            }
            _ => panic!("Shouldn't get anything but connected!"),
        }
        trace!("Device connected!");
        Ok(())
    }

    async fn disconnect(&self) -> Result<()> {
        // TODO
        Ok(())
    }

    async fn discover_characteristics(&self) -> Result<Vec<Characteristic>> {
        let characteristics = self.characteristics();
        Ok(characteristics.into_iter().collect())
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
}

impl From<SendError> for Error {
    fn from(_: SendError) -> Self {
        Error::Other("Channel closed".to_string().into())
    }
}
