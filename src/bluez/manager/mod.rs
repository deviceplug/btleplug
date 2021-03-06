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

use super::{bluez_dbus::adapter::ORG_BLUEZ_ADAPTER1_NAME, BLUEZ_DEST, DEFAULT_TIMEOUT};
use crate::{bluez::adapter::Adapter, Result};
use dbus::blocking::{stdintf::org_freedesktop_dbus::ObjectManager, SyncConnection};
use static_assertions::assert_impl_all;
use std::sync::Arc;

/// This struct is the interface into BlueZ. It can be used to list, manage, and connect to bluetooth
/// adapters.
#[derive(Clone)]
pub struct Manager {
    dbus_conn: Arc<SyncConnection>,
}
assert_impl_all!(Manager: Sync, Send);

impl Manager {
    /// Constructs a new manager to communicate with the BlueZ system. Only one Manager should be
    /// created by your application.
    pub fn new() -> Result<Manager> {
        Ok(Manager {
            dbus_conn: Arc::new(SyncConnection::new_system()?),
        })
    }

    /// Returns the list of adapters available on the system.
    pub fn adapters(&self) -> Result<Vec<Adapter>> {
        // Create a convenience proxy connection that's already namespaced to org.bluez
        let bluez = self.dbus_conn.with_proxy(BLUEZ_DEST, "/", DEFAULT_TIMEOUT);

        // First, use org.freedesktop.DBus.ObjectManager to query org.bluez
        // for adapters
        let adapters = bluez
            .get_managed_objects()?
            .into_iter()
            .filter(|(_k, v)| v.keys().any(|i| i.starts_with(ORG_BLUEZ_ADAPTER1_NAME)))
            .map(|(path, _v)| Adapter::from_dbus_path(&path))
            .collect::<Result<Vec<_>>>()?;

        Ok(adapters)
    }
}
