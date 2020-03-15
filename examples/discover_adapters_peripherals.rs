#[allow(dead_code)]

#[allow(unused_imports)]
use std::thread;
use std::time::Duration;
#[allow(unused_imports)]
use rand::{Rng, thread_rng};

#[allow(unused_imports)]
#[cfg(target_os = "linux")]
use btleplug::bluez::{adapter::Adapter, adapter::ConnectedAdapter, manager::Manager};
#[allow(unused_imports)]
#[cfg(target_os = "windows")]
use btleplug::winrtble::{adapter::Adapter, manager::Manager};
#[allow(unused_imports)]
#[cfg(target_os = "macos")]
use btleplug::corebluetooth::{adapter::Adapter, manager::Manager};
#[allow(unused_imports)]
use btleplug::api::{UUID, Central, Peripheral, Characteristic};

#[cfg(target_os = "linux")]
fn connect_to(adapter: &Adapter) -> ConnectedAdapter {
        adapter.connect().expect("Error connecting to BLE Adapter....") //linux
}
#[cfg(target_os = "linux")]
fn print_adapter_info(adapter: &ConnectedAdapter) {
    println!("connected adapter {:?} is UP: {:?}", adapter.name, adapter.is_up());
    println!("adapter states : {:?}", adapter.states);
}

#[cfg(target_os = "windows")]
fn connect_to(adapter: &Adapter) -> &Adapter {
    adapter //windows 10
}
#[cfg(target_os = "windows")]
fn print_adapter_info(_adapter: &Adapter) {
    println!("adapter info can't be printed on Windows 10");
}

/**
If you are getting run time error like that :
 thread 'main' panicked at 'Can't scan BLE adapter for connected devices...: PermissionDenied', src/libcore/result.rs:1188:5
 you can try to run app with > sudo ./discover_adapters_peripherals
 on linux
**/
fn main() {
    let manager = Manager::new().unwrap();
    let adapter_list = manager.adapters().unwrap();
    if adapter_list.len() <= 0 {
        eprint!("Bluetooth adapter(s) were NOT found, sorry...\n");
    } else {
        for adapter in adapter_list.iter() {
            println!("connecting to BLE adapter: ...");
            let connected_adapter = connect_to(&adapter);
            print_adapter_info(&connected_adapter);
            adapter.start_scan().expect("Can't scan BLE adapter for connected devices...");
            thread::sleep(Duration::from_secs(2));
            if adapter.peripherals().is_empty() {
                eprintln!("->>> BLE peripheral devices were not found, sorry. Exiting...");
            } else {
                 // all peripheral devices in range
                // for peripheral in connected_adapter.peripherals().iter() {
                for peripheral in adapter.peripherals().iter() {
                    println!("peripheral : {:?} is connected: {:?}", peripheral.properties().local_name, peripheral.is_connected());
                    if peripheral.properties().local_name.is_some() && !peripheral.is_connected() {
                        println!("start connect to peripheral : {:?}...", peripheral.properties().local_name);
                        peripheral.connect().expect("Can't connect to peripheral...");
                        println!("now connected (\'{:?}\') to peripheral : {:?}...", peripheral.is_connected(), peripheral.properties().local_name);
                        let chars = peripheral.discover_characteristics();
                        if peripheral.is_connected() {
                            println!("Discover peripheral : \'{:?}\' characteristics...", peripheral.properties().local_name);
                            for chars_vector in chars.into_iter() {
                                for char_item in chars_vector.iter() {
                                    println!("{:?}", char_item);
                                }
                            }
                            println!("disconnecting from peripheral : {:?}...", peripheral.properties().local_name);
                            peripheral.disconnect().expect("Error on disconnecting from BLE peripheral ");
                        }
                    } else {
                        //sometimes peripheral is not discovered completely
                        eprintln!("SKIP connect to UNKNOWN peripheral : {:?}", peripheral);
                    }
                }
            }
        }
    }
}