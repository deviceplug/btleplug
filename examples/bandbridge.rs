use openssl::symm::{Cipher, Crypter, Mode};
use std::str::FromStr;
use std::sync::mpsc;
use std::sync::mpsc::RecvTimeoutError;
use std::thread;
use std::time::Duration;

use btleplug::api::{Central, Peripheral, ValueNotification, UUID};
#[cfg(target_os = "linux")]
use btleplug::bluez::{adapter::Adapter, adapter::ConnectedAdapter, manager::Manager};

#[cfg(target_os = "linux")]
fn connect_to(adapter: &Adapter) -> ConnectedAdapter {
    adapter
        .connect()
        .expect("Error connecting to BLE Adapter....") //linux
}
/**
If you are getting run time error like that :
 thread 'main' panicked at 'Can't scan BLE adapter for connected devices...: PermissionDenied', src/libcore/result.rs:1188:5
 you can try to run app with > sudo ./discover_adapters_peripherals
 on linux
**/

/**
 * This example is based on gadgetbridge codebase. For details refer to its codebase/documentation.
**/
fn main() {
    let manager = Manager::new().unwrap();
    let adapter_list = manager.adapters().unwrap();
    if adapter_list.len() <= 0 {
        eprint!("Bluetooth adapter(s) were NOT found, sorry...\n");
    } else {
        let adapter = adapter_list.first().unwrap();
        print!("connecting to BLE adapter: ...");

        let connected_adapter = connect_to(&adapter);
        println!("Connected");

        connected_adapter
            .start_scan()
            .expect("Can't scan BLE adapter for connected devices...");
        thread::sleep(Duration::from_secs(2));
        // bluetooh address of the mi band 5
        // edit it to your bands address
        let mut m1 = [0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        m1.reverse();
        let address = btleplug::api::BDAddr { address: m1 };
        let peripheral = connected_adapter
            .peripherals()
            .into_iter()
            .find(|p| p.properties().address == address)
            .expect(&format! {"Peripheral with address {:?} not found", address});
        println!(
            "peripheral : {:?} with address: {:?} is connected: {:?}",
            peripheral.properties().local_name,
            peripheral.properties().address,
            peripheral.is_connected()
        );
        if !peripheral.is_connected() {
            println!(
                "start connect to peripheral : {:?}...",
                peripheral.properties().local_name
            );
            peripheral
                .connect()
                .expect("Can't connect to peripheral...");
            println!(
                "now connected (\'{:?}\') to peripheral : {:?}...",
                peripheral.is_connected(),
                peripheral.properties().local_name
            );
        }
        if peripheral.is_connected() {
            println!(
                "Discover peripheral : \'{:?}\' characteristics...",
                peripheral.properties().local_name
            );
        }
        let chars = peripheral.discover_characteristics().unwrap();
        let authchar = UUID::from_str("00000009-0000-3512-2118-0009af100700").unwrap();
        let authchar = chars
            .into_iter()
            .find(|c| c.uuid == authchar)
            .expect("authchar not found");
        let _ = peripheral.subscribe(&authchar);

        // Initiate authentication
        let requestauthnumber = [0x80 | 0x02, 0x00, 0x02, 0x01, 0x00];
        peripheral.command_async(&authchar, &requestauthnumber, None);

        let (tx, rx) = mpsc::channel();
        peripheral.on_notification(Box::new(move |recv| tx.send(recv).unwrap()));

        thread::spawn(move || loop {
            // This comparison is not needed. Because  we only subscribed for
            // notification for auth charachteristic
            //
            // if Ok(uuid) == UUID::from_str("00000009-0000-3512-2118-0009af100700")
            //
            let value: Vec<u8> = rx
                .recv_timeout(Duration::from_secs(1))
                .and_then(|recvd: ValueNotification| Ok(recvd.value))
                .unwrap_or_else(|_: RecvTimeoutError| {
                    println! {"received nothing, waiting .."};
                    vec![0, 0, 0]
                });
            match value[..3] {
                [0x10, 0x82, 0x01] => {
                    // this key needs to be extracted; checkout gadgetbridge wiki
                    // for miband 5
                    let key = [
                        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                        0x00, 0x00, 0x00, 0x00,
                    ];
                    println! {"{:?}", &value[3..19]};
                    println! {"{:0x?}", &value[3..19]};
                    // println! {"{}", &value[3..19].len()};
                    println! {"Will now encrypt..."}

                    let mut ciphertext = vec![0u8; 32];
                    let encrypter = Crypter::new(Cipher::aes_128_ecb(), Mode::Encrypt, &key, None);
                    let _ = encrypter
                        .unwrap()
                        .update(&value[3..19], ciphertext.as_mut_slice())
                        .unwrap();

                    // need to send back the ecrypted number!!
                    // 0x83, 0x00, aes(device_key,MODE_ECB).encryp(value[3..19])
                    peripheral.command_async(
                        &authchar,
                        &[&[0x80 | 0x03, 0x00], &ciphertext[..16]].concat(),
                        None,
                    );
                    // println!("{:?}", ciphertext.as_slice());
                    // println!("{:0x?}", ciphertext.as_slice());
                }
                [0x10, 0x83, 0x01] => {
                    println! {"auth successful !!"}
                    peripheral
                        .disconnect()
                        .expect("Error on disconnecting from BLE peripheral ");
                    break;
                }
                _ => {}
            }
        })
        .join()
        .unwrap();
    }
}
