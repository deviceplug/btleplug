// Copyright (c) 2017 Akos Kiss.
//
// Licensed under the BSD 3-Clause License
// <LICENSE.md or https://opensource.org/licenses/BSD-3-Clause>.
// This file may not be copied, modified, or distributed except
// according to those terms.

#![feature(integer_atomics)]

#[macro_use]
extern crate log;
#[macro_use]
extern crate objc;

mod adapter;
mod delegate;
mod discovery_session;
mod device;
mod gatt_service;
mod gatt_characteristic;
mod gatt_descriptor;
mod framework;
mod utils;

pub use adapter::BluetoothAdapter;
pub use discovery_session::BluetoothDiscoverySession;
pub use device::BluetoothDevice;
pub use gatt_service::BluetoothGATTService;
pub use gatt_characteristic::BluetoothGATTCharacteristic;
pub use gatt_descriptor::BluetoothGATTDescriptor;
