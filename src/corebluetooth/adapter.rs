use crate::api::{Central, CentralEvent, EventHandler, BDAddr};
use crate::Result;
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use std::convert::TryInto;
use super::peripheral::Peripheral;
use super::internal::{run_corebluetooth_thread, CoreBluetoothMessage, CoreBluetoothEvent};
use async_std::{
    task,
    prelude::StreamExt,
    sync::{channel, Sender},
};

#[derive(Clone)]
pub struct Adapter {
    event_handlers: Arc<Mutex<Vec<EventHandler>>>,
    peripherals: Arc<Mutex<HashMap<BDAddr, Peripheral>>>,
    sender: Sender<CoreBluetoothMessage>,
}

pub fn uuid_to_bdaddr(uuid: &String) -> BDAddr {
    BDAddr {
        address: uuid.as_bytes()[0..6].try_into().unwrap()
    }
}

impl Adapter {
    pub fn new() -> Self {
        let (sender, mut receiver) = channel(256);
        let adapter_sender = run_corebluetooth_thread(sender);
        // Since init currently blocked until the state update, we know the
        // receiver is dropped after that. We can pick it up here and make it
        // part of our event loop to update our peripherals.
        info!("Waiting on adapter connect");
        task::block_on(async {
            receiver.recv().await.unwrap()
        });
        info!("Waiting on adapter connected");
        let event_handlers = Arc::new(Mutex::new(Vec::<EventHandler>::new()));
        let peripherals = Arc::new(Mutex::new(HashMap::new()));
        let handler_clone = event_handlers.clone();
        let peripherals_clone = peripherals.clone();
        let adapter_sender_clone = adapter_sender.clone();
        task::spawn(async move{
            let emit = |event: CentralEvent| {
                let vec = handler_clone.lock().unwrap();
                for handler in (*vec).iter() {
                    handler(event.clone());
                }
            };

            loop {
                // TODO We should probably have the sender throw out None on
                // Drop to clean this up?
                let msg = receiver.next().await;
                if msg.is_none() {
                    info!("Stopping CoreBluetooth Adapter event loop.");
                    break;
                } else {
                    match msg.unwrap() {
                        CoreBluetoothEvent::DeviceDiscovered(uuid, name, event_receiver) => {
                            // TODO Gotta change uuid into a BDAddr for now. Expand
                            // library identifier type. :(
                            let id = uuid_to_bdaddr(&uuid.to_string());
                            let mut p = peripherals_clone.lock().unwrap();
                            p.insert(id, Peripheral::new(uuid, name, handler_clone.clone(), event_receiver, adapter_sender_clone.clone()));
                            emit(CentralEvent::DeviceDiscovered(id));
                        },
                        _ => {}
                    }
                }
            }
        });

        Adapter {
            event_handlers,
            peripherals,
            sender: adapter_sender
        }
    }

    pub fn emit(&self, event: CentralEvent) {
        debug!("emitted {:?}", event);
        let handlers = self.event_handlers.clone();
        let vec = handlers.lock().unwrap();
        for handler in (*vec).iter() {
            handler(event.clone());
        }
    }
}

impl Central<Peripheral> for Adapter {
    fn on_event(&self, handler: EventHandler) {
        let list = self.event_handlers.clone();
        list.lock().unwrap().push(handler);
    }

    fn start_scan(&self) -> Result<()> {
        info!("Starting CoreBluetooth Scan");
        task::block_on(async {
            self.sender.send(CoreBluetoothMessage::StartScanning).await;
        });
        Ok(())
    }

    fn stop_scan(&self) -> Result<()> {
        info!("Stopping CoreBluetooth Scan");
        task::block_on(async {
            self.sender.send(CoreBluetoothMessage::StopScanning).await;
        });
        Ok(())
    }

    fn peripherals(&self) -> Vec<Peripheral> {
        let l = self.peripherals.lock().unwrap();
        l.values().cloned().collect()
    }

    fn peripheral(&self, address: BDAddr) -> Option<Peripheral> {
        let l = self.peripherals.lock().unwrap();
        l.get(&address).cloned()
    }

    fn active(&self, enabled: bool) {
    }

    fn filter_duplicates(&self, enabled: bool) {
    }
}
