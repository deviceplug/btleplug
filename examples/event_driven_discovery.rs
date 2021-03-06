extern crate btleplug;
extern crate rand;

use btleplug::api::{bleuuid::BleUuid, Central, CentralEvent};
use btleplug::{Adapter, Manager};

fn get_central(manager: &Manager) -> Adapter {
    let adapters = manager.adapters().unwrap();
    adapters.into_iter().nth(0).unwrap()
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
            CentralEvent::ManufacturerDataAdvertisement {
                address,
                manufacturer_data,
            } => {
                println!(
                    "ManufacturerDataAdvertisement: {:?}, {:?}",
                    address, manufacturer_data
                );
            }
            CentralEvent::ServiceDataAdvertisement {
                address,
                service_data,
            } => {
                println!(
                    "ServiceDataAdvertisement: {:?}, {:?}",
                    address, service_data
                );
            }
            CentralEvent::ServicesAdvertisement { address, services } => {
                let services: Vec<String> =
                    services.into_iter().map(|s| s.to_short_string()).collect();
                println!("ServicesAdvertisement: {:?}, {:?}", address, services);
            }
            _ => {}
        }
    }
}
