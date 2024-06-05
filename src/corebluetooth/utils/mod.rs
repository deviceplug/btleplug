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

use objc2_foundation::{NSString, NSUUID};
use uuid::Uuid;

pub mod core_bluetooth;

pub fn nsuuid_to_uuid(uuid: &NSUUID) -> Uuid {
    uuid.UUIDString().to_string().parse().unwrap()
}

pub unsafe fn nsstring_to_string(nsstring: *const NSString) -> Option<String> {
    nsstring
        .as_ref()
        .and_then(|ns| CStr::from_ptr(ns.UTF8String()).to_str().ok())
        .map(String::from)
}
