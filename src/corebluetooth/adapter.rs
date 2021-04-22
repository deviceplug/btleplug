use super::internal::{run_corebluetooth_thread, CoreBluetoothEvent, CoreBluetoothMessage};
use super::peripheral::Peripheral;
use crate::api::{BDAddr, Central, CentralEvent};
use crate::common::adapter_manager::AdapterManager;
use crate::{Error, Result};
use async_trait::async_trait;
use futures::channel::mpsc::{self, Sender};
use futures::sink::SinkExt;
use futures::stream::{Stream, StreamExt};
use log::info;
use std::convert::{TryFrom, TryInto};
use std::pin::Pin;
use tokio::task;

/// Implementation of [api::Central](crate::api::Central).
#[derive(Clone, Debug)]
pub struct Adapter {
    manager: AdapterManager<Peripheral>,
    sender: Sender<CoreBluetoothMessage>,
}

pub(crate) fn uuid_to_bdaddr(uuid: &str) -> BDAddr {
    let b: [u8; 6] = uuid.as_bytes()[0..6].try_into().unwrap();
    BDAddr::try_from(b).unwrap()
}

impl Adapter {
    pub(crate) async fn new() -> Result<Self> {
        let (sender, mut receiver) = mpsc::channel(256);
        let adapter_sender = run_corebluetooth_thread(sender);
        // Since init currently blocked until the state update, we know the
        // receiver is dropped after that. We can pick it up here and make it
        // part of our event loop to update our peripherals.
        info!("Waiting on adapter connect");
        if !matches!(
            receiver.next().await,
            Some(CoreBluetoothEvent::AdapterConnected)
        ) {
            return Err(Error::Other("Adapter failed to connect.".to_string()));
        }
        info!("Adapter connected");
        let manager = AdapterManager::default();

        let manager_clone = manager.clone();
        let adapter_sender_clone = adapter_sender.clone();
        task::spawn(async move {
            while let Some(msg) = receiver.next().await {
                match msg {
                    CoreBluetoothEvent::DeviceDiscovered(uuid, name, event_receiver) => {
                        // TODO Gotta change uuid into a BDAddr for now. Expand
                        // library identifier type. :(
                        let id = uuid_to_bdaddr(&uuid.to_string());
                        manager_clone.add_peripheral(
                            id,
                            Peripheral::new(
                                uuid,
                                name,
                                manager_clone.clone(),
                                event_receiver,
                                adapter_sender_clone.clone(),
                            ),
                        );
                        manager_clone.emit(CentralEvent::DeviceDiscovered(id));
                    }
                    /*
                    CoreBluetoothEvent::DeviceUpdated(uuid, name) => {
                        let id = uuid_to_bdaddr(&uuid.to_string());
                        emit(CentralEvent::DeviceUpdated(id));
                    },
                    */
                    CoreBluetoothEvent::DeviceLost(uuid) => {
                        let id = uuid_to_bdaddr(&uuid.to_string());
                        manager_clone.emit(CentralEvent::DeviceDisconnected(id));
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
