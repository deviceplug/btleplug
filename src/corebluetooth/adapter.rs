use super::internal::{run_corebluetooth_thread, CoreBluetoothEvent, CoreBluetoothMessage};
use super::peripheral::Peripheral;
use crate::api::{AdapterManager, BDAddr, Central, CentralEvent};
use crate::Result;
use async_std::task;
use futures::channel::mpsc::{self, Sender};
use futures::sink::SinkExt;
use futures::stream::StreamExt;
use log::info;
use std::convert::TryInto;
use std::sync::mpsc::Receiver;

#[derive(Clone, Debug)]
pub struct Adapter {
    manager: AdapterManager<Peripheral>,
    sender: Sender<CoreBluetoothMessage>,
}

pub fn uuid_to_bdaddr(uuid: &String) -> BDAddr {
    BDAddr {
        address: uuid.as_bytes()[0..6].try_into().unwrap(),
    }
}

impl Adapter {
    pub fn new() -> Self {
        let (sender, mut receiver) = mpsc::channel(256);
        let adapter_sender = run_corebluetooth_thread(sender);
        // Since init currently blocked until the state update, we know the
        // receiver is dropped after that. We can pick it up here and make it
        // part of our event loop to update our peripherals.
        info!("Waiting on adapter connect");
        task::block_on(async { receiver.next().await.unwrap() });
        info!("Waiting on adapter connected");
        let adapter_sender_clone = adapter_sender.clone();
        let manager = AdapterManager::new();

        let manager_clone = manager.clone();
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
                    CoreBluetoothEvent::DeviceUpdated(uuid, name) => {
                        let id = uuid_to_bdaddr(&uuid.to_string());
                        let peripheral = manager_clone.peripheral(id).unwrap();
                        {
                            let mut properties = peripheral.properties.lock().unwrap();
                            properties.local_name = Some(name);
                        }
                        manager_clone.update_peripheral(id, peripheral);
                        manager_clone.emit(CentralEvent::DeviceUpdated(id));
                    }
                    CoreBluetoothEvent::DeviceLost(uuid) => {
                        let id = uuid_to_bdaddr(&uuid.to_string());
                        manager_clone.emit(CentralEvent::DeviceDisconnected(id));
                    }
                    _ => {}
                }
            }
        });

        Adapter {
            manager,
            sender: adapter_sender,
        }
    }

    pub fn emit(&self, event: CentralEvent) {
        self.manager.emit(event)
    }
}

impl Central<Peripheral> for Adapter {
    fn event_receiver(&self) -> Option<Receiver<CentralEvent>> {
        self.manager.event_receiver()
    }

    fn start_scan(&self) -> Result<()> {
        info!("Starting CoreBluetooth Scan");
        task::block_on(async {
            let mut sender = self.sender.clone();
            sender.send(CoreBluetoothMessage::StartScanning).await?;
            Ok(())
        })
    }

    fn stop_scan(&self) -> Result<()> {
        info!("Stopping CoreBluetooth Scan");
        task::block_on(async {
            let mut sender = self.sender.clone();
            sender.send(CoreBluetoothMessage::StopScanning).await?;
            Ok(())
        })
    }

    fn peripherals(&self) -> Vec<Peripheral> {
        self.manager.peripherals()
    }

    fn peripheral(&self, address: BDAddr) -> Option<Peripheral> {
        self.manager.peripheral(address)
    }

    fn active(&self, _enabled: bool) {}

    fn filter_duplicates(&self, _enabled: bool) {}
}
