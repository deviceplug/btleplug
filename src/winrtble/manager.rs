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
use windows::{
    Devices::Bluetooth::BluetoothAdapter,
    Devices::Enumeration::DeviceInformation,
    core::HSTRING
};
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
        // Get the selector for Bluetooth adapters
        let selector = BluetoothAdapter::GetDeviceSelector()?;
        
        // Find all devices that match the selector
        let devices = DeviceInformation::FindAllAsyncAqsFilter(&HSTRING::from(selector))?.await?;
        
        let futures = devices
            .into_iter()
            .map(|device| async move {
                let device_id = device.Id()?;
                let bt_adapter = BluetoothAdapter::FromIdAsync(&device_id)?.await?;
                Adapter::new(bt_adapter).await
            });
        let adapters = futures::future::try_join_all(futures).await?;
        Ok(adapters)
    }
}
