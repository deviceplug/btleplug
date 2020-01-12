// btleplug Source Code File
//
// Copyright 2020 Nonpolynomial Labs LLC. All rights reserved.
//
// Licensed under the BSD 3-Clause license. See LICENSE file in the project root
// for full license information.

use super::adapter::Adapter;
use ::Result;
use ::Error;

pub struct Manager {
}

impl Manager {
    pub fn new() -> Self {
        Self {}
    }

    pub fn adapters(&self) -> Result<Adapter> {

        Ok(Adapter::new())
        // TODO What do we do if there is no bluetooth adapter, like on an older
        // macbook pro? Will BluetoothAdapter::init() fail?
    }
}
