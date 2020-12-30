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
// mod dbus;
mod peripheral;

use super::bluez_dbus::adapter::OrgBluezAdapter1;
use dbus::{
    arg::{RefArg, Variant},
    channel::Token,
    blocking::{Proxy, SyncConnection},
    message::MatchRule,
    Path,
};

use std::{
    self, 
    collections::HashSet, 
    sync::{
        Arc, 
        atomic::{AtomicBool, Ordering}, 
        mpsc::Receiver}, 
    thread, 
    time::Duration,
};


use crate::api::{AdapterManager, BDAddr, Central, CentralEvent};
use crate::Result;

use crate::bluez::adapter::peripheral::Peripheral;

/// Adapter represents a physical bluetooth interface in your system, for example a bluetooth
/// dongle.
#[derive(Clone)]
pub struct Adapter {
    connection: Arc<SyncConnection>,
    path: String,
    manager: AdapterManager<Peripheral>,
    match_tokens: Vec<Token>,
    should_stop: Arc<AtomicBool>,
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
            match_tokens: Vec::with_capacity(2),
            should_stop: Arc::new(AtomicBool::new(false)),
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
    
    fn filter_duplicates(&self, enabled: bool) {
        todo!()
        // self.filter_duplicates
        // .clone()
        // .store(enabled, Ordering::Relaxed);
    }
    
    fn start_scan<'a>(&'a self) -> Result<()> {
        use ::dbus::blocking::stdintf::org_freedesktop_dbus::ObjectManagerInterfacesAdded as IA;
        let connection = self.connection.clone();
        let manager = self.manager.clone();
        let token = self.connection.add_match(MatchRule::new(),  move |args: IA, _c, _msg| {
            let path = args.object;
            if !path.starts_with("/org/bluez") {
                return true
            }
            
            if let Some(device) = args.interfaces.get("org.bluez.Device1") {    
                if let Some(address) = device.get("Address") {
                    if let Some(address) = address.as_str() {
                        let address :BDAddr = address.parse().unwrap();
                        let peripheral = manager
                            .peripheral(address)
                            .unwrap_or_else(|| Peripheral::new(manager.clone(), connection.clone(), &path, address));
                        peripheral.update_properties(&device);
                        if !manager.has_peripheral(&address) {
                            manager.add_peripheral(address, peripheral);
                            manager.emit(CentralEvent::DeviceDiscovered(address));
                        } else {
                            manager.update_peripheral(address, peripheral);
                            manager.emit(CentralEvent::DeviceUpdated(address));
                        }
                    } else {
                        error!("Could not parse Bluetooth address");
                    }
                } else {
                    error!("Could not retrieve 'Address' from DBus 'InterfaceAdded' message with interface 'org.bluez.Device1'");
                }
            } else {
                debug!("Interface added to /org/bluez was not a 'org.bluez.Device1'");
            }
            
            return true;
        })?;
        
        // Create a thread to process incoming DBus messages
        let connection = self.connection.clone();
        let should_stop = self.should_stop.clone();
        thread::spawn(move ||{
            while !should_stop.load(Ordering::Relaxed){
                connection.process(Duration::from_secs(1)).unwrap();
            }
            connection.remove_match(token).unwrap();
        });
        
        Ok(self.proxy().start_discovery()?)
    }
    
    fn stop_scan(&self) -> Result<()> {
        self.should_stop.store(false, Ordering::Relaxed);
        Ok(self.proxy().stop_discovery()?)
    }

    fn peripherals(&self) -> Vec<Peripheral> {
        self.manager.peripherals()
    }

    fn peripheral(&self, address: BDAddr) -> Option<Peripheral> {
        self.manager.peripheral(address)
    }

    fn active(&self, enabled: bool) {
        todo!()
    }
}
