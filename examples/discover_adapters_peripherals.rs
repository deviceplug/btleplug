#[allow(unused_imports)]
#[allow(dead_code)]

use std::thread;
use std::time::Duration;
use rand::{Rng, thread_rng};
#[cfg(target_os = "linux")]
use btleplug::bluez::{adapter::ConnectedAdapter, manager::Manager};
#[cfg(target_os = "windows")]
use btleplug::winrtble::{adapter::Adapter, manager::Manager};
#[cfg(target_os = "macos")]
use btleplug::corebluetooth::{adapter::Adapter, manager::Manager};
use btleplug::api::{UUID, Central, Peripheral, Characteristic};

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
            println!("connecting to BLE adapter: {:?}...", adapter.name);
            let connected_adapter: ConnectedAdapter = adapter.connect().expect("Error connecting to BLE Adapter....");
            println!("connected adapter {:?} is UP: {:?}", connected_adapter.adapter.name, connected_adapter.adapter.is_up());
            println!("adapter states : {:?}", connected_adapter.adapter.states);
            thread::sleep(Duration::from_secs(2));
            connected_adapter.start_scan().expect("Can't scan BLE adapter for connected devices...");
            thread::sleep(Duration::from_secs(2));
            if connected_adapter.peripherals().is_empty() {
                eprintln!("->>> BLE peripheral devices were not found, sorry. Exiting...");
            } else {
                 // all peripheral devices in range
                for peripheral in connected_adapter.peripherals().iter() {
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
