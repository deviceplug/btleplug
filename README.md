# btleplug

[![Crates.io Version](https://img.shields.io/crates/v/btleplug)](https://crates.io/crates/btleplug)
[![docs.rs page](https://docs.rs/btleplug/badge.svg)](https://docs.rs/btleplug)
[![Crates.io Downloads](https://img.shields.io/crates/d/btleplug)](https://crates.io/crates/btleplug)
[![Crates.io License](https://img.shields.io/crates/l/btleplug)](https://crates.io/crates/btleplug)

[![Discord](https://img.shields.io/discord/738080600032018443.svg?logo=discord)](https://discord.gg/QGhMFzR)

[![Github donate button](https://img.shields.io/badge/github-donate-ff69b4.svg)](https://www.github.com/sponsors/qdot)

btleplug is an async Rust BLE library, supporting Windows 10, macOS, Linux, and possibly iOS and
Android. It grew out of several earlier abandoned libraries for various platforms
([rumble](https://github.com/mwylde/rumble), [blurmac](https://github.com/servo/devices), etc...),
with the goal of building a fully cross platform library. Adding support for other platforms such as
Android is also planned.

btleplug is meant to be _host/central mode only_. If you are interested in peripheral BTLE (i.e.
acting like a Bluetooth LE device instead of connecting to one), check out
[bluster](https://github.com/dfrankland/bluster/tree/master/src).

This library **DOES NOT SUPPORT BLUETOOTH 2/CLASSIC**. There are no plans to add BT2/Classic
support.

## Development Goals

The issues in this repo reflect the development goals of the project. First and foremost is getting
as many platforms as possible up and running enough to support [our main usage of this
library](https://github.com/buttplugio/buttplug-rs).

Beyond that, some of our other goals are:

- Add FFI so this library can be used from C (and maybe C++ using
  [cxx](https://github.com/dtolnay/cxx).
- Possibly create a WASM compatible layer using
  [wasm-bindgen](https://github.com/rustwasm/wasm-bindgen) and
  [WebBluetooth](https://webbluetoothcg.github.io/web-bluetooth/).

## Platform Status

- **Linux / Windows / macOS**
  - Device enumeration and characteristic/services implemented, works
    ok.
  - Please file bugs and missing features if you find them.
- **iOS**
  - Trying to figure out if the macOS implementation will translate,
    minus things having to do with background processing. Might just
    work already?
  - [Tracking issue here](https://github.com/deviceplug/btleplug/issues/12)
  - Please file bugs and missing features if you find them.
- **Android**
  - Android implementation is in testing now, should be released soon (probably as 0.9).
  - Tracking issue
    [here](https://github.com/deviceplug/btleplug/issues/8).
  - Please hold off on filing more issues until base implementation is
    landed.
- **WASM/WebBluetooth**
  - WebBluetooth has been added to wasm-bindgen's web-sys by @qdot, and a full
    implementation has been done in other libraries that could easily be ported
    here. This is now definitely in the works, once the new API surface is
    solidified.
  - [Tracking issue here](https://github.com/deviceplug/btleplug/issues/13)
  - Please hold off on filing more issues until base implementation is
    landed.

### Linux Device Discovery Cavaet

Note that using Event Based Discovery on Bluez (Linux) can cause very odd issues with service
discovery, timeouts, and general weirdness. See [Issue 165](https://github.com/deviceplug/btleplug/issues/165) for more info, but for now it's recommended to use polling on linux (as seen in the `subscribe_notify_characteristic` example) instead of event driven device discovery (as seen in the `event_driven_discovery` example).

### macOS permissions note

To use Bluetooth on macOS Big Sur (11) or later, you need to either package your
binary into an application bundle with an `Info.plist` including
`NSBluetoothAlwaysUsageDescription`, or (for a command-line application such as
the examples included with `btleplug`) enable the Bluetooth permission for your
terminal. You can do the latter by going to _System Preferences_ → _Security &
Privacy_ → _Privacy_ → _Bluetooth_, clicking the '+' button, and selecting
'Terminal' (or iTerm or whichever terminal applicatation you use).

### Platform Feature Table

- X: Completed and released
- O: In development
- Blank: Not started

| Feature                               | Windows | MacOS | Linux                                                 |
| ------------------------------------- | ------- | ----- | ----------------------------------------------------- |
| Bring Up Adapter                      | X       | X     | X                                                     |
| Handle Multiple Adapters              |         |       | X                                                     |
| Discover Devices                      | X       | X     | X                                                     |
| └ Discover Services                   |         |       | [O](https://github.com/deviceplug/btleplug/issues/11) |
| └ Discover Characteristics            | X       | X     | X                                                     |
| └ Discover Descriptors                |         |       |                                                       |
| └ Discover Name                       | X       | X     | X                                                     |
| └ Discover Manufacturer Data          | X       | X     | X                                                     |
| └ Discover Service Data               | X       | X     | X                                                     |
| └ Discover MAC address                | X       |       | X                                                     |
| GATT Server Connect                   | X       | X     | X                                                     |
| GATT Server Connect Event             | X       | X     | X                                                     |
| GATT Server Disconnect                | X       | X     | X                                                     |
| GATT Server Disconnect Event          | X       | X     | X                                                     |
| Write to Characteristic               | X       | X     | X                                                     |
| Read from Characteristic              | X       | X     | X                                                     |
| Subscribe to Characteristic           | X       | X     | X                                                     |
| Unsubscribe from Characteristic       | X       | X     | X                                                     |
| Get Characteristic Notification Event | X       | X     | X                                                     |
| Read Descriptor                       |         |       |                                                       |
| Write Descriptor                      |         |       |                                                       |

## Library Features

#### Serialization/Deserialization

To enable implementation of serde's `Serialize` and `Deserialize` across some common types in the `api` module, use the `serde` feature.

```toml
[dependencies]
btleplug = { version = "0.4", features = ["serde"] }
```

## License

BTLEPlug is covered under a BSD 3-Clause License, with some parts from
Rumble/Blurmac covered under MIT/Apache dual license, and BSD 3-Clause
licenses, respectively. See LICENSE.md for more info and copyright
information.
