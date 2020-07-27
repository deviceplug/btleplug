extern crate btleplug;
extern crate rand;

use btleplug::api::{Central, CentralEvent};
#[cfg(target_os = "linux")]
use btleplug::bluez::{adapter::ConnectedAdapter, manager::Manager};
#[cfg(target_os = "macos")]
use btleplug::corebluetooth::{adapter::Adapter, manager::Manager};
#[cfg(target_os = "windows")]
use btleplug::winrtble::{adapter::Adapter, manager::Manager};

// adapter retrieval works differently depending on your platform right now.
// API needs to be aligned.

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn get_central(manager: &Manager) -> Adapter {
    let adapters = manager.adapters().unwrap();
    adapters.into_iter().nth(0).unwrap()
}

#[cfg(target_os = "linux")]
fn get_central(manager: &Manager) -> ConnectedAdapter {
    let adapters = manager.adapters().unwrap();
    let adapter = adapters.into_iter().nth(0).unwrap();
    adapter.connect().unwrap()
}

pub fn main() {
    let manager = Manager::new().unwrap();

    // get the first bluetooth adapter
    // connect to the adapter
    let central = get_central(&manager);

    // Each adapter can only have one event receiver. We fetch it via
    // event_receiver(), which will return an option. The first time the getter
    // is called, it will return Some(Receiver<CentralEvent>). After that, it
    // will only return None.
    //
    // While this API is awkward, is is done as not to disrupt the adapter
    // retrieval system in btleplug v0.x while still allowing us to use event
    // streams/channels instead of callbacks. In btleplug v1.x, we'll retrieve
    // channels as part of adapter construction.
    let event_receiver = central.event_receiver().unwrap();

    // start scanning for devices
    central.start_scan().unwrap();

    // Print based on whatever the event receiver outputs. Note that the event
    // receiver blocks, so in a real program, this should be run in its own
    // thread (not task, as this library does not yet use async channels).
    while let Ok(event) = event_receiver.recv() {
        match event {
            CentralEvent::DeviceDiscovered(bd_addr) => {
                println!("DeviceDiscovered: {:?}", bd_addr);
            }
            CentralEvent::DeviceConnected(bd_addr) => {
                println!("DeviceConnected: {:?}", bd_addr);
            }
            CentralEvent::DeviceDisconnected(bd_addr) => {
                println!("DeviceDisconnected: {:?}", bd_addr);
            }
            _ => {}
        }
    }
}
