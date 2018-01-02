# Rumble

Rumble is a Bluetooth Low Energy (BLE) central module library for Rust. 
Currently only Linux (with the BlueZ bluetooth library) is supported, although 
other operating systems may be supported in the future. Rumble interfaces with 
BlueZ using its socket interface rather than DBus. This offers much more control 
and reliability over the DBus interface, and does not require running BlueZ in 
experimental mode for BLE.

The current state is *very* experimental. Some important features like bluetooth
encryption are not supported, and support for BLE control messages is limited. 
The API is also highly likely to change. However it is already capable of 
interfacing with many BLE devices.

## Usage

An example of how to use the library to control some BLE smart lights:

```rust
extern crate rumble;
use std::thread;
use std::time::Duration;

use rumble::manager::Manager;
use rumble::device::CharacteristicUUID::B16;

pub fn main() {
    let manager = Manager::new().unwrap();

    // get the first bluetooth adapter
    let adapters = manager.adapters().unwrap();
    let mut adapter = adapters.into_iter().nth(0).unwrap();

    // reset the adapter
    adapter = manager.down(&adapter).unwrap();
    adapter = manager.up(&adapter).unwrap();

    // connect to the adapter
    let connected = adapter.connect(vec![]).unwrap();

    // start scanning for devices
    connected.start_scan().unwrap();
    thread::sleep(Duration::from_secs(1));

    // find the device we're interested in
    let devices = connected.discovered();
    let dev = devices.iter()
        .find(|d| d.local_name.iter()
            .any(|name| name.contains("LEDBlue")))
        .unwrap();

    // connect to the device
    connected.connect(dev).unwrap();
    thread::sleep(Duration::from_secs(1));

    // discover characteristics
    connected.discover_chars(dev);
    thread::sleep(Duration::from_secs(2));

    // find the characteristics we want
    let chars = connected.device(dev.address).unwrap().characteristics;
    let status_char = chars.iter().find(|c| c.uuid == B16(0xFFE4)).unwrap();
    let cmd_char = chars.iter().find(|c| c.uuid == B16(0xFFE9)).unwrap();

    // request the device's current status
    let status_msg = vec![0xEF, 0x01, 0x77];
    connected.request(dev.address, status_char, &status_msg,
                      Some(Box::new(move |_, resp| {
                          println!("status: {:?}", resp);
                      })));

    // send a command to the device
    let green_cmd = vec![0x56, 0x00, 0xFF, 0x00, 0x00, 0xF0, 0xAA];
    connected.command(dev.address, cmd_char, &green_cmd);
    thread::sleep(Duration::from_secs(5));
}
```
