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
use crate::{Result, api};
use async_trait::async_trait;
use std::future::IntoFuture;
use windows::Devices::{Bluetooth::BluetoothAdapter, Enumeration::DeviceInformation};

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
        let selector = BluetoothAdapter::GetDeviceSelector()?;
        let devices = DeviceInformation::FindAllAsyncAqsFilter(&selector)?
            .into_future()
            .await?
            .into_iter()
            .collect::<Vec<_>>();
        let mut adapters = Vec::new();
        for device in devices {
            let device_id = device.Id()?;
            let bluetooth_adapter = BluetoothAdapter::FromIdAsync(&device_id)?
                .into_future()
                .await?;
            let radio = bluetooth_adapter.GetRadioAsync()?.into_future().await?;
            adapters.push(Adapter::new(bluetooth_adapter, radio)?);
        }
        Ok(adapters)
    }
}
