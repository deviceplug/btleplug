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

use crate::Error;

impl From<dbus::Error> for Error {
    fn from(e: dbus::Error) -> Self {
        match e.name() {
            // TODO: translate other dbus errors into relevant btleplug::Error kind
            _ => Error::Other(format!(
                "{}: {}",
                e.name().unwrap_or("Unknown DBus error"),
                e.message().unwrap_or("Unknown DBus error.")
            )),
        }
    }
}
