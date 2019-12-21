use api::Central;
use winrtble::peripheral::Peripheral;
use api::EventHandler;
use api::BDAddr;
use ::Result;
use winrtble::ble::watcher::BLEWatcher;
use winrtble::utils;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct Adapter {
    watcher: Arc<Mutex<BLEWatcher>>,
    peripherals: Arc<Mutex<HashMap<BDAddr, Peripheral>>>,
    event_handlers: Arc<Mutex<Vec<EventHandler>>>,
}

impl Adapter {
    pub fn new() -> Self {
        let watcher = Arc::new(Mutex::new(BLEWatcher::new()));
        let peripherals = Arc::new(Mutex::new(HashMap::new()));
        let event_handlers = Arc::new(Mutex::new(Vec::new()));
        Adapter { watcher, peripherals, event_handlers }
    }
}

impl Central<Peripheral> for Adapter {
    fn on_event(&self, handler: EventHandler) {
        let list = self.event_handlers.clone();
        list.lock().unwrap().push(handler);
    }

    fn start_scan(&self) -> Result<()> {
        let peripherals = self.peripherals.clone();
        let watcher = self.watcher.lock().unwrap();
        watcher.start(Box::new(move |args| {
            let bluetooth_address = args.get_bluetooth_address().unwrap();
            let address = utils::to_addr(bluetooth_address);
            let mut peripherals = peripherals.lock().unwrap();
            let peripheral = peripherals.entry(address).or_insert_with(|| {
                Peripheral::new(address)
            });
            peripheral.update_properties(&args);
        }))
    }

    fn stop_scan(&self) -> Result<()> {
        let watcher = self.watcher.lock().unwrap();
        watcher.stop().unwrap();
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
}
