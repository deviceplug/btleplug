use btleplug::{
    api::{bleuuid::uuid_from_u16, Central, Peripheral, WriteType},
    platform::Manager,
};
use rand::{thread_rng, Rng};
use std::thread;
use std::time::Duration;
use uuid::Uuid;

const LIGHT_CHARACTERISTIC_UUID: Uuid = uuid_from_u16(0xFFE9);

pub fn main() {
    let manager = Manager::new().unwrap();

    // get the first bluetooth adapter
    let central = manager
        .adapters()
        .expect("Unable to fetch adapter list.")
        .into_iter()
        .nth(0)
        .expect("Unable to find adapters.");

    // start scanning for devices
    central.start_scan().unwrap();
    // instead of waiting, you can use central.event_receiver() to get a channel
    // to listen for notifications on.
    thread::sleep(Duration::from_secs(2));

    // find the device we're interested in
    let light = central
        .peripherals()
        .into_iter()
        .find(|p| {
            p.properties()
                .local_name
                .iter()
                .any(|name| name.contains("LEDBlue"))
        })
        .expect("No lights found");

    // connect to the device
    light.connect().unwrap();

    // discover characteristics
    light.discover_characteristics().unwrap();

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
            .unwrap();
        thread::sleep(Duration::from_millis(200));
    }
}
