# IMPORTANT NOTE

This version of btleplug is being uploaded to crates to secure the
module name in the packaging system. This version is functionally
similar to rumble, and has all of the Linux and some basic Win10
support. However, it is still extremely experimental. We recommend
waiting until 0.4.0 (hopefully out Late Jan-Early Feb 2020) to start
actually using the library.

# btleplug

btleplug is a fork of the seemingly-abandoned
[rumble](https://github.com/mwylde/rumble) Rust BLE library. Our goal
is the bring in some of the outstanding PRs from that project, expand
the platform support to MacOS (Partial WinRT/UWP support is already in
thanks to PRs in rumble), and possibly make the API surface more
ergonomic for being a truly cross-platform library.

Oh and async might happen to because why not.

btleplug is meant to be *host mode only*. If you are interested in
peripheral BTLE (i.e. acting like a Bluetooth LE device instead of
connecting to one), check out
[bluster](https://github.com/dfrankland/bluster/tree/master/src).

This library **DOES NOT SUPPORT BLUETOOTH 2/CLASSIC**. There are no
plans to add BT2/Classic support.

## A Whole New World of Bluetooth Copypasta

At the moment, and probably for the foreseeable future, very little of
what is included in BTLEPlug will be new or original code. The goal
for the moment is to get a single library that works everywhere, then
start bending the API surface around the different platform
requirements once we can at least bring up adapters and start finding
devices everywhere.

The libraries we're forking include:

- [rumble](https://github.com/mwylde/rumble)
  - Started with a bluez implementation, but became our main repo fork
    basis because it was the only library to have even a partial UWP
    implementation. API surface was built to reflect bluez 1:1, which
    makes cross-platform ergonomics a bit difficult, but it's a model
    we can start with for now, and change later.
  - Project seems to be abandoned.
- [blurmac](https://github.com/servo/devices) ([alternative repo?](https://github.com/akosthekiss/blurmac))
  - Complete-ish WebBluetooth BTLE implementation for MacOS/iOS
    CoreBluetooth, originally built for use in Mozilla's Servo
    browser. Makes some assumptions about being embedded in Servo. For
    instance, the base library doesn't spin up any event queues
    because it expects to be embedded in a Cocoa application with a
    main event queue.
  - Project seems to be abandoned.
- [blurdroid](https://github.com/servo/devices) ([alternative repo?](https://github.com/akosthekiss/blurdroid))
  - Same as blurmac, developed for Servo, but handles Android 4.4+'s
    BTLE stack via JNI calls.
  - Project seems to be abandoned.

In addition, here's the libraries we'll be referencing/cribbing from
for updating APIs.

- [bluster](https://github.com/dfrankland/bluster/tree/master/src)
  - BTLE Peripheral library. Uses async rust via Tokio.
  - Active project
- [noble-mac](https://github.com/timeular/noble-mac)
  - Noble (node BTLE module) implementation for MacOS via
    CoreBluetooth. Built in Obj-C and C.
  - Active project
- [noble-uwp](https://github.com/jasongin/noble-uwp)
  - Noble (node BTLE module) implemenation for Windows UWP. Built in
    C++.
  - Active project

## Development Goals

The issues in this repo reflect the development goals of the project.
First and foremost is getting as many platforms as possible up and
running enough to support [our main usage of this
library](https://github.com/buttplugio/buttplug-rs). For the time
being we'll most likely keep the rumble API surface model, just so we
don't have to change large portions of the code as we go.

Beyond that, some of our other goals are:

- Make API more ergonomic to support multiple bluetooth APIs (not just
  focusing on bluez)
- Add FFI so this library can be used from C (and maybe C++ using
  [cxx](https://github.com/dtolnay/cxx).
- Provide both async and sync versions of as many APIs as possible
  (once again, depending on platform API capabilities)
- Possibly create a WASM compatible layer using
  [wasm-bindgen](https://github.com/rustwasm/wasm-bindgen) and
  [WebBluetooth](https://webbluetoothcg.github.io/web-bluetooth/)

## Platform Status

- **Linux**
  - Device enumeration and characteristic/services implemented, works
    ok enough. 
  - Please file bugs and missing features if you find them.
- **Windows**
  - Device enumeration and some characteristic/service functions
    implemented. Usable but definitely missing functionality. 
  - Please file bugs and missing features if you find them.
- **MacOS**
  - Got a blurmac example up and running, now need to integrate it
    into the library. 
  - Tracking issue [here](https://github.com/deviceplug/btleplug/issues/2)
  - Please hold off on filing more issues until base implementation is
    landed.
- **Android**
  - A rust android library exists (the aforementioned
    [blurdroid](https://github.com/servo/devices)), but getting a PoC
    up and tested is going to require some work. **Definitely looking
    for help**. 
  - Tracking issue
    [here](https://github.com/deviceplug/btleplug/issues/8). 
  - Please hold off on filing more issues until base implementation is
    landed.
- **iOS**
  - Trying to figure out if the macOS implementation will translate. 
  - [Tracking issue here](https://github.com/deviceplug/btleplug/issues/12)
  - Please hold off on filing more issues until base implementation is
    landed.
- **WASM/WebBluetooth**
  - This seems more useful for a stunt hack than anything, but I love
    a good stunt hack.
  - We'd probably want
    [wasm-bindgen](https://github.com/rustwasm/wasm-bindgen) to
    support WebBluetooth via its API extensions, and just build a shim
    on top of that?
  - [Tracking issue here](https://github.com/deviceplug/btleplug/issues/13)
  - Please hold off on filing more issues until base implementation is
    landed.

### Platform Feature Table

- X: Completed and released
- O: In development
- Blank: Not started

| Feature | Windows | MacOS | Linux |
|---------|---------|-------|-------|
| Bring Up Adapter |X||X|
| Handle Multiple Adapters |||X|
| Discover Devices |X||X|
| └ Discover Services ||| [O](https://github.com/deviceplug/btleplug/issues/11) |
| └ Discover Name |X||X|
| └ Discover Manufacturer Data |X||X|
| GATT Server Connect |X||X|
| GATT Server Connect Event |X||X|
| GATT Server Disconnect |X||X|
| GATT Server Disconnect Event |X||X|
| Write to Characteristic (Sync) |X||X|
| Read from Characteristic (Async) |||X|
| Write to Characteristic (Sync) |X||X|
| Read from Characteristic (Async) |X|||
| Subscribe to Characteristic (Sync) |X||X|
| Subscribe to Characteristic (Async) ||||
| Unsubscribe from Characteristic (Sync) ||||
| Unsubscribe from Characteristic (Async) ||||
| Get Characteristic Notification Event |X||X|
| Read Descriptor ||||
| Write Descriptor ||||

## Old rumble README Content

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

## License

BTLEPlug is covered under a BSD 3-Clause License, with some parts from
Rumble/Blurmac covered under MIT/Apache dual license, and BSD 3-Clause
licenses, respectively. See LICENSE.md for more info and copyright
information.
