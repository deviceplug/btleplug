use super::internal::{run_corebluetooth_thread, CoreBluetoothEvent, CoreBluetoothMessage};
use super::peripheral::{Peripheral, PeripheralId};
use crate::api::{Central, CentralEvent, ScanFilter};
use crate::common::adapter_manager::AdapterManager;
use crate::{Error, Result};
use futures::channel::mpsc::{self, Sender};
use futures::sink::SinkExt;
use futures::stream::{Stream, StreamExt};
use log::*;
use std::pin::Pin;
use std::sync::Arc;
use tokio::task;

/// Implementation of [api::Central](crate::api::Central).
#[derive(Clone, Debug)]
pub struct Adapter {
    manager: Arc<AdapterManager<Peripheral>>,
    sender: Sender<CoreBluetoothMessage>,
}

impl Adapter {
    pub(crate) async fn new() -> Result<Self> {
        let (sender, mut receiver) = mpsc::channel(256);
        let adapter_sender = run_corebluetooth_thread(sender)?;
        // Since init currently blocked until the state update, we know the
        // receiver is dropped after that. We can pick it up here and make it
        // part of our event loop to update our peripherals.
        debug!("Waiting on adapter connect");
        if !matches!(
            receiver.next().await,
            Some(CoreBluetoothEvent::AdapterConnected)
        ) {
            return Err(Error::Other(
                "Adapter failed to connect.".to_string().into(),
            ));
        }
        debug!("Adapter connected");
        let manager = Arc::new(AdapterManager::default());

        let manager_clone = manager.clone();
        let adapter_sender_clone = adapter_sender.clone();
        task::spawn(async move {
            while let Some(msg) = receiver.next().await {
                match msg {
                    CoreBluetoothEvent::DeviceDiscovered {
                        uuid,
                        name,
                        event_receiver,
                    } => {
                        manager_clone.add_peripheral(Peripheral::new(
                            uuid,
                            name,
                            Arc::downgrade(&manager_clone),
                            event_receiver,
                            adapter_sender_clone.clone(),
                        ));
                        manager_clone.emit(CentralEvent::DeviceDiscovered(uuid.into()));
                    }
                    CoreBluetoothEvent::DeviceUpdated { uuid, name } => {
                        let id = uuid.into();
                        if let Some(entry) = manager_clone.peripheral_mut(&id) {
                            entry.value().update_name(&name);
                            manager_clone.emit(CentralEvent::DeviceUpdated(id));
                        }
                    }
                    CoreBluetoothEvent::DeviceDisconnected { uuid } => {
                        manager_clone.emit(CentralEvent::DeviceDisconnected(uuid.into()));
                    }
                    _ => {}
                }
            }
        });

        Ok(Adapter {
            manager,
            sender: adapter_sender,
        })
    }
}

impl Central for Adapter {
    type Peripheral = Peripheral;

    async fn events(&self) -> Result<Pin<Box<dyn Stream<Item = CentralEvent> + Send>>> {
        Ok(self.manager.event_stream())
    }

    async fn start_scan(&self, filter: ScanFilter) -> Result<()> {
        self.sender
            .to_owned()
            .send(CoreBluetoothMessage::StartScanning { filter })
            .await?;
        Ok(())
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

    async fn peripheral(&self, id: &PeripheralId) -> Result<Peripheral> {
        self.manager.peripheral(id).ok_or(Error::DeviceNotFound)
    }

    async fn add_peripheral(&self, _address: &PeripheralId) -> Result<Peripheral> {
        Err(Error::NotSupported(
            "Can't add a Peripheral from a PeripheralId".to_string(),
        ))
    }

    async fn adapter_info(&self) -> Result<String> {
        // TODO: Get information about the adapter.
        Ok("CoreBluetooth".to_string())
    }
}
