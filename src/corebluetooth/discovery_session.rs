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

use std::error::Error;
use std::sync::Arc;

use super::adapter::BluetoothAdapter;


#[derive(Clone, Debug)]
pub struct BluetoothDiscoverySession {
    // pub(crate) adapter: Arc<BluetoothAdapter>,
}

impl BluetoothDiscoverySession {
    pub fn create_session(_adapter: Arc<BluetoothAdapter>) -> Result<BluetoothDiscoverySession, Box<dyn Error>> {
        trace!("BluetoothDiscoverySession::create_session");
        Ok(BluetoothDiscoverySession {
            // adapter: adapter.clone()
        })
    }

    pub fn start_discovery(&self) -> Result<(), Box<dyn Error>> {
        trace!("BluetoothDiscoverySession::start_discovery");
        // NOTE: discovery is started by BluetoothAdapter::new to allow devices to pop up
        Ok(())
    }

    pub fn stop_discovery(&self) -> Result<(), Box<dyn Error>> {
        trace!("BluetoothDiscoverySession::stop_discovery");
        // NOTE: discovery is only stopped when BluetoothAdapter is dropped
        Ok(())
    }
}
