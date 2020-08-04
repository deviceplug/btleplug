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

use super::adapter::Adapter;
#[allow(unused_imports)]
use crate::{Error, Result};
use winrt::{
    windows::devices::radios::{Radio, RadioKind},
    RtAsyncOperation,
};

#[derive(Debug)]
pub struct Manager {}

impl Manager {
    pub fn new() -> Result<Self> {
        Ok(Self {})
    }

    pub fn adapters(&self) -> Result<Vec<Adapter>> {
        let mut result: Vec<Adapter> = vec![];
        let radios = Radio::get_radios_async()
            .unwrap()
            .blocking_get()
            .unwrap()
            .unwrap();

        for radio in &radios {
            if let Some(radio) = radio {
                if let Ok(kind) = radio.get_kind() {
                    if kind == RadioKind::Bluetooth {
                        // try create BLE adapter
                        result.push(Adapter::new());
                    }
                }
            }
        }
        return Ok(result);
    }
}
