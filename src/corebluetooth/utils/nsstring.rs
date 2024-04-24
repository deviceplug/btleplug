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

use objc2_foundation::NSString;

use super::id;

/// Convert the given `NSString` to a Rust `String`, or `None` if it is `nil`.
pub fn nsstring_to_string(nsstring: id) -> Option<String> {
    let nsstring: *const NSString = nsstring.cast();
    if let Some(nsstring) = unsafe { nsstring.as_ref() } {
        Some(nsstring.to_string())
    } else {
        None
    }
}
