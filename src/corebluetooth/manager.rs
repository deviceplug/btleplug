// btleplug Source Code File
//
// Copyright 2020 Nonpolynomial Labs LLC. All rights reserved.
//
// Licensed under the BSD 3-Clause license. See LICENSE file in the project root
// for full license information.

use super::adapter::Adapter;
use crate::Result;

#[derive(Clone, Debug)]
pub struct Manager {}

impl Manager {
    pub async fn new() -> Result<Self> {
        Ok(Self {})
    }

    pub async fn adapters(&self) -> Result<Vec<Adapter>> {
        Ok(vec![Adapter::new().await?])
        // TODO What do we do if there is no bluetooth adapter, like on an older
        // macbook pro? Will BluetoothAdapter::init() fail?
    }
}
