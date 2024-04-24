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

use objc2::rc::Id;
use objc2::runtime::AnyObject;
use objc2_foundation::NSUUID;
use uuid::Uuid;

pub mod core_bluetooth;

pub fn nsuuid_to_uuid(uuid: &NSUUID) -> Uuid {
    uuid.UUIDString().to_string().parse().unwrap()
}

#[allow(non_camel_case_types)]
pub type id = *const objc2::runtime::AnyObject;
#[allow(non_upper_case_globals)]
pub const nil: id = std::ptr::null();
pub type StrongPtr = Id<AnyObject>;
