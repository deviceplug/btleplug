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
    info!("Adapter: {:#?}", adapter);

    let connected = adapter.connect(vec![]).unwrap();

    connected.start_scan().unwrap();

    std::thread::sleep(std::time::Duration::from_secs(5));
    let devices = connected.discovered.lock().unwrap();
    info!("Devices: {:#?}", *devices);

    // println!("Adapter: {:#?}", manager.update(&connected.adapter).unwrap());

}
