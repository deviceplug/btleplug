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

use super::{
    bluez_dbus::adapter::OrgBluezAdapter1, BLUEZ_DEST, BLUEZ_INTERFACE_CHARACTERISTIC,
    BLUEZ_INTERFACE_DEVICE, BLUEZ_INTERFACE_SERVICE,
};
use dashmap::DashMap;
use dbus::{
    arg::{RefArg, Variant},
    blocking::{stdintf::org_freedesktop_dbus::PropertiesPropertiesChanged, Proxy, SyncConnection},
    channel::Token,
    message::{MatchRule, Message, SignalArgs},
    Path,
};

use std::{
    self,
    collections::HashMap,
    iter::Iterator,
    str::FromStr,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::Receiver,
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use parking_lot::ReentrantMutex;

use crate::{api::UUID, Result};
use crate::{
    api::{AdapterManager, BDAddr, Central, CentralEvent, CharPropFlags},
    Error,
};

use crate::bluez::adapter::peripheral::Peripheral;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum TokenType {
    Discovery,
    PeripheralPropertyChanged,
}

type ParseCharPropFlagsResult<T> = std::result::Result<T, ParseCharPropFlagsError>;

#[derive(Debug, Fail, Clone, PartialEq)]
pub enum ParseCharPropFlagsError {
    #[fail(display = "BlueZ characteristic flag \"{}\" is unknown", _0)]
    UnknownFlag(String),
}

impl From<ParseCharPropFlagsError> for Error {
    fn from(e: ParseCharPropFlagsError) -> Self {
        Error::Other(format!("ParseUUIDError: {}", e))
    }
}

impl FromStr for CharPropFlags {
    type Err = ParseCharPropFlagsError;

    fn from_str(s: &str) -> ParseCharPropFlagsResult<Self> {
        match s {
            "broadcast" => Ok(CharPropFlags::BROADCAST),
            "read" => Ok(CharPropFlags::READ),
            "write-without-response" => Ok(CharPropFlags::WRITE_WITHOUT_RESPONSE),
            "write" => Ok(CharPropFlags::WRITE),
            "notify" => Ok(CharPropFlags::NOTIFY),
            "indicate" => Ok(CharPropFlags::INDICATE),
            "authenticated-signed-writes" => Ok(CharPropFlags::AUTHENTICATED_SIGNED_WRITES),
            "extended-properties" => Ok(CharPropFlags::EXTENDED_PROPERTIES),

            // TODO: Support these extended properties
            "reliable-write" => Ok(CharPropFlags::empty()),
            "writable-auxiliaries" => Ok(CharPropFlags::empty()),
            "encrypt-read" => Ok(CharPropFlags::empty()),
            "encrypt-write" => Ok(CharPropFlags::empty()),
            "encrypt-authenticated-read" => Ok(CharPropFlags::empty()),
            "encrypt-authenticated-write" => Ok(CharPropFlags::empty()),
            "authorize" => Ok(CharPropFlags::empty()),

            _ => Err(ParseCharPropFlagsError::UnknownFlag(s.to_string())),
        }
    }
}

/// Adapter represents a physical bluetooth interface in your system, for example a bluetooth
/// dongle.
#[derive(Clone)]
pub struct Adapter {
    connection: Arc<SyncConnection>,
    listener: Arc<ReentrantMutex<SyncConnection>>,
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
        let proxy = conn.with_proxy(BLUEZ_DEST, path, Duration::from_secs(5));
        info!("DevInfo: {:?}", proxy.address()?);

        let adapter = Adapter {
            connection: conn,
            listener: Arc::new(ReentrantMutex::new(SyncConnection::new_system()?)),
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
                let lock = listener.lock();
                while lock.process(Duration::from_secs(0)).unwrap() {}
                drop(lock);
                thread::sleep(Duration::from_millis(100));
            }
        }));

        Ok(adapter)
    }

    pub fn proxy(&self) -> Proxy<&SyncConnection> {
        self.connection
            .with_proxy(BLUEZ_DEST, &self.path, Duration::from_secs(5))
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
            .with_proxy(BLUEZ_DEST, "/", Duration::from_secs(5));
        let objects = proxy.get_managed_objects()?;

        trace!("Fetching already known peripherals from \"{}\"", self.path);

        // A lot of out of order objects get returned, and we need to add them in order
        // Let's start off by filtering out objects that belong to this adapter
        let adapter_objects = objects
            .iter()
            .filter(|(p, _i)| p.as_str().unwrap().starts_with(self.path.as_str()));

        // first, objects that implement org.bluez.Device1,
        adapter_objects
            .clone()
            .filter_map(|(p, i)| {
                if let Some(device) = i.get(BLUEZ_INTERFACE_DEVICE) {
                    Some((p, device))
                } else {
                    None
                }
            })
            .map(|(path, device)| Ok(self.add_device(path.as_str().unwrap(), device)?))
            .collect::<Result<()>>()?;

        trace!("Fetching known peripheral services");
        // then, objects that implement org.bluez.GattService1 as they depend on devices being known first
        adapter_objects
            .clone()
            .filter_map(|(p, i)| {
                if let Some(c) = i.get(BLUEZ_INTERFACE_SERVICE) {
                    Some((p, c))
                } else {
                    None
                }
            })
            .map(|(path, characteristic)| {
                Ok(self.add_characteristic(path.as_str().unwrap(), characteristic)?)
            })
            .collect::<Result<()>>()?;

        trace!("Fetching known peripheral characteristics");
        // then, objects that implement org.bluez.GattService1 as they depend on devices being known first
        adapter_objects
            .clone()
            .filter_map(|(p, i)| {
                if let Some(c) = i.get(BLUEZ_INTERFACE_CHARACTERISTIC) {
                    Some((p, c))
                } else {
                    None
                }
            })
            .map(|(path, characteristic)| {
                Ok(self.add_characteristic(path.as_str().unwrap(), characteristic)?)
            })
            .collect::<Result<()>>()?;

        // TODO: Descriptors are nested behind characteristics, and their UUID may be used more than once.
        //       btleplug will need to refactor it's API before descriptors may be supported.
        // trace!("Fetching known peripheral descriptors");
        // adapter_objects
        //     .clone()
        //     .filter_map(|(p, i)| if let Some(c) = i.get("org.bluez.GattDescriptor1") { Some((p,c)) } else { None })
        //     .map(
        //         |(path, characteristic)| Ok(self.add_characteristic(path.as_str().unwrap(), characteristic)?),
        //     )
        //     .collect::<Result<()>>()?;

        Ok(())
    }

    /// Helper function to add a org.bluez.Device1 object to the adapter manager
    fn add_device(
        &self,
        path: &str,
        device: &::std::collections::HashMap<String, Variant<Box<dyn RefArg + 'static>>>,
    ) -> Result<()> {
        if let Some(address) = device.get("Address") {
            if let Some(address) = address.as_str() {
                let address: BDAddr = address.parse()?;
                let mut peripheral = self.manager.peripheral(address).unwrap_or_else(|| {
                    Peripheral::new(self.manager.clone(), self.connection.clone(), path, address)
                });
                peripheral.update_properties(&device);
                if !self.manager.has_peripheral(&address) {
                    info!(
                        "Adding discovered peripheral \"{}\" on \"{}\"",
                        address, self.path
                    );
                    {
                        let listener = self.listener.lock();
                        peripheral.listen(&listener)?;
                        // TODO: cal peripheral.stop_listening(...) when the peripheral is removed.
                    }
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

    fn add_characteristic(
        &self,
        path: &str,
        characteristic: &::std::collections::HashMap<String, Variant<Box<dyn RefArg + 'static>>>,
    ) -> Result<()> {
        // Convert "/org/bluez/hciXX/dev_XX_XX_XX_XX_XX_XX/serviceXX" into "XX:XX:XX:XX:XX:XX"
        if let Some(device_id) = path.strip_prefix(format!("{}/dev_", self.path).as_str()) {
            let device_id: BDAddr = device_id[..17].replace("_", ":").parse()?;

            if let Some(device) = self.manager.peripheral(device_id) {
                trace!("Adding characteristic \"{}\" on \"{:?}\"", path, device_id);
                let uuid: UUID = characteristic
                    .get("UUID")
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .parse()?;
                let flags = if let Some(flags) = characteristic.get("Flags") {
                    flags
                        .0
                        .as_iter()
                        .unwrap()
                        .map(|s| s.as_str().unwrap().parse::<CharPropFlags>())
                        .fold(Ok(CharPropFlags::new()), |a, f| {
                            if f.is_ok() {
                                Ok(f.unwrap() | a.unwrap())
                            } else {
                                f
                            }
                        })?
                } else {
                    CharPropFlags::new()
                };

                device.add_characteristic(path, uuid, flags)?;
            } else {
                error!("Got a service object for an unknown device \"{:?}\"", path);
            }
        } else {
            return Err(Error::Other(
                "Invalid DBus path for characteristic".to_string(),
            ));
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
            self.listener.lock().remove_match(token)?;
        }

        // TODO: Should this be invoked earlier? Do we need to rely on the application to called 'start_scan()' before fetching peripherals that may already be known to bluez?
        self.get_existing_peripherals()?;

        trace!("Starting discovery listener");
        let adapter = self.clone();
        {
            self.match_tokens.insert(
                TokenType::Discovery,
                self.listener
                    .lock()
                    .add_match(MatchRule::new(), move |args: IA, _c, _msg| {
                        trace!("Received 'InterfacesAdded' signal");
                        let path = args.object;
                        if !path.starts_with("/org/bluez") {
                            debug!("Path for signal did not start with \"/org/bluez\"");
                            return true;
                        }

                        if let Some(device) = args.interfaces.get(BLUEZ_INTERFACE_DEVICE) {
                            adapter.add_device(&path, device).unwrap();
                        } else {
                            debug!(
                                "Interface added to /org/bluez was not a '{}'",
                                BLUEZ_INTERFACE_DEVICE
                            );
                        }

                        return true;
                    })?,
            );
        }

        debug!("Starting discovery");
        Ok(self.proxy().start_discovery()?)
    }

    fn stop_scan(&self) -> Result<()> {
        if let Some((_t, token)) = self.match_tokens.remove(&TokenType::Discovery) {
            trace!("Stopping discovery listener");
            self.listener.lock().remove_match(token)?;
        }

        debug!("Stopping discovery");
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
