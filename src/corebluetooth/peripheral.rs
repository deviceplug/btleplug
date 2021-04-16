// btleplug Source Code File
//
// Copyright 2020 Nonpolynomial Labs LLC. All rights reserved.
//
// Licensed under the BSD 3-Clause license. See LICENSE file in the project root
// for full license information.

use super::{
    adapter::uuid_to_bdaddr,
    internal::{
        CBPeripheralEvent, CoreBluetoothMessage, CoreBluetoothReply, CoreBluetoothReplyFuture,
    },
};
use crate::{
    api::{
        self, AddressType, BDAddr, CentralEvent, Characteristic, PeripheralProperties,
        ValueNotification, WriteType,
    },
    common::{adapter_manager::AdapterManager, util},
    Error, Result,
};
use async_trait::async_trait;
use futures::channel::mpsc::{self, Receiver, SendError, Sender, UnboundedSender};
use futures::sink::SinkExt;
use futures::stream::{Stream, StreamExt};
use log::{debug, error, info};
use std::{
    collections::{BTreeSet, HashMap},
    fmt::{self, Debug, Display, Formatter},
    pin::Pin,
    sync::{Arc, Mutex},
};
use tokio::task;
use uuid::Uuid;

/// Implementation of [api::Peripheral](crate::api::Peripheral).
#[derive(Clone)]
pub struct Peripheral {
    notification_senders: Arc<Mutex<Vec<UnboundedSender<ValueNotification>>>>,
    manager: AdapterManager<Self>,
    uuid: Uuid,
    characteristics: Arc<Mutex<BTreeSet<Characteristic>>>,
    properties: Arc<Mutex<PeripheralProperties>>,
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
        let properties = Arc::new(Mutex::from(PeripheralProperties {
            // Rumble required ONLY a BDAddr, not something you can get from
            // MacOS, so we make it up for now. This sucks.
            address: uuid_to_bdaddr(&uuid.to_string()),
            address_type: AddressType::Random,
            local_name,
            tx_power_level: None,
            manufacturer_data: HashMap::new(),
            service_data: HashMap::new(),
            services: Vec::new(),
            discovery_count: 1,
            has_scan_response: true,
        }));
        let notification_senders = Arc::new(Mutex::new(Vec::new()));
        let ns_clone = notification_senders.clone();
        let p_clone = properties.clone();
        let m_clone = manager.clone();
        task::spawn(async move {
            let mut event_receiver = event_receiver;
            loop {
                match event_receiver.next().await {
                    Some(CBPeripheralEvent::Notification(uuid, data)) => {
                        util::send_notification(
                            &ns_clone,
                            &ValueNotification { uuid, value: data },
                        );
                    }
                    Some(CBPeripheralEvent::ManufacturerData(manufacturer_id, data)) => {
                        let mut properties = p_clone.lock().unwrap();
                        properties
                            .manufacturer_data
                            .insert(manufacturer_id, data.clone());
                        m_clone.emit(CentralEvent::ManufacturerDataAdvertisement {
                            address: properties.address,
                            manufacturer_data: properties.manufacturer_data.clone(),
                        });
                    }
                    Some(CBPeripheralEvent::ServiceData(service_data)) => {
                        let mut properties = p_clone.lock().unwrap();
                        properties.service_data.extend(service_data.clone());

                        m_clone.emit(CentralEvent::ServiceDataAdvertisement {
                            address: properties.address,
                            service_data,
                        });
                    }
                    Some(CBPeripheralEvent::Services(services)) => {
                        let mut properties = p_clone.lock().unwrap();
                        properties.services = services.clone();

                        m_clone.emit(CentralEvent::ServicesAdvertisement {
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
        Self {
            properties,
            manager,
            characteristics: Arc::new(Mutex::new(BTreeSet::new())),
            notification_senders,
            uuid,
            message_sender,
        }
    }

    fn emit(&self, event: CentralEvent) {
        debug!("emitted {:?}", event);
        self.manager.emit(event)
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
            .field("uuid", &self.uuid)
            .field("characteristics", &self.characteristics)
            .field("properties", &self.properties)
            .field("message_sender", &self.message_sender)
            .finish()
    }
}

#[async_trait]
impl api::Peripheral for Peripheral {
    fn address(&self) -> BDAddr {
        self.properties.lock().unwrap().address
    }

    async fn properties(&self) -> Result<PeripheralProperties> {
        Ok(self.properties.lock().unwrap().clone())
    }

    fn characteristics(&self) -> BTreeSet<Characteristic> {
        self.characteristics.lock().unwrap().clone()
    }

    async fn is_connected(&self) -> Result<bool> {
        // TODO
        Ok(false)
    }

    async fn connect(&self) -> Result<()> {
        let fut = CoreBluetoothReplyFuture::default();
        self.message_sender
            .to_owned()
            .send(CoreBluetoothMessage::ConnectDevice(
                self.uuid,
                fut.get_state_clone(),
            ))
            .await?;
        match fut.await {
            CoreBluetoothReply::Connected(chars) => {
                *(self.characteristics.lock().unwrap()) = chars;
                self.emit(CentralEvent::DeviceConnected(
                    self.properties.lock().unwrap().address,
                ));
            }
            _ => panic!("Shouldn't get anything but connected!"),
        }
        info!("Device connected!");
        Ok(())
    }

    async fn disconnect(&self) -> Result<()> {
        // TODO
        Ok(())
    }

    async fn discover_characteristics(&self) -> Result<Vec<Characteristic>> {
        let characteristics = self.characteristics.lock().unwrap().clone();
        Ok(characteristics.into_iter().collect())
    }

    async fn write(
        &self,
        characteristic: &Characteristic,
        data: &[u8],
        write_type: WriteType,
    ) -> Result<()> {
        let fut = CoreBluetoothReplyFuture::default();
        self.message_sender
            .to_owned()
            .send(CoreBluetoothMessage::WriteValue(
                self.uuid,
                characteristic.uuid,
                Vec::from(data),
                write_type,
                fut.get_state_clone(),
            ))
            .await?;
        match fut.await {
            CoreBluetoothReply::Ok => {}
            reply => panic!("Unexpected reply: {:?}", reply),
        }
        Ok(())
    }

    async fn read(&self, characteristic: &Characteristic) -> Result<Vec<u8>> {
        let fut = CoreBluetoothReplyFuture::default();
        self.message_sender
            .to_owned()
            .send(CoreBluetoothMessage::ReadValue(
                self.uuid,
                characteristic.uuid,
                fut.get_state_clone(),
            ))
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
        self.message_sender
            .to_owned()
            .send(CoreBluetoothMessage::Subscribe(
                self.uuid,
                characteristic.uuid,
                fut.get_state_clone(),
            ))
            .await?;
        match fut.await {
            CoreBluetoothReply::Ok => info!("subscribed!"),
            _ => panic!("Didn't subscribe!"),
        }
        Ok(())
    }

    async fn unsubscribe(&self, characteristic: &Characteristic) -> Result<()> {
        let fut = CoreBluetoothReplyFuture::default();
        self.message_sender
            .to_owned()
            .send(CoreBluetoothMessage::Unsubscribe(
                self.uuid,
                characteristic.uuid,
                fut.get_state_clone(),
            ))
            .await?;
        match fut.await {
            CoreBluetoothReply::Ok => {}
            _ => panic!("Didn't unsubscribe!"),
        }
        Ok(())
    }

    async fn notifications(&self) -> Result<Pin<Box<dyn Stream<Item = ValueNotification>>>> {
        let (sender, receiver) = mpsc::unbounded();
        let mut senders = self.notification_senders.lock().unwrap();
        senders.push(sender);
        Ok(Box::pin(receiver))
    }
}

impl From<SendError> for Error {
    fn from(_: SendError) -> Self {
        Error::Other("Channel closed".to_string())
    }
}
