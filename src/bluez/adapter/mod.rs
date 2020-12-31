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
use dashmap::DashMap;
use dbus::{
    arg::{RefArg, Variant},
    blocking::{Proxy, SyncConnection},
    channel::Token,
    message::MatchRule,
    Path,
};

use std::{
    self, 
    sync::{
        Arc, 
        Mutex, 
        atomic::{AtomicBool, Ordering}, 
        mpsc::Receiver,
    }, 
    thread::{self, JoinHandle}, 
    time::Duration,
};

use crate::api::{AdapterManager, BDAddr, Central, CentralEvent};
use crate::Result;

use crate::bluez::adapter::peripheral::Peripheral;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum TokenType {
    Discovery,
}

/// Adapter represents a physical bluetooth interface in your system, for example a bluetooth
/// dongle.
#[derive(Clone)]
pub struct Adapter {
    connection: Arc<SyncConnection>,
    listener: Arc<Mutex<SyncConnection>>,
    path: String,
    manager: AdapterManager<Peripheral>,
    match_tokens: Arc<DashMap<TokenType, Token>>,

    should_stop: Arc<AtomicBool>,
    thread_handle: Arc<Mutex<Option<JoinHandle<()>>>>,
}

assert_impl_all!(SyncConnection: Sync, Send);
assert_impl_all!(Adapter: Sync, Send);

impl Adapter {
    pub(crate) fn from_dbus_path(path: &Path) -> Result<Adapter> {
        let conn = Arc::new(SyncConnection::new_system()?);
        let proxy = conn.with_proxy("org.bluez", path, Duration::from_secs(5));
        info!("DevInfo: {:?}", proxy.address()?);
        
        let adapter = Adapter {
            connection: conn,
            listener: Arc::new(Mutex::new(SyncConnection::new_system()?)),
            path: path.to_string(),
            manager: AdapterManager::new(),
            match_tokens: Arc::new(DashMap::new()),
            should_stop: Arc::new(AtomicBool::new(false)),
            thread_handle: Arc::new(Mutex::new(None)),
        };
        
        // // Enable watching the DBus channel for new messages
        // conn.channel().set_watch_enabled(true);
        // let watcher = conn.channel().watch();
        // // watcher.
        let listener = adapter.listener.clone();
        let should_stop = adapter.should_stop.clone();

        // Spawn a new thread to process incoming DBus messages.
        // This thread gets cleaned up in the 'impl Drop for Adapter'
        *adapter.thread_handle.lock().unwrap() = Some(thread::spawn(move || {
            while !should_stop.load(Ordering::Relaxed) {
                // listener is protected by a mutex. as calling `process()` when a proxy is awaiting a reply will cause the reply to be dropped...
                let l = listener.lock().unwrap();
                while l.process(Duration::from_secs(0)).unwrap() {}
                drop(l);
                thread::sleep(Duration::from_millis(100));
            }
        }));

        Ok(adapter)
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

    fn get_existing_peripherals(&self) -> Result<()> {
        use dbus::blocking::stdintf::org_freedesktop_dbus::ObjectManager;
        let proxy = self
            .connection
            .with_proxy("org.bluez", "/", Duration::from_secs(5));
        let objects = proxy.get_managed_objects()?;

        debug!("Fetching already known peripherals from \"{}\"", self.path);

        // A lot of objects get returned, we're only interested in objects that implement 'org.bluez.Device1'
        for (path, interfaces) in objects {
            if let Some(device) = interfaces.get("org.bluez.Device1") {
                let adapter_path = device.get("Adapter").unwrap().as_str().unwrap();
                if self.path.eq(adapter_path) { 
                    self.add_device(&path, device)?;
                }else{
                    debug!("Ignoring \"{:?}\", does not belong to \"{:?}\"", device.get("Address"), self.path);
                }
            }
        }

        Ok(())
    }

    /// Helper function to add a org.bluez.Device1 object to the adapter manager
    fn add_device(
        &self,
        path: &Path,
        device: &::std::collections::HashMap<String, Variant<Box<dyn RefArg + 'static>>>,
    ) -> Result<()> {
        if let Some(address) = device.get("Address") {
            if let Some(address) = address.as_str() {
                let address: BDAddr = address.parse()?;
                let peripheral = self.manager.peripheral(address).unwrap_or_else(|| {
                    Peripheral::new(
                        self.manager.clone(),
                        self.connection.clone(),
                        &path,
                        address,
                    )
                });
                peripheral.update_properties(&device);
                if !self.manager.has_peripheral(&address) {
                    info!("Adding discovered peripheral \"{}\" on \"{}\"", address, self.path);
                    self.manager.add_peripheral(address, peripheral);
                    self.manager.emit(CentralEvent::DeviceDiscovered(address));
                } else {
                    info!("Updating peripheral \"{}\"", address);
                    self.manager.update_peripheral(address, peripheral);
                    self.manager.emit(CentralEvent::DeviceUpdated(address));
                }
            } else {
                error!("Could not parse Bluetooth address");
            }
        } else {
            error!("Could not retrieve 'Address' from DBus 'InterfaceAdded' message with interface 'org.bluez.Device1'");
        }

        Ok(())
    }
}

impl Drop for Adapter {
    /// Cleans up the thread started in Adapter::new()
    fn drop(&mut self) {
        let mut thread_handle = self.thread_handle.lock().unwrap();
        let handle = std::mem::replace(&mut *thread_handle, None);
        if let Some(handle) = handle {
            self.should_stop.store(true, Ordering::Relaxed);
            handle.join().unwrap();
        }
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
        use dbus::blocking::stdintf::org_freedesktop_dbus::ObjectManagerInterfacesAdded as IA;

        if self.proxy().discovering()? {
            warn!("start_scan() already called");
            return Ok(());
        }

        // remove the previous token if it's still awkwardly overstaying their welcome...
        if let Some((_t, token)) = self.match_tokens.remove(&TokenType::Discovery) {
            warn!("Removing previous match token");
            self.listener.lock().unwrap().remove_match(token)?;
        }

        // TODO: Should this be invoked earlier? Do we need to rely on the application to called 'start_scan()' before fetching peripherals that may already be known to bluez?
        self.get_existing_peripherals()?;

        debug!("Starting listener");

        let adapter = self.clone();
        {
            self.match_tokens.insert(
                TokenType::Discovery,
                self.listener
                .lock().unwrap()
                .add_match(MatchRule::new(), move |args: IA, _c, _msg| {
                        debug!("Received 'InterfacesAdded' signal!");
                        let path = args.object;
                        if !path.starts_with("/org/bluez") {
                            debug!("Path for signal did not start with \"/org/bluez\"");
                            return true;
                        }

                        if let Some(device) = args.interfaces.get("org.bluez.Device1") {
                            adapter.add_device(&path, device).unwrap();
                        } else {
                            debug!("Interface added to /org/bluez was not a 'org.bluez.Device1'");
                        }

                        return true;
                    })?,
            );
        }

        debug!("Starting discovery");
        Ok(self.proxy().start_discovery()?)
    }

    fn stop_scan(&self) -> Result<()> {
        if let Some((_t, token)) = self.match_tokens.remove(&TokenType::Discovery){
            self.listener.lock().unwrap().remove_match(token)?;
        }
        
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
