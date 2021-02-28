use async_trait::async_trait;
use futures::channel::mpsc;
use futures::sink::SinkExt;
use futures::stream::Stream;
use log::{info, warn};
use std::collections::BTreeSet;
use std::pin::Pin;

use super::super::{
    internal::{CoreBluetoothMessage, CoreBluetoothReply, CoreBluetoothReplyFuture},
    peripheral::Peripheral,
};
use crate::api::{
    async_api, BDAddr, CentralEvent, Characteristic, Peripheral as _, PeripheralProperties,
    ValueNotification, WriteType,
};
use crate::Result;

#[async_trait]
impl async_api::Peripheral for Peripheral {
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
            _ => panic!("Didn't subscribe!"),
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
        self.on_notification(Box::new(move |notification| {
            // TODO: If the receiver is dropped then remove this from the notification_handlers
            // list.
            if let Err(e) = sender.unbounded_send(notification) {
                warn!("Error sending to channel for notification stream: {}", e);
            }
        }));
        Ok(Box::pin(receiver))
    }
}
