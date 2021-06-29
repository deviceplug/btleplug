use std::time::Duration;

use btleplug::api::{ValueNotification, CharPropFlags};
use btleplug::api::{Central, Manager as _, Peripheral};
use btleplug::platform::{Adapter, Manager};
use std::io::Cursor;
use std::error::Error;
use uuid::Uuid;
use tokio::time;
use futures::stream::{StreamExt};

const PERIPHERAL_NAME_MATCH_FILTER: &'static str = "Neuro"; // filter BLE device by partial name
const NOTIFY_CHARACTERISTIC_UUID:Uuid = Uuid::from_u128(0x6e400002_b534_f393_67a9_e50e24dccA9e); // subscribe UUID

#[cfg(target_os = "linux")]
fn print_adapter_info(adapter: &Adapter) {
    println!("connected adapter {:?}", adapter);
}
#[cfg(any(target_os = "windows", target_os = "macos"))]
fn print_adapter_info(_adapter: &Adapter) {
    println!("adapter info can't be printed on Windows 10 or mac");
}

/// Processing received BLE data
fn my_on_notification_handler(data: ValueNotification) {
    let rdr = Cursor::new(data.value);
    println!("Received data from [{:?}] = {:?}", data.uuid, rdr);
}

/**
If you are getting run time error like that :
 thread 'main' panicked at 'Can't scan BLE adapter for connected devices...: PermissionDenied',
 you can try to run app with > sudo ./discover_adapters_peripherals
 on linux
**/
#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {

    let manager = Manager::new().await?;
    let adapter_list = manager.adapters().await?;
    if adapter_list.len() <= 0 {
        eprint!("Bluetooth adapter(s) were NOT found, sorry...\n");
    } else {
        for adapter in adapter_list.iter() {
            println!("connecting to BLE adapter...");
            print_adapter_info(&adapter);

            adapter
                .start_scan()
                .await
                .expect("Can't scan BLE adapter for connected devices...");
            time::sleep(Duration::from_secs(2)).await;
            let peripherals = adapter.peripherals().await?;

            if peripherals.is_empty() {
                eprintln!("->>> BLE peripheral devices were not found, sorry. Exiting...");
            } else {
                // all peripheral devices in range
                for peripheral in peripherals.iter() {
                    let properties = peripheral.properties().await?;
                    let is_connected = peripheral.is_connected().await?;
                    let local_name = properties.local_name.unwrap_or(String::from("Unknown prop name"));
                    println!("peripheral : {:?} is connected: {:?}",
                             &local_name, is_connected);
                    // filter needed peripheral
                    if local_name.contains(PERIPHERAL_NAME_MATCH_FILTER) {
                        println!("found matching peripheral : {:?}...", &local_name);
                        if !is_connected { // we can be connected previously
                            let connect_result = peripheral
                                .connect()
                                .await;
                            match connect_result {
                                Ok(_) => {}
                                Err(err) => {
                                    eprintln!("Can't connect to peripheral, skipping due to error = {:?}...", err);
                                    continue;
                                }
                            }
                        }
                        let is_connected = peripheral.is_connected().await?;
                        println!(
                            "now connected (\'{:?}\') to peripheral : {:?}...",
                            is_connected, &local_name
                        );
                        let chars = peripheral.discover_characteristics().await?;
                        if is_connected {
                            println!(
                                "Discover peripheral : \'{:?}\' characteristics...",
                                local_name
                            );
                            for char_item in chars.into_iter() {
                                println!("Checking CHARACTERISTIC...: {:?} result = {:?}", char_item.uuid,
                                         char_item.uuid == NOTIFY_CHARACTERISTIC_UUID);
                                // subscribe on selected uuid
                                if char_item.uuid == NOTIFY_CHARACTERISTIC_UUID
                                    && char_item.properties == CharPropFlags::NOTIFY {

                                    println!("Lets try subscribe to desired CHARACTERISTIC...: {:?}", char_item.uuid);
                                    peripheral.subscribe(&char_item).await?;
                                    let mut notify_result = peripheral.notifications().await?;
                                    // process while BLE connection is not broken or stopped
                                    while let Some(data) = notify_result.next().await {
                                        my_on_notification_handler(data)
                                    }
                                }
                            }
                            println!("disconnecting from peripheral : {:?}...", local_name);
                            peripheral
                                .disconnect()
                                .await
                                .expect("Error on disconnecting from BLE peripheral ");
                        }
                    } else {
                        //sometimes peripheral is not discovered completely
                        eprintln!("SKIP connect to UNKNOWN peripheral : {:?}", peripheral);
                    }
                }
            }
        }
    }
    Ok(())
}
