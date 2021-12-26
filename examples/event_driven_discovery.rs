// See the "macOS permissions note" in README.md before running this on macOS
// Big Sur or later.

use btleplug::api::{
    bleuuid::BleUuid, Central, CentralEvent, Manager as _, Peripheral as _, ScanFilter,
};
use btleplug::platform::Manager;
use futures::stream::StreamExt;
use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    pretty_env_logger::init();

    let manager = Manager::new().await?;

    let adapters = manager.adapters().await?;

    // start scanning for devices
    for adapter in &adapters {
        adapter.start_scan(ScanFilter::default()).await?;
    }

    let mut stream_map = tokio_stream::StreamMap::new();
    // Turn adapters into something like:
    // vec![(0, adaptors[0].events().await?), ...].iter()
    //
    // Insert these into a StreamMap
    // For the key we use the index into adapters.
    // The value is the event stream.
    //
    for (idx, evt_stream) in adapters
        .iter()
        .enumerate()
        .map(|(idx, adapter)| (idx, adapter.events()))
    {
        stream_map.insert(idx, evt_stream.await?);
    }

    // Print based on whatever the event receiver outputs. Note that the event
    // receiver blocks, so in a real program, this should be run in its own
    // thread (not task, as this library does not yet use async channels).
    loop {
        tokio::select! {
           idx_event_opt = stream_map.next() => {
              if let Some((adapter_idx, event)) = idx_event_opt {
                 match event {
                        CentralEvent::DeviceDiscovered(id) => {
                            // Example of getting the perepheral for the id.
                            let peripheral = adapters[adapter_idx].peripheral(&id).await?;
                            assert!(id == peripheral.id());
                            println!("DeviceDiscovered: {:?}", id);
                        }
                        CentralEvent::DeviceConnected(id) => {
                            println!("DeviceConnected: {:?}", id);
                        }
                        CentralEvent::DeviceDisconnected(id) => {
                            println!("DeviceDisconnected: {:?}", id);
                        }
                        CentralEvent::ManufacturerDataAdvertisement {
                            id,
                            manufacturer_data,
                        } => {
                            println!(
                                "ManufacturerDataAdvertisement: {:?}, {:?}",
                                id, manufacturer_data
                            );
                        }
                        CentralEvent::ServiceDataAdvertisement { id, service_data } => {
                            println!("ServiceDataAdvertisement: {:?}, {:?}", id, service_data);
                        }
                        CentralEvent::ServicesAdvertisement { id, services } => {
                            let services: Vec<String> =
                                services.into_iter().map(|s| s.to_short_string()).collect();
                            println!("ServicesAdvertisement: {:?}, {:?}", id, services);
                        }
                        _ => {}
                 }
              }
           }
        }
    }
}
