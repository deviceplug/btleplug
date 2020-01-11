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
- [blurmac](https://github.com/servo/devices) ([alternative repo?](https://github.com/akosthekiss/blurdroid))
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

## Platform Feature Table

| Feature | Windows | MacOS | Linux | Android | iOS |
|---------|---------|-------|-------|---------|-----|
| Bring Up Adapter |X||X|||
| Handle Multiple Adapters ||||||
| Discover Devices |X||X|||
| └ Discover Service List ||||||
| └ Discover Name ||||||
| └ Discover Manufacturer/Service Data ||||||
| GATT Server Connect ||||||
| GATT Server Connect Event ||||||
| GATT Server Disconnect ||||||
| GATT Server Disconnect Event ||||||
| Write Characteristic ||||||
| Read Characteristic ||||||
| Subscribe to Characteristic ||||||
| Unsubscribe from Characteristic ||||||

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
