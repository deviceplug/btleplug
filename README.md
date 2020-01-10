# btleplug

btleplug is a fork of the seemingly-abandoned
[rumble](https://github.com/mwylde/rumble) Rust BLE library. Our goal
is the bring in some of the outstanding PRs from that project, expand
the platform support to MacOS (Partial WinRT/UWP support is already in
thanks to PRs in rumble), and possibly make the API surface more
ergonomic for being a truly cross-platform library.

Oh and async might happen to because why not.

Thanks to @mwylde and all the other rumble contributors for getting
the library as far as it is now!

## Old README Content:

### Rumble

Rumble is a Bluetooth Low Energy (BLE) central module library for Rust. 
Currently only Linux (with the BlueZ bluetooth library) is supported, although 
other operating systems may be supported in the future. Rumble interfaces with 
BlueZ using its socket interface rather than DBus. This offers much more control 
and reliability over the DBus interface, and does not require running BlueZ in 
experimental mode for BLE.

As of version 0.2, the API is becoming more stable and the library itself more
useful. You should still expect to encounter bugs, limitations, and odd behaviors.
Pull requests (and wireshark traces) welcome!

### Usage

An example of how to use the library to control some BLE smart lights:

```rust
extern crate rumble;
extern crate rand;

use std::thread;
use std::time::Duration;
use rand::{Rng, thread_rng};
use rumble::bluez::manager::Manager;
use rumble::api::{UUID, Central, Peripheral};

pub fn main() {
    let manager = Manager::new().unwrap();

    // get the first bluetooth adapter
    let adapters = manager.adapters().unwrap();
    let mut adapter = adapters.into_iter().nth(0).unwrap();

    // reset the adapter -- clears out any errant state
    adapter = manager.down(&adapter).unwrap();
    adapter = manager.up(&adapter).unwrap();

    // connect to the adapter
    let central = adapter.connect().unwrap();

    // start scanning for devices
    central.start_scan().unwrap();
    // instead of waiting, you can use central.on_event to be notified of
    // new devices
    thread::sleep(Duration::from_secs(2));

    // find the device we're interested in
    let light = central.peripherals().into_iter()
        .find(|p| p.properties().local_name.iter()
            .any(|name| name.contains("LEDBlue"))).unwrap();

    // connect to the device
    light.connect().unwrap();

    // discover characteristics
    light.discover_characteristics().unwrap();

    // find the characteristic we want
    let chars = light.characteristics();
    let cmd_char = chars.iter().find(|c| c.uuid == UUID::B16(0xFFE9)).unwrap();

    // dance party
    let mut rng = thread_rng();
    for _ in 0..20 {
        let color_cmd = vec![0x56, rng.gen(), rng.gen(), rng.gen(), 0x00, 0xF0, 0xAA];
        light.command(&cmd_char, &color_cmd).unwrap();
        thread::sleep(Duration::from_millis(200));
    }
}
```
