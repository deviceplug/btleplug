extern crate rust_bluez;

#[macro_use]
extern crate log;
extern crate env_logger;

use std::time::Duration;
use std::thread::JoinHandle;

use rust_bluez::manager::Manager;
use rust_bluez::device::Device;
use rust_bluez::device::CharacteristicUUID::B16;

fn main() {
    env_logger::init().unwrap();

    let manager = Manager::new().unwrap();

    let adapters = manager.adapters().unwrap();
    let mut adapter = adapters.into_iter().nth(0).unwrap();

    // println!("Adapter: {:#?}", adapter);

    adapter = manager.down(&adapter).unwrap();
    // println!("Adapter: {:#?}", adapter);

    adapter = manager.up(&adapter).unwrap();
    info!("Adapter: {:#?}", adapter);

    let connected = adapter.connect(vec![]).unwrap();

    connected.start_scan().unwrap();

    std::thread::sleep(std::time::Duration::from_secs(1));
    let devices = connected.discovered();
    info!("Devices: {:#?}", devices);

    let lights: Vec<&Device> = devices
        .iter()
        .filter(|d| d.local_name.iter()
            .any(|name| name.contains("LED")))
        .collect();

    lights.iter().for_each(|dev| {
        info!("Connecting to {:?}", dev);
        connected.connect(dev).unwrap();
    });

    std::thread::sleep(std::time::Duration::from_secs(1));

    lights.iter().for_each(|dev| {
        connected.discover_chars(dev);
    });

    std::thread::sleep(std::time::Duration::from_secs(1));


    let threads: Vec<JoinHandle<()>> = lights.iter().map(|dev| {
        let green = vec![0x56, 0x00, 0xFF, 0x00, 0x00, 0xF0, 0xAA];
        let warm = vec![0x56, 0x00, 0x00, 0x00, 0xFF, 0x0f, 0xaa];

        let connected = connected.clone();
        let address = dev.address;
        std::thread::spawn(move|| {
            let chars = connected.device(address).unwrap().characteristics;
            let cmd_char = chars.iter().find(|c| c.uuid == B16(0xFFE9)).unwrap();

            for _ in 0..10 {
                connected.write(address, cmd_char, &green);
                std::thread::sleep(Duration::from_millis(250));
                connected.write(address, cmd_char, &warm);
                std::thread::sleep(Duration::from_millis(250));
            }
        })
    }).collect();

    threads.into_iter().for_each(|t| t.join().unwrap());

//    connected.write(first_device.address, cmd_char, &green);
//    std::thread::sleep(std::time::Duration::from_secs(5));
    std::thread::sleep(std::time::Duration::from_secs(1));
    info!("shutting down...");
}
