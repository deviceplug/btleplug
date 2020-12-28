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

// mod acl_stream;
mod dbus;
mod peripheral;

use self::dbus::OrgBluezAdapter1;
use ::dbus::blocking::Proxy;
use ::dbus::blocking::SyncConnection;
use ::dbus::Path;

use std::{
    self,
    collections::HashSet,
    sync::{mpsc::Receiver, Arc},
    time::Duration,
};

use crate::api::{AdapterManager, BDAddr, Central, CentralEvent};
use crate::Result;

use crate::bluez::adapter::peripheral::Peripheral;

#[derive(Hash, Eq, PartialEq, Debug, Copy, Clone)]
pub enum AdapterState {
    Up,
    Init,
    Running,
    Raw,
    PScan,
    IScan,
    Inquiry,
    Auth,
    Encrypt,
}

impl AdapterState {
    // Is this really needed?
    fn from_dbus<A: OrgBluezAdapter1>(conn: &A) -> HashSet<AdapterState> {
        let mut set = HashSet::new();
        if conn.discovering().unwrap() {
            set.insert(AdapterState::IScan);
        }
        if conn.powered().unwrap() {
            set.insert(AdapterState::Up);
        }

        set
    }
}
/// Adapter represents a physical bluetooth interface in your system, for example a bluetooth
/// dongle.
#[derive(Clone)]
pub struct Adapter {
    connection: Arc<SyncConnection>,
    path: String,
    manager: AdapterManager<Peripheral>,
}

assert_impl_all!(SyncConnection: Sync, Send);
assert_impl_all!(Adapter: Sync, Send);

impl Adapter {
    pub(crate) fn from_dbus(conn: Arc<SyncConnection>, path: &Path) -> Result<Adapter> {
        let proxy = conn.with_proxy("org.bluez", path, Duration::from_secs(5));
        info!("DevInfo: {:?}", proxy.address()?);
        Ok(Adapter {
            connection: conn,
            path: path.to_string(),
            manager: AdapterManager::new(),
        })
    }

    pub fn proxy(&self) -> Proxy<&SyncConnection> {
        self.connection
            .with_proxy("org.bluez", &self.path, Duration::from_secs(5))
    }

    pub fn is_up(&self) -> Result<bool> {
        Ok(self.proxy().powered()?)
    }

    pub fn name(&self) -> Result<String> {
        Ok(self.proxy().name()?)
    }

    pub fn address(&self) -> Result<BDAddr> {
        Ok(self.proxy().address()?.parse()?)
    }

    pub fn discoverable(&self) -> Result<bool> {
        Ok(self.proxy().discoverable()?)
    }

    pub fn set_discoverable(&self, enabled: bool) -> Result<()> {
        Ok(self.proxy().set_discoverable(enabled)?)
    }
}

impl Central<Peripheral> for Adapter {
    fn event_receiver(&self) -> Option<Receiver<CentralEvent>> {
        self.manager.event_receiver()
    }

    fn active(&self, _enabled: bool) {
        unimplemented!()
    }

    fn filter_duplicates(&self, _enabled: bool) {
        unimplemented!()
    }

    fn start_scan(&self) -> Result<()> {
        unimplemented!()
    }

    fn stop_scan(&self) -> Result<()> {
        unimplemented!()
    }

    fn peripherals(&self) -> Vec<Peripheral> {
        self.manager.peripherals()
    }

    fn peripheral(&self, address: BDAddr) -> Option<Peripheral> {
        self.manager.peripheral(address)
    }
}
