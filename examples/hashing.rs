// See the "macOS permissions note" in README.md before running this on macOS
// Big Sur or later.

use btleplug::api::{Central, CentralEvent, Manager as _, PeripheralIdent, ScanFilter};
use btleplug::platform::{Adapter, Manager, Peripheral, PeripheralId, PeripheralIdKeyed};
use futures::stream::StreamExt;
use std::collections::HashSet;
use std::error::Error;

async fn get_central(manager: &Manager) -> Adapter {
    let adapters = manager.adapters().await.unwrap();
    adapters.into_iter().nth(0).unwrap()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    pretty_env_logger::init();
    let manager = Manager::new().await?;
    let central = get_central(&manager).await;
    let mut events = central.events().await?;
    let mut ids = HashSet::<PeripheralId>::new();
    let mut keys = HashSet::<PeripheralIdKeyed>::new();
    central.start_scan(ScanFilter::default()).await?;

    while let Some(event) = events.next().await {
        match &event {
            CentralEvent::DeviceDiscovered(id) => {
                let periph = central.peripheral(&id).await?;
                assert!(event.id() == periph.id() && id.id() == *id);
                ids.insert(event.id());
                // Peripheral doesn't implement Hash, but PeripheralIdentKey does,
                // It hashes a periheral based upon the hash of its PeripheralId.
                //
                // If the platform returns the same PeripheralId for peripherals on
                // different adapters (untested), then PeripheralIdKeyed would have the same hash
                // for both.

                let periph_clone = periph.clone();
                // periph moved into keys.
                keys.insert(periph.into());
                keys.contains(event.get_id());
                keys.contains(id.get_id());
                assert!(
                    keys.contains(event.get_id())
                        && keys.contains(periph_clone.get_id())
                        && keys.contains(id)
                );

                // move a PeripheralIdKeyed out, and get our Peripheral from it.
                let periph: Peripheral = keys.take(id).expect("Peripheral not in set").peripheral();
                assert!(periph.id() == periph_clone.id())
            }
            _ => {
                let id = event.id();
                if !ids.contains(&id) {
                    ids.insert(id);
                }
            }
        }
    }
    Ok(())
}
