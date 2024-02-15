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
use crate::{api, Result};
use async_trait::async_trait;
use windows::Devices::Radios::{Radio, RadioKind};

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
        let radios = Radio::GetRadiosAsync()?.await?;
        Ok(radios
            .into_iter()
            .filter(|radio| radio.Kind() == Ok(RadioKind::Bluetooth))
            .map(|radio| Adapter::new(radio))
            .collect())
    }
}
