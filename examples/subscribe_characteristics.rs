use std::thread;
use std::time::Duration;
use async_std::{
    prelude::{FutureExt, StreamExt},
    task,
};

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
use btleplug::api::{UUID, ValueNotification, Central, CentralEvent, Peripheral, Characteristic, CharPropFlags};

const PERIPHERAL_NAME_MATCH_FILTER: &'static str = "Neuro";
// string to match with BLE name
const SUBSCRIBE_TO_CHARACTERISTIC: UUID = UUID::B128( // only NOTIFY type should be specified   s
    [0x1B, 0xC5, 0xD5, 0xA5, 0x02, 0x00, 0xCF, 0x88, 0xE4, 0x11, 0xB9, 0xD6, 0x03, 0x00, 0x2F, 0x3D]);
//3D:2F:00:03:D6:B9:11:E4:88:CF:00:02:A5:D5:C5:1B
// const DEVICE_COMMAND: UUID = UUID::B16(0x4600);
// const DEVICE_COMMAND: Vec<u8> = vec![0x46];

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
    println!("Received data from [{:?}] = {:?}", data.uuid, data.value.get(0));
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
                                    // println!("Checking CHARACTERISTIC...: {:?} result = {:?}", char_item.uuid,
                                    //          char_item.uuid == SUBSCRIBE_TO_CHARACTERISTIC);
                                    // subscribe on selected chars
                                    if char_item.uuid == SUBSCRIBE_TO_CHARACTERISTIC
                                        && char_item.properties == CharPropFlags::NOTIFY {
                                        println!("Lets try subscribe to desired CHARACTERISTIC...: {:?}", char_item.uuid);
                                        // do subscribe
                                        //     peripheral.on_notification(Box::new(my_on_notification_handler));
                                        peripheral.on_notification(
                                            Box::new(|data: ValueNotification| {
                                                let handle = thread::spawn(move || {
                                                    println!("Received data from [{:?}] = {:?}", data.uuid, data.value.get(0));
                                                });
                                            })
                                        );
                                        let subscribe_result = peripheral.subscribe(&char_item);
                                        let is_subscribed = subscribe_result.is_ok();
                                        println!("Is subscribed? = {}", is_subscribed);
                                        if is_subscribed {
                                            // send command to device
                                            let DEVICE_COMMAND = vec![0x46];
                                            let connect_result = peripheral.command(&char_item, &DEVICE_COMMAND);
                                            println!("Sent command OK? = {:?}", connect_result.is_ok());
                                            while connect_result.is_ok() {
                                                print!(".");
                                                thread::sleep(Duration::from_millis(10));
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

/*                            let (event_sender, event_receiver) = channel(256);

                            let on_event = move |event: CentralEvent| match event {
                                CentralEvent::DeviceDiscovered(bd_addr) => {
                                    println!("DeviceDiscovered: {:?}", bd_addr);
                                    let s = event_sender.clone();
                                    let e = event.clone();
                                    task::spawn(async move {
                                        s.send(e).await;
                                    });
                                }
                                CentralEvent::DeviceConnected(bd_addr) => {
                                    println!("DeviceConnected: {:?}", bd_addr);
                                    let s = event_sender.clone();
                                    let e = event.clone();
                                    task::spawn(async move {
                                        s.send(e).await;
                                    });
                                }
                                CentralEvent::DeviceDisconnected(bd_addr) => {
                                    println!("DeviceDisconnected: {:?}", bd_addr);
                                    let s = event_sender.clone();
                                    let e = event.clone();
                                    task::spawn(async move {
                                        s.send(e).await;
                                    });
                                }
                                _ => {}
                            };
                            adapter.on_event(Box::new(on_event));*/
