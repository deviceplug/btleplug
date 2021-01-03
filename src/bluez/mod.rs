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

pub mod adapter;
pub(crate) mod bluez_dbus;
pub mod manager;
mod util;


pub(crate) const BLUEZ_DEST: &str = "org.bluez";
pub(crate) const BLUEZ_INTERFACE_ADAPTER :&str = "org.bluez.Adapter1";
pub(crate) const BLUEZ_INTERFACE_DEVICE :&str = "org.bluez.Device1";
pub(crate) const BLUEZ_INTERFACE_SERVICE :&str = "org.bluez.GattService1";
pub(crate) const BLUEZ_INTERFACE_CHARACTERISTIC :&str = "org.bluez.GattCharacteristic1";