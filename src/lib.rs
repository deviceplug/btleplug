// btleplug Source Code File
//
// Copyright 2020 Nonpolynomial Labs LLC. All rights reserved.
//
// Licensed under the BSD 3-Clause license. See LICENSE file in the project root
// for full license information.
//
// Some portions of this file are taken and/or modified from Rumble
// (https://github.com/mwylde/rumble), using a dual MIT/Apache License under the
// following copyright:
//
// Copyright (c) 2014 The Rust Project Developers

//! BtlePlug is a Bluetooth Low Energy (BLE) central module library for Rust. It
//! currently supports Windows 10, macOS (and possibly iOS), Linux (using BlueZ
//! sockets instead of D-Bus). Android support is coming in a future update.
//!
//! ## Usage
//!
//! An example of how to use the library to control some BLE smart lights:
//!
//! ```rust,no_run
//! extern crate btleplug;
//! extern crate rand;
//!
//! use std::thread;
//! use std::time::Duration;
//! use rand::{Rng, thread_rng};
//! #[cfg(target_os = "linux")]
//! use btleplug::bluez::{adapter::Adapter, manager::Manager};
//! #[cfg(target_os = "windows")]
//! use btleplug::winrtble::{adapter::Adapter, manager::Manager};
//! #[cfg(target_os = "macos")]
//! use btleplug::corebluetooth::{adapter::Adapter, manager::Manager};
//! use btleplug::api::{Central, Peripheral, WriteType, UUID};
//!
//! // adapter retreival works differently depending on your platform right now.
//! // API needs to be aligned.
//!
//! pub fn main() {
//!     let manager = Manager::new().unwrap();
//!
//!     // get the first bluetooth adapter
//!     let adapters = manager.adapters().unwrap();
//!     let central = adapters.into_iter().nth(0).unwrap();
//!
//!     // start scanning for devices
//!     central.start_scan().unwrap();
//!     // instead of waiting, you can use central.event_receiver() to fetch a channel and
//!     // be notified of new devices
//!     thread::sleep(Duration::from_secs(2));
//!
//!     // find the device we're interested in
//!     let light = central.peripherals().into_iter()
//!         .find(|p| p.properties().local_name.iter()
//!             .any(|name| name.contains("LEDBlue"))).unwrap();
//!
//!     // connect to the device
//!     light.connect().unwrap();
//!
//!     // discover characteristics
//!     light.discover_characteristics().unwrap();
//!
//!     // find the characteristic we want
//!     let chars = light.characteristics();
//!     let cmd_char = chars.iter().find(|c| c.uuid == UUID::B16(0xFFE9)).unwrap();
//!
//!     // dance party
//!     let mut rng = thread_rng();
//!     for _ in 0..20 {
//!         let color_cmd = vec![0x56, rng.gen(), rng.gen(), rng.gen(), 0x00, 0xF0, 0xAA];
//!         light.write(&cmd_char, &color_cmd, WriteType::WithoutResponse).unwrap();
//!         thread::sleep(Duration::from_millis(200));
//!     }
//! }
//! ```

extern crate libc;

#[cfg(target_os = "windows")]
extern crate winrt;

#[cfg(any(target_os = "macos", target_os = "ios"))]
#[macro_use]
extern crate objc;

// We won't actually use anything specifically out of this crate. However, if we
// want the CoreBluetooth code to compile, we need the objc protocols
// (specifically, the core bluetooth protocols) exposed by it.
#[cfg(any(target_os = "macos", target_os = "ios"))]
extern crate cocoa;

extern crate bytes;

#[cfg(target_os = "linux")]
extern crate enum_primitive;
extern crate num;

extern crate thiserror;

use std::result;
use std::time::Duration;

pub mod api;
#[cfg(target_os = "linux")]
pub mod bluez;
mod common;
#[cfg(any(target_os = "macos", target_os = "ios"))]
pub mod corebluetooth;
#[cfg(target_os = "windows")]
pub mod winrtble;

#[derive(Debug, thiserror::Error, Clone)]
pub enum Error {
    #[error("Permission denied")]
    PermissionDenied,

    #[error("Device not found")]
    DeviceNotFound,

    #[error("Not connected")]
    NotConnected,

    #[error("The operation is not supported: {}", _0)]
    NotSupported(String),

    #[error("Timed out after {:?}", _0)]
    TimedOut(Duration),

    #[error("{}", _0)]
    Other(String),
}

// BtlePlug Result type
pub type Result<T> = result::Result<T, Error>;
