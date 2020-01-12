use api::{Central, CentralEvent, EventHandler, BDAddr};
use ::Result;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use super::ble::adapter::BluetoothAdapter;
use super::peripheral::Peripheral;

#[derive(Clone)]
pub struct Adapter {
    adapter: Arc<Mutex<BluetoothAdapter>>,
    event_handlers: Arc<Mutex<Vec<EventHandler>>>,
}

impl Adapter {
    pub fn new() -> Self {
        Adapter {
            event_handlers: Arc::new(Mutex::new(Vec::new())),
            adapter: Arc::new(Mutex::new(BluetoothAdapter::init().unwrap()))
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
        self.adapter.lock().unwrap().start_discovery();
        Ok(())
    }

    fn stop_scan(&self) -> Result<()> {
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
