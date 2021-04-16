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

use super::{adapter::Adapter, bindings};
use crate::{api, Result};
use async_trait::async_trait;
use bindings::Windows::Devices::Radios::{Radio, RadioKind};

/// Implementation of [api::Manager](crate::api::Manager).
#[derive(Clone, Debug)]
pub struct Manager {}

impl Manager {
    pub async fn new() -> Result<Self> {
        Ok(Self {})
    }
}

#[async_trait]
impl api::Manager for Manager {
    type Adapter = Adapter;

    async fn adapters(&self) -> Result<Vec<Adapter>> {
        let mut result: Vec<Adapter> = vec![];
        let radios = Radio::GetRadiosAsync().unwrap().await.unwrap();

        for radio in &radios {
            let kind = radio.Kind().unwrap();
            if kind == RadioKind::Bluetooth {
                result.push(Adapter::new());
            }
        }
        return Ok(result);
    }
}
