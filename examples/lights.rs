// See the "macOS permissions note" in README.md before running this on macOS
// Big Sur or later.

use btleplug::api::{bleuuid::uuid_from_u16, Central, Manager as _, Peripheral as _, WriteType};
use btleplug::platform::{Adapter, Manager, Peripheral};
use rand::{thread_rng, Rng};
use std::error::Error;
use std::time::Duration;
use uuid::Uuid;

const LIGHT_CHARACTERISTIC_UUID: Uuid = uuid_from_u16(0xFFE9);
use tokio::time;

async fn find_light(central: &Adapter) -> Option<Peripheral> {
    for p in central.peripherals().await.unwrap() {
        if p.properties()
            .await
            .unwrap()
            .local_name
            .iter()
            .any(|name| name.contains("LEDBlue"))
        {
            return Some(p);
        }
    }
    None
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    pretty_env_logger::init();

    let manager = Manager::new().await.unwrap();

    // get the first bluetooth adapter
    let central = manager
        .adapters()
        .await
        .expect("Unable to fetch adapter list.")
        .into_iter()
        .nth(0)
        .expect("Unable to find adapters.");

    // start scanning for devices
    central.start_scan().await?;
    // instead of waiting, you can use central.event_receiver() to get a channel
    // to listen for notifications on.
    time::sleep(Duration::from_secs(2)).await;

    // find the device we're interested in
    let light = find_light(&central).await.expect("No lights found");

    // connect to the device
    light.connect().await?;

    // discover characteristics
    light.discover_characteristics().await?;

    // find the characteristic we want
    let chars = light.characteristics();
    let cmd_char = chars
        .iter()
        .find(|c| c.uuid == LIGHT_CHARACTERISTIC_UUID)
        .expect("Unable to find characterics");

    // dance party
    let mut rng = thread_rng();
    for _ in 0..20 {
        let color_cmd = vec![0x56, rng.gen(), rng.gen(), rng.gen(), 0x00, 0xF0, 0xAA];
        light
            .write(&cmd_char, &color_cmd, WriteType::WithoutResponse)
            .await?;
        time::sleep(Duration::from_millis(200)).await;
    }
    Ok(())
}
