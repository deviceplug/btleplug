extern crate rust_bluez;

use rust_bluez::manager::Manager;

fn main() {
    let manager = Manager::new().unwrap();

    let adapters = manager.adapters().unwrap();
    let mut adapter = adapters.into_iter().nth(0).unwrap();

    println!("Adapter: {:#?}", adapter);

    adapter = manager.down(&adapter).unwrap();
    println!("Adapter: {:#?}", adapter);

    adapter = manager.up(&adapter).unwrap();
    println!("Adapter: {:#?}", adapter);

    let mut connected = adapter.connect().unwrap();

    connected.scan_le().unwrap();

    println!("Adapter: {:#?}", manager.update(&connected.adapter).unwrap());

    unsafe {
        connected.print_devices();
    }
}

