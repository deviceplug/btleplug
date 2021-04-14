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
use std::ffi::{CStr, CString};

use super::super::framework::{nil, ns};

/// Convert the given `NSString` to a Rust `String`, or `None` if it is `nil`.
pub fn nsstring_to_string(nsstring: *mut Object) -> Option<String> {
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

#[allow(dead_code)]
pub fn str_to_nsstring(string: &str) -> *mut Object {
    let cstring = CString::new(string).unwrap();
    ns::string(cstring.as_ptr())
}
