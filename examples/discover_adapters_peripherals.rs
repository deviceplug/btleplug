use std::error::Error;
use std::time::Duration;
use tokio::time;

use btleplug::api::{Central, Manager as _, Peripheral};
use btleplug::platform::Manager;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    pretty_env_logger::init();

    let manager = Manager::new().await?;
    let adapter_list = manager.adapters().await?;
    if adapter_list.is_empty() {
        eprintln!("Bluetooth adapter(s) were NOT found, sorry...");
    } else {
        for adapter in adapter_list.iter() {
            println!("connecting to BLE adapter: ...");

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
                    println!(
                        "peripheral : {:?} is connected: {:?}",
                        properties.local_name,
                        peripheral.is_connected().await?
                    );
                    if properties.local_name.is_some() && !is_connected {
                        println!(
                            "start connect to peripheral : {:?}...",
                            properties.local_name
                        );
                        peripheral
                            .connect()
                            .await
                            .expect("Can't connect to peripheral...");
                        let is_connected = peripheral.is_connected().await?;
                        println!(
                            "now connected (\'{:?}\') to peripheral : {:?}...",
                            is_connected, properties.local_name
                        );
                        let chars = peripheral.discover_characteristics().await?;
                        if is_connected {
                            println!(
                                "Discover peripheral : \'{:?}\' characteristics...",
                                properties.local_name
                            );
                            for characteristic in chars.into_iter() {
                                println!("{:?}", characteristic);
                            }
                            println!(
                                "disconnecting from peripheral : {:?}...",
                                properties.local_name
                            );
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
