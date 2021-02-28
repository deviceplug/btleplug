use super::super::{adapter::Adapter, internal::CoreBluetoothMessage, peripheral::Peripheral};
use crate::api::async_api::Central;
use crate::api::{BDAddr, CentralEvent};
use crate::{Error, Result};
use async_trait::async_trait;
use futures::sink::SinkExt;
use futures::stream::Stream;
use std::pin::Pin;

#[async_trait]
impl Central for Adapter {
    type Peripheral = Peripheral;

    async fn events(&self) -> Result<Pin<Box<dyn Stream<Item = CentralEvent>>>> {
        Ok(self.manager.event_stream())
    }

    async fn start_scan(&self) -> Result<()> {
        self.sender
            .to_owned()
            .send(CoreBluetoothMessage::StartScanning)
            .await?;
        Ok(())
    }

    async fn active(&self, _enabled: bool) {
        todo!()
    }

    async fn filter_duplicates(&self, _enabled: bool) {
        todo!()
    }

    async fn stop_scan(&self) -> Result<()> {
        self.sender
            .to_owned()
            .send(CoreBluetoothMessage::StopScanning)
            .await?;
        Ok(())
    }

    async fn peripherals(&self) -> Result<Vec<Peripheral>> {
        Ok(self.manager.peripherals())
    }

    async fn peripheral(&self, address: BDAddr) -> Result<Peripheral> {
        self.manager
            .peripheral(address)
            .ok_or(Error::DeviceNotFound)
    }
}
