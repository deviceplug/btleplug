use std::thread;
use std::time::Duration;

#[allow(unused_imports)]
#[cfg(target_os = "linux")]
use btleplug::bluez::{adapter::Adapter, manager::Manager};
#[allow(unused_imports)]
#[cfg(target_os = "windows")]
use btleplug::winrtble::{adapter::Adapter, manager::Manager};
#[allow(unused_imports)]
#[cfg(target_os = "macos")]
use btleplug::corebluetooth::{adapter::Adapter, manager::Manager};
#[allow(unused_imports)]
use btleplug::api::{ValueNotification, Central, CentralEvent, Peripheral, Characteristic, CharPropFlags};
use std::io::Cursor;
use uuid::Uuid;
use btleplug::api::{NotificationHandler, UUID};

const PERIPHERAL_NAME_MATCH_FILTER: &'static str = "Neuro"; // filter BLE device by partial name

// string to match with BLE name
const SUBSCRIBE_TO_CHARACTERISTIC: UUID = UUID::B128( // only NOTIFY type should be specified   s
                                                      [0x9E,0xCA,0xDC,0x24,0x0E,0xE5,0xA9,0x67,0x93,0xF3,0x34,0xB5,0x02,0x00,0x40,0x6E]);
//6E:40:00:02:B5:34:F3:93:67:A9:E5:0E:24:DC:CA:9E - in REVERSED bytes ORDER !!!

#[cfg(target_os = "linux")]
fn print_adapter_info(adapter: &Adapter) {
    println!(
        "connected adapter {:?} is powered: {:?}",
        adapter.name(),
        adapter.is_powered()
    );
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn print_adapter_info(_adapter: &Adapter) {
    println!("adapter info can't be printed on Windows 10 or mac");
}

fn my_on_notification_handler(data: ValueNotification) {
    let rdr = Cursor::new(data.value);
    println!("Received data from [{:?}] = {:?}", data.uuid, rdr);
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

            print_adapter_info(&adapter);
            adapter.start_scan().expect("Can't scan BLE adapter for connected devices...");
            thread::sleep(Duration::from_secs(2));
            // let mut handle;

            if adapter.peripherals().is_empty() {
                eprintln!("->>> BLE peripheral devices were not found, sorry. Exiting...");
            } else {
                // all peripheral devices in range
                for peripheral in adapter.peripherals().iter() {
                    println!("peripheral : {:?} is connected: {:?}", peripheral.properties().local_name, peripheral.is_connected());
                    // filter needed peripheral
                    if peripheral.properties().local_name.is_some()
                        && !peripheral.is_connected()
                        && peripheral.properties().local_name.unwrap().contains(PERIPHERAL_NAME_MATCH_FILTER) {
                        println!("start connect to peripheral : {:?}...", peripheral.properties().local_name);
                        peripheral.connect().expect("Can't connect to peripheral...");
                        println!("now connected (\'{:?}\') to peripheral : {:?}...", peripheral.is_connected(), peripheral.properties().local_name);
                        let chars = peripheral.discover_characteristics();
                        if peripheral.is_connected() {
                            println!("Discover peripheral : \'{:?}\' characteristics...", peripheral.properties().local_name);
                            for chars_vector in chars.into_iter() {
                                for char_item in chars_vector.iter() {
                                    println!("Checking CHARACTERISTIC...: {:?} result = {:?}", char_item.uuid,
                                             char_item.uuid == SUBSCRIBE_TO_CHARACTERISTIC);
                                    // subscribe on selected chars
                                    if char_item.uuid == SUBSCRIBE_TO_CHARACTERISTIC
                                        && char_item.properties == CharPropFlags::NOTIFY {
                                        println!("Lets try subscribe to desired CHARACTERISTIC...: {:?}", char_item.uuid);

                                        // do subscribe to notify characteristic
                                        peripheral.on_notification(Box::new(my_on_notification_handler));

                                        let subscribe_result = peripheral.subscribe(&char_item);
                                        let is_subscribed = subscribe_result.is_ok();
                                        println!("Is subscribed? = {}", is_subscribed);
                                        if is_subscribed {
                                            loop {
                                                // print!(".");
                                                thread::sleep(Duration::from_millis(1));
                                            }
                                        }
                                    }
                                }
                            }

                            println!("disconnecting from peripheral : {:?}...", peripheral.properties().local_name);
                            peripheral.disconnect().expect("Error on disconnecting from BLE peripheral");
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
