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
        AdapterManager, AddressType, BDAddr, CentralEvent, Characteristic, NotificationHandler,
        Peripheral as ApiPeripheral, PeripheralProperties, ValueNotification, WriteType,
    },
    common::util,
    Error, Result,
};
use async_std::task;
use futures::channel::mpsc::{Receiver, SendError, Sender};
use futures::sink::SinkExt;
use futures::stream::StreamExt;
use log::{debug, error, info};
use std::{
    collections::{BTreeSet, HashMap},
    fmt::{self, Debug, Display, Formatter},
    iter::FromIterator,
    sync::{Arc, Mutex},
};
use uuid::Uuid;

#[derive(Clone)]
pub struct Peripheral {
    notification_handlers: Arc<Mutex<Vec<NotificationHandler>>>,
    manager: AdapterManager<Self>,
    uuid: Uuid,
    characteristics: Arc<Mutex<BTreeSet<Characteristic>>>,
    pub(crate) properties: Arc<Mutex<PeripheralProperties>>,
    message_sender: Sender<CoreBluetoothMessage>,
    // We're not actually holding a peripheral object here, that's held out in
    // the objc thread. We'll just communicate with it through our
    // receiver/sender pair.
}

impl Peripheral {
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
            local_name: local_name,
            tx_power_level: None,
            manufacturer_data: HashMap::new(),
            service_data: HashMap::new(),
            services: Vec::new(),
            discovery_count: 1,
            has_scan_response: true,
        }));
        let notification_handlers = Arc::new(Mutex::new(Vec::<NotificationHandler>::new()));
        let nh_clone = notification_handlers.clone();
        let p_clone = properties.clone();
        let m_clone = manager.clone();
        task::spawn(async move {
            let mut event_receiver = event_receiver;
            loop {
                match event_receiver.next().await {
                    Some(CBPeripheralEvent::Notification(uuid, data)) => {
                        util::invoke_handlers(
                            &nh_clone,
                            &ValueNotification {
                                uuid,
                                handle: None,
                                value: data,
                            },
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
            notification_handlers,
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

impl ApiPeripheral for Peripheral {
    /// Returns the address of the peripheral.
    fn address(&self) -> BDAddr {
        self.properties.lock().unwrap().address
    }

    /// Returns the set of properties associated with the peripheral. These may be updated over time
    /// as additional advertising reports are received.
    fn properties(&self) -> PeripheralProperties {
        self.properties.lock().unwrap().clone()
    }

    /// The set of characteristics we've discovered for this device. This will be empty until
    /// `discover_characteristics` is called.
    fn characteristics(&self) -> BTreeSet<Characteristic> {
        self.characteristics.lock().unwrap().clone()
    }

    /// Returns true iff we are currently connected to the device.
    fn is_connected(&self) -> bool {
        false
    }

    /// Creates a connection to the device. This is a synchronous operation; if this method returns
    /// Ok there has been successful connection. Note that peripherals allow only one connection at
    /// a time. Operations that attempt to communicate with a device will fail until it is connected.
    fn connect(&self) -> Result<()> {
        info!("Trying device connect!");
        task::block_on(async {
            let mut message_sender = self.message_sender.clone();
            let fut = CoreBluetoothReplyFuture::default();
            message_sender
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
        })
    }

    /// Terminates a connection to the device. This is a synchronous operation.
    fn disconnect(&self) -> Result<()> {
        Ok(())
    }

    /// Discovers all characteristics for the device. This is a synchronous operation.
    fn discover_characteristics(&self) -> Result<Vec<Characteristic>> {
        let chrs = self.characteristics.lock().unwrap().clone();
        let v = Vec::from_iter(chrs.into_iter());
        Ok(v)
    }

    /// Write some data to the characteristic. Returns an error if the write couldn't be send or (in
    /// the case of a write-with-response) if the device returns an error.
    fn write(
        &self,
        characteristic: &Characteristic,
        data: &[u8],
        write_type: WriteType,
    ) -> Result<()> {
        task::block_on(async {
            let mut message_sender = self.message_sender.clone();
            let fut = CoreBluetoothReplyFuture::default();
            message_sender
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
        })
    }

    /// Sends a read-by-type request to device for the range of handles covered by the
    /// characteristic and for the specified declaration UUID. See
    /// [here](https://www.bluetooth.com/specifications/gatt/declarations) for valid UUIDs.
    /// Synchronously returns either an error or the device response.
    fn read_by_type(&self, _characteristic: &Characteristic, _uuid: Uuid) -> Result<Vec<u8>> {
        Err(Error::NotSupported("read_by_type".into()))
    }

    /// Enables either notify or indicate (depending on support) for the specified characteristic.
    /// This is a synchronous call.
    fn subscribe(&self, characteristic: &Characteristic) -> Result<()> {
        info!("Trying to subscribe!");
        task::block_on(async {
            let mut message_sender = self.message_sender.clone();
            let fut = CoreBluetoothReplyFuture::default();
            message_sender
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
        })
    }

    /// Disables either notify or indicate (depending on support) for the specified characteristic.
    /// This is a synchronous call.
    fn unsubscribe(&self, characteristic: &Characteristic) -> Result<()> {
        info!("Trying to unsubscribe!");
        task::block_on(async {
            let mut message_sender = self.message_sender.clone();
            let fut = CoreBluetoothReplyFuture::default();
            message_sender
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
        })
    }

    /// Registers a handler that will be called when value notification messages are received from
    /// the device. This method should only be used after a connection has been established. Note
    /// that the handler will be called in a common thread, so it should not block.
    fn on_notification(&self, handler: NotificationHandler) {
        let mut list = self.notification_handlers.lock().unwrap();
        list.push(handler);
    }

    fn read(&self, characteristic: &Characteristic) -> Result<Vec<u8>> {
        info!("Trying read!");
        task::block_on(async {
            let mut message_sender = self.message_sender.clone();
            let fut = CoreBluetoothReplyFuture::default();
            message_sender
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
        })
    }
}

impl From<SendError> for Error {
    fn from(_: SendError) -> Self {
        Error::Other("Channel closed".to_string())
    }
}
