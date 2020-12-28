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

use std::{
    collections::HashMap,
    iter::Take,
    mem, result,
    slice::Iter,
    sync::{Arc, Mutex},
    time::Duration,
};

mod dbus;
use self::dbus::OrgFreedesktopDBusObjectManager;
use ::dbus::{blocking::Connection, Error as DBusError};

use nix::sys::ioctl::ioctl_param_type;

use crate::{
    bluez::{
        adapter::{Adapter, ConnectedAdapter},
        ioctl,
    },
    Error, Result,
};

#[derive(Debug, Copy)]
#[repr(C)]
pub struct HCIDevReq {
    pub dev_id: u16,
    pub dev_opt: u32,
}

impl Clone for HCIDevReq {
    fn clone(&self) -> Self {
        *self
    }
}

impl Default for HCIDevReq {
    fn default() -> Self {
        HCIDevReq {
            dev_id: 0,
            dev_opt: 0,
        }
    }
}

#[derive(Copy)]
#[repr(C)]
pub struct HCIDevListReq {
    dev_num: u16,
    dev_reqs: [HCIDevReq; 16],
}

impl HCIDevListReq {
    pub fn iter(&self) -> Take<Iter<HCIDevReq>> {
        self.dev_reqs.iter().take(self.dev_num as usize)
    }
}

impl Clone for HCIDevListReq {
    fn clone(&self) -> Self {
        *self
    }
}

impl Default for HCIDevListReq {
    fn default() -> Self {
        HCIDevListReq {
            dev_num: 16u16,
            dev_reqs: unsafe { mem::zeroed() },
        }
    }
}

/// This struct is the interface into BlueZ. It can be used to list, manage, and connect to bluetooth
/// adapters.
pub struct Manager {
    dbus_conn: Arc<Connection>,
}

impl Manager {
    /// Constructs a new manager to communicate with the BlueZ system. Only one Manager should be
    /// created by your application.
    pub fn new() -> Result<Manager> {
        let conn = Connection::new_system()?;
        Ok(Manager {
            dbus_conn: Arc::new(conn),
        })
    }

    /// Returns the list of adapters available on the system.
    pub fn adapters(&self) -> Result<Vec<Adapter>> {
        // Create a convenience proxy connection that's already namespaced to org.bluez
        let bluez = self
            .dbus_conn
            .with_proxy("org.bluez", "/", Duration::from_secs(5));

        // First, use org.freedesktop.DBus.ObjectManager to query org.bluez
        // for adapters
        let adapters = bluez
            .get_managed_objects()?
            .into_iter()
            .filter(|(_k, v)| v.keys().any(|i| i.starts_with("org.bluez.Adapter")))
            .map(|(path, _v)| {
                Adapter::from_dbus(self.dbus_conn.clone(), &path)
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(adapters)
    }

    /// Updates the state of an adapter.
    pub fn update(&self, adapter: &Adapter) -> Result<Adapter> {
        Adapter::from_dbus(&self.dbus_conn.with_proxy(
            "org.bluez",
            adapter.path,
            Duration::from_secs(5),
        ))
    }

    /// Disables an adapter.
    pub fn down(&self, adapter: &Adapter) -> Result<Adapter> {
        let ctl = self.ctl_fd.lock().unwrap();
        unsafe { ioctl::hci_dev_down(*ctl, adapter.dev_id as ioctl_param_type)? };
        Adapter::from_dev_id(*ctl, adapter.dev_id)
    }

    /// Enables an adapater.
    pub fn up(&self, adapter: &Adapter) -> Result<Adapter> {
        let ctl = self.ctl_fd.lock().unwrap();
        unsafe {
            ioctl::hci_dev_up(*ctl, adapter.dev_id as ioctl_param_type)?;
        }
        Adapter::from_dev_id(*ctl, adapter.dev_id)
    }

    /// Establishes a connection to an adapter. Returns a `ConnectedAdapter`, which is the
    /// [`Central`](../../api/trait.Central.html) implementation for BlueZ.
    pub fn connect(&self, adapter: &Adapter) -> Result<ConnectedAdapter> {
        ConnectedAdapter::new(adapter)
    }
}
