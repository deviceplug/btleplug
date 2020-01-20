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

use nix;
use nix::errno::Errno;

use crate::{Result, Error};

fn errno_to_error(errno: Errno) -> Error {
    match errno {
        Errno::EPERM => Error::PermissionDenied,
        Errno::ENODEV => Error::DeviceNotFound,
        Errno::ENOTCONN => Error::NotConnected,
        _ => Error::Other(errno.to_string())
    }
}

impl From<nix::Error> for Error {
    fn from(e: nix::Error) -> Self {
        match e {
            nix::Error::Sys(errno) => {
                errno_to_error(errno)
            },
            _ => {
                Error::Other(e.to_string())
            }
        }
    }
}

pub fn handle_error(v: i32) -> Result<i32> {
    if v < 0 {
        debug!("got error {}", Errno::last());
        Err(errno_to_error(Errno::last()))
    } else {
        Ok(v)
    }
}
