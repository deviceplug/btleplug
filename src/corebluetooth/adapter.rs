use crate::api::{Central, CentralEvent, EventHandler, BDAddr};
use crate::Result;
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use std::convert::TryInto;
use super::ble::{
    adapter::BluetoothAdapter,
    delegate::{
        bm,
        DelegateMessage,
    },
};
use super::peripheral::Peripheral;
use async_std::{
    task,
    prelude::StreamExt,
};

#[derive(Clone)]
pub struct Adapter {
    adapter: Arc<BluetoothAdapter>,
    event_handlers: Arc<Mutex<Vec<EventHandler>>>,
    peripherals: Arc<Mutex<HashMap<BDAddr, Peripheral>>>,
}

pub fn uuid_to_bdaddr(uuid: &String) -> BDAddr {
    BDAddr {
        address: uuid.as_bytes()[0..6].try_into().unwrap()
    }
}

impl Adapter {
    pub fn new() -> Self {

        let adapter = Arc::new(BluetoothAdapter::init().unwrap());
        let adapter_clone = adapter.clone();
        // Since init currently blocked until the state update, we know the
        // receiver is dropped after that. We can pick it up here and make it
        // part of our event loop to update our peripherals.

        let event_handlers = Arc::new(Mutex::new(Vec::new()));
        let peripherals = Arc::new(Mutex::new(HashMap::new()));
        let handler_clone = event_handlers.clone();
        let peripherals_clone = peripherals.clone();
        let mut recv = bm::delegate_receiver_clone(adapter.delegate);

        task::spawn(async move{
            loop {
                // TODO We should probably have the sender throw out None on
                // Drop to clean this up?
                match recv.next().await.unwrap() {
                    DelegateMessage::DiscoveredPeripheral(uuid, name) => {
                        // TODO Gotta change uuid into a BDAddr for now. Expand
                        // library identifier type. :(
                        let id = uuid_to_bdaddr(&uuid);
                        let mut p = peripherals_clone.lock().unwrap();
                        p.insert(id, Peripheral::new(adapter_clone.clone(), &uuid));
                    },
                    _ => {}
                }
            }
        });

        Adapter {
            event_handlers,
            adapter,
            peripherals,
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
        self.adapter.start_discovery();
        Ok(())
    }

    fn stop_scan(&self) -> Result<()> {
        self.adapter.stop_discovery();
        Ok(())
    }

    fn peripherals(&self) -> Vec<Peripheral> {
        vec!()
    }

    fn peripheral(&self, address: BDAddr) -> Option<Peripheral> {
        None
    }

    fn active(&self, enabled: bool) {
    }

    fn filter_duplicates(&self, enabled: bool) {
    }
}
