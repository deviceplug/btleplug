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

use objc::runtime::Object;
use std::slice;
use uuid::Uuid;

use self::nsstring::nsstring_to_string;
use super::framework::ns;

pub mod core_bluetooth;
pub mod nsstring;

pub fn nsdata_to_vec(data: *mut Object) -> Vec<u8> {
    let length = ns::data_length(data);
    if length == 0 {
        return vec![];
    }
    let bytes = ns::data_bytes(data);
    unsafe { slice::from_raw_parts(bytes, length as usize).to_vec() }
}

pub fn nsuuid_to_uuid(uuid: *mut Object) -> Uuid {
    let uuid_nsstring = ns::uuid_uuidstring(uuid);
    nsstring_to_string(uuid_nsstring).unwrap().parse().unwrap()
}
