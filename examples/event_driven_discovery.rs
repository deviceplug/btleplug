extern crate btleplug;
extern crate rand;

use async_std::{
    prelude::{FutureExt, StreamExt},
    sync::{channel, Receiver},
    task,
};
use btleplug::api::{Central, CentralEvent, Peripheral};
#[cfg(target_os = "linux")]
use btleplug::bluez::{adapter::ConnectedAdapter, manager::Manager};
#[cfg(target_os = "macos")]
use btleplug::corebluetooth::{adapter::Adapter, manager::Manager};
#[cfg(target_os = "windows")]
use btleplug::winrtble::{adapter::Adapter, manager::Manager};
use std::thread;
use std::time::Duration;

// adapter retrieval works differently depending on your platform right now.
// API needs to be aligned.

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn get_central(manager: &Manager) -> Adapter {
    let adapters = manager.adapters().unwrap();
    adapters.into_iter().nth(0).unwrap()
}

#[cfg(target_os = "linux")]
fn get_central(manager: &Manager) -> ConnectedAdapter {
    let adapters = manager.adapters().unwrap();
    let adapter = adapters.into_iter().nth(0).unwrap();
    adapter.connect().unwrap()
}

pub fn main() {
    let manager = Manager::new().unwrap();

    // get the first bluetooth adapter
    // connect to the adapter
    let central = get_central(&manager);

    // start scanning for devices
    central.start_scan().unwrap();
    // instead of waiting, you can use central.on_event to be notified of
    // new devices

    let (event_sender, event_receiver) = channel(256);
    // Add ourselves to the central event handler output now, so we don't
    // have to carry around the Central object. We'll be using this in
    // connect anyways.
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

    central.on_event(Box::new(on_event));

    let mut count = 0;
    loop {
        count += 1;
        println!("Count = {}", count);
        thread::sleep(Duration::from_secs(1));
    }
}
