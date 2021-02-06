// btleplug Source Code File
//
// Copyright 2020 Nonpolynomial Labs LLC. All rights reserved.
//
// Licensed under the BSD 3-Clause license. See LICENSE file in the project root
// for full license information.
//
// Some portions of this file are taken and/or modified from blurmac
// (https://github.com/servo/devices), using a BSD 3-Clause license under the
// following copyright:
//
// Copyright (c) 2017 Akos Kiss.
//
// Licensed under the BSD 3-Clause License
// <LICENSE.md or https://opensource.org/licenses/BSD-3-Clause>.
// This file may not be copied, modified, or distributed except
// according to those terms.

use std::ffi::CStr;

use objc::runtime::Object;

use super::framework::{cb, nil, ns};

pub mod NSStringUtils {
    use super::*;

    pub fn string_to_string(nsstring: *mut Object) -> String {
        if nsstring == nil {
            return String::from("nil");
        }
        unsafe {
            String::from(
                CStr::from_ptr(ns::string_utf8string(nsstring))
                    .to_str()
                    .unwrap(),
            )
        }
    }

    pub fn string_to_maybe_string(nsstring: *mut Object) -> Option<String> {
        if nsstring == nil {
            return None;
        }
        unsafe {
            Some(String::from(
                CStr::from_ptr(ns::string_utf8string(nsstring))
                    .to_str()
                    .unwrap(),
            ))
        }
    }
}

pub mod CoreBluetoothUtils {
    use super::*;

    pub fn uuid_to_canonical_uuid_string(cbuuid: *mut Object) -> String {
        // NOTE: CoreBluetooth tends to return uppercase UUID strings, and only 4 character long if the
        // UUID is short (16 bits). However, WebBluetooth mandates lowercase UUID strings. And Servo
        // seems to compare strings, not the binary representation.
        let uuid = NSStringUtils::string_to_string(cb::uuid_uuidstring(cbuuid));
        let long = if uuid.len() == 4 {
            format!("0000{}-0000-1000-8000-00805f9b34fb", uuid)
        } else {
            uuid
        };
        long.to_lowercase()
    }

    pub fn peripheral_debug(peripheral: *mut Object) -> String {
        if peripheral == nil {
            return String::from("nil");
        }
        let name = cb::peripheral_name(peripheral);
        let uuid = ns::uuid_uuidstring(cb::peer_identifier(peripheral));
        if name != nil {
            format!(
                "CBPeripheral({}, {})",
                NSStringUtils::string_to_string(name),
                NSStringUtils::string_to_string(uuid)
            )
        } else {
            format!("CBPeripheral({})", NSStringUtils::string_to_string(uuid))
        }
    }

    pub fn service_debug(service: *mut Object) -> String {
        if service == nil {
            return String::from("nil");
        }
        let uuid = cb::uuid_uuidstring(cb::attribute_uuid(service));
        format!("CBService({})", NSStringUtils::string_to_string(uuid))
    }

    pub fn characteristic_debug(characteristic: *mut Object) -> String {
        if characteristic == nil {
            return String::from("nil");
        }
        let uuid = cb::uuid_uuidstring(cb::attribute_uuid(characteristic));
        format!(
            "CBCharacteristic({})",
            NSStringUtils::string_to_string(uuid)
        )
    }
}
