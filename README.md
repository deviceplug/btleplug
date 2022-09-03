# btleplug

[![Crates.io Version](https://img.shields.io/crates/v/btleplug)](https://crates.io/crates/btleplug)
[![docs.rs page](https://docs.rs/btleplug/badge.svg)](https://docs.rs/btleplug)
[![Crates.io Downloads](https://img.shields.io/crates/d/btleplug)](https://crates.io/crates/btleplug)
[![Crates.io License](https://img.shields.io/crates/l/btleplug)](https://crates.io/crates/btleplug)

[![Discord](https://img.shields.io/discord/738080600032018443.svg?logo=discord)](https://discord.gg/QGhMFzR)

[![Github donate button](https://img.shields.io/badge/github-donate-ff69b4.svg)](https://www.github.com/sponsors/qdot)

btleplug is an async Rust BLE library, supporting Windows 10, macOS, Linux, iOS, and Android
(including Flutter, see below for more info). 

It grew out of several earlier abandoned libraries for various platforms
([rumble](https://github.com/mwylde/rumble), [blurmac](https://github.com/servo/devices), etc...),
with the goal of building a fully cross platform library. Adding support for other platforms such as
Android is planned.

btleplug is meant to be _host/central mode only_. If you are interested in peripheral BTLE (i.e.
acting like a Bluetooth LE device instead of connecting to one), check out
[bluster](https://github.com/dfrankland/bluster/tree/master/src).

This library **DOES NOT SUPPORT BLUETOOTH 2/CLASSIC**. There are no plans to add BT2/Classic
support.

## Platform Status

- **Linux / Windows / macOS / iOS / Android**
  - Device enumeration and characteristic/services implemented and working.
  - Please file bugs and missing features if you find them.
- **WASM/WebBluetooth**
  - WebBluetooth is possible, and a PR is in, but needs review.
  - [Tracking issue here](https://github.com/deviceplug/btleplug/issues/13)
  - Please hold off on filing more issues until base implementation is
    landed.

### Platform Feature Table

- X: Completed and released
- O: In development
- Blank: Not started

| Feature                               | Windows | MacOS / iOS | Linux | Android |
| ------------------------------------- | ------- | ----------- | ----- | ------- |
| Bring Up Adapter                      | X       | X           | X     | X       |
| Handle Multiple Adapters              |         |             | X     |         |
| Discover Devices                      | X       | X           | X     | X       |
| └ Discover Services                   | X       | X           | X     | X       |
| └ Discover Characteristics            | X       | X           | X     | X       |
| └ Discover Descriptors                |         |             |       |         |
| └ Discover Name                       | X       | X           | X     | X       |
| └ Discover Manufacturer Data          | X       | X           | X     | X       |
| └ Discover Service Data               | X       | X           | X     | X       |
| └ Discover MAC address                | X       |             | X     | X       |
| GATT Server Connect                   | X       | X           | X     | X       |
| GATT Server Connect Event             | X       | X           | X     | X       |
| GATT Server Disconnect                | X       | X           | X     | X       |
| GATT Server Disconnect Event          | X       | X           | X     | X       |
| Write to Characteristic               | X       | X           | X     | X       |
| Read from Characteristic              | X       | X           | X     | X       |
| Subscribe to Characteristic           | X       | X           | X     | X       |
| Unsubscribe from Characteristic       | X       | X           | X     | X       |
| Get Characteristic Notification Event | X       | X           | X     | X       |
| Read Descriptor                       |         |             |       |         |
| Write Descriptor                      |         |             |       |         |

## Library Features

#### Serialization/Deserialization

To enable implementation of serde's `Serialize` and `Deserialize` across some common types in the `api` module, use the `serde` feature.

```toml
[dependencies]
btleplug = { version = "0.10", features = ["serde"] }
```

## Build/Installation Notes for Specific Platforms

### macOS

To use Bluetooth on macOS Big Sur (11) or later, you need to either package your
binary into an application bundle with an `Info.plist` including
`NSBluetoothAlwaysUsageDescription`, or (for a command-line application such as
the examples included with `btleplug`) enable the Bluetooth permission for your
terminal. You can do the latter by going to _System Preferences_ → _Security &
Privacy_ → _Privacy_ → _Bluetooth_, clicking the '+' button, and selecting
'Terminal' (or iTerm or whichever terminal application you use).

### Android

Due to requiring a hybrid Rust/Java build, btleplug for Android requires a somewhat complicated
setup.

Some information on performing the build is available in the [original issue for Android support in btlplug](https://github.com/deviceplug/btleplug/issues/8). 

A quick overview of the build process:

- For java, you will need the java portion of
  [jni-utils-rs](https://github.com/deviceplug/jni-utils-rs) available either in a Maven repository
  or locally (if locally, you'll need to check out btleplug and change the gradle file).
- Either build the java portion of btleplug, in the `src/droidplug/java` directory, using the
  included gradle files, and them to a Maven repo, or have the Java portion of your android app point to that as a local implementation.
- For Rust, the build should go as normal, though we recommend using `cargo-ndk` to build. Output
  the jniLibs and make sure they end up in the right place in your app.

### iOS

As the Corebluetooth implemenation is shared between macOS and iOS, btleplug on iOS should "just work", and seems to be stable. How this is built can vary based on your app setup and what language you're binding to, but sample instructions are as follows ([taken from here](https://github.com/deviceplug/btleplug/issues/12#issuecomment-1007671555)):

- Write a rust library (static) that uses btleplug and exposes an FFI API to C
- Use cbindgen to generate a C header file for that API
- Use cargo-lipo to build a universal static lib
- Drag the header file and the library into your Xcode project
- Add NSBluetoothAlwaysUsageDescription to your Info.plist file

There are also some examples in the Flutter shim listed below.

### Flutter

While we don't specifically support Flutter in this repo yet, there's a template repo available at
[https://github.com/trobanga/btleplugtest](https://github.com/trobanga/btleplugtest). This template has builds for both Android and iOS using btleplug.

## Alternative Libraries

Everyone has different bluetooth needs, so if btleplug doesn't fit yours, try these other libraries by the rust community!

- [Bluey](https://github.com/rib/bluey) - Cross Platform BLE library that takes a different API
  approach (less Bluez centric)
- [Bluer](https://crates.io/crates/bluer) - Official Rust interface for Bluez on Linux, with more
  features since it only supports one platform (we use
  [Bluez-async](https://crates.io/crates/bluez-async) internally.)
  
## License

BTLEPlug is covered under a BSD 3-Clause License, with some parts from
Rumble/Blurmac covered under MIT/Apache dual license, and BSD 3-Clause
licenses, respectively. See LICENSE.md for more info and copyright
information.
