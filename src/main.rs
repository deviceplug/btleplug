extern crate rust_bluez;

#[macro_use]
extern crate log;
extern crate env_logger;

use rust_bluez::manager::Manager;

fn main() {
    env_logger::init().unwrap();

    let manager = Manager::new().unwrap();

    let adapters = manager.adapters().unwrap();
    let mut adapter = adapters.into_iter().nth(0).unwrap();

    // println!("Adapter: {:#?}", adapter);

    adapter = manager.down(&adapter).unwrap();
    // println!("Adapter: {:#?}", adapter);

    adapter = manager.up(&adapter).unwrap();
    debug!("Adapter: {:#?}", adapter);

    let scanner = adapter.scanner(Some(|device| {
        info!("Device!: {:?}", device);
    })).unwrap();
    std::thread::sleep(std::time::Duration::from_secs(10));
    info!("Devices: {:#?}", scanner.devices());

    // println!("Adapter: {:#?}", manager.update(&connected.adapter).unwrap());

}
