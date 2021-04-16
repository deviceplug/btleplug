use btleplug::api::{bleuuid::BleUuid, Central, CentralEvent, Manager as _};
use btleplug::platform::{Adapter, Manager};
use futures::stream::StreamExt;
use std::error::Error;

async fn get_central(manager: &Manager) -> Adapter {
    let adapters = manager.adapters().await.unwrap();
    adapters.into_iter().nth(0).unwrap()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    pretty_env_logger::init();

    let manager = Manager::new().await?;

    // get the first bluetooth adapter
    // connect to the adapter
    let central = get_central(&manager).await;

    // Each adapter can only have one event receiver. We fetch it via
    // event_receiver(), which will return an option. The first time the getter
    // is called, it will return Some(Receiver<CentralEvent>). After that, it
    // will only return None.
    //
    // While this API is awkward, is is done as not to disrupt the adapter
    // retrieval system in btleplug v0.x while still allowing us to use event
    // streams/channels instead of callbacks. In btleplug v1.x, we'll retrieve
    // channels as part of adapter construction.
    let mut events = central.events().await?;

    // start scanning for devices
    central.start_scan().await?;

    // Print based on whatever the event receiver outputs. Note that the event
    // receiver blocks, so in a real program, this should be run in its own
    // thread (not task, as this library does not yet use async channels).
    while let Some(event) = events.next().await {
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
    Ok(())
}
