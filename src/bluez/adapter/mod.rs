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
pub mod peripheral;

use super::{
    bluez_dbus::adapter::OrgBluezAdapter1, bluez_dbus::device::OrgBluezDevice1Properties,
    bluez_dbus::device::ORG_BLUEZ_DEVICE1_NAME,
    bluez_dbus::gatt_characteristic::OrgBluezGattCharacteristic1Properties,
    bluez_dbus::gatt_characteristic::ORG_BLUEZ_GATT_CHARACTERISTIC1_NAME,
    bluez_dbus::gatt_service::ORG_BLUEZ_GATT_SERVICE1_NAME, BLUEZ_DEST, DEFAULT_TIMEOUT,
};
use crate::{
    api::{AdapterManager, BDAddr, Central, CentralEvent, CharPropFlags},
    bluez::adapter::peripheral::Peripheral,
    Error, Result,
};
use dashmap::DashMap;
use dbus::{
    arg::RefArg,
    blocking::{Proxy, SyncConnection},
    channel::Token,
    message::SignalArgs,
    Path,
};
use displaydoc::Display;
use log::{debug, error, info, trace, warn};
use parking_lot::ReentrantMutex;
use static_assertions::assert_impl_all;
use std::{
    self,
    iter::Iterator,
    str::FromStr,
    sync::{mpsc::Receiver, Arc, Condvar, Mutex},
    thread::{self, JoinHandle},
    time::Duration,
};
use thiserror::Error;
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum TokenType {
    DeviceDiscovery,
    DeviceLost,
}

type ParseCharPropFlagsResult<T> = std::result::Result<T, ParseCharPropFlagsError>;

#[derive(Debug, Error, Display, Clone, PartialEq)]
pub enum ParseCharPropFlagsError {
    /// BlueZ characteristic flag "{0}" is unknown
    UnknownFlag(String),
}

impl From<ParseCharPropFlagsError> for Error {
    fn from(e: ParseCharPropFlagsError) -> Self {
        Error::Other(format!("ParseCharPropFlagsError: {}", e))
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

    should_stop: Arc<(Condvar, Mutex<bool>)>,
    thread_handle: Arc<Mutex<Option<JoinHandle<()>>>>,
}

assert_impl_all!(SyncConnection: Sync, Send);
assert_impl_all!(Adapter: Sync, Send);

impl Adapter {
    pub(crate) fn from_dbus_path(path: &Path) -> Result<Adapter> {
        let conn = Arc::new(SyncConnection::new_system()?);
        let proxy = conn.with_proxy(BLUEZ_DEST, path, DEFAULT_TIMEOUT);
        info!("DevInfo: {:?}", proxy.address()?);

        let adapter = Adapter {
            connection: conn,
            listener: Arc::new(ReentrantMutex::new(SyncConnection::new_system()?)),
            path: path.to_string(),
            manager: AdapterManager::default(),
            match_tokens: Arc::new(DashMap::new()),
            should_stop: Arc::new((Condvar::new(), Mutex::new(false))),
            thread_handle: Arc::new(Mutex::new(None)),
        };

        adapter.setup();

        Ok(adapter)
    }

    fn setup(&self) {
        use dbus::blocking::stdintf::org_freedesktop_dbus::ObjectManagerInterfacesRemoved as InterfacesRemoved;
        let mut lost_rule = InterfacesRemoved::match_rule(None, None);
        lost_rule.path = Some(Path::from("/"));

        // // Enable watching the DBus channel for new messages
        // conn.channel().set_watch_enabled(true);
        // let watcher = conn.channel().watch();
        // // watcher.
        let listener = self.listener.clone();
        let should_stop = self.should_stop.clone();

        // Spawn a new thread to process incoming DBus messages.
        // This thread gets cleaned up in the 'impl Drop for Adapter'
        *self.thread_handle.lock().unwrap() = Some(thread::spawn(move || {
            let (cvar, should_stop) = &*should_stop;
            loop {
                let (should_stop, _timeout_result) = cvar
                    .wait_timeout(should_stop.lock().unwrap(), Duration::from_millis(100))
                    .unwrap();
                if *should_stop {
                    break;
                }

                // listener is protected by a mutex. as calling `process()` when any proxied request is awaiting a reply, will cause the reply to be dropped...
                let lock = listener.lock();
                // Important! The [parking_log::ReentrantMutex] promotes fairness, that is, eventually giving
                //  other threads waiting on a lock to get the lock. Where as, Rust's std::sync::Mutex does not.
                //  See: https://docs.rs/parking_lot/0.11.1/parking_lot/type.Mutex.html#fairness
                while lock.process(Duration::from_secs(0)).unwrap() {}
            }
        }));

        let adapter = self.clone();
        self.match_tokens.insert(
            TokenType::DeviceLost,
            self.listener
                .lock()
                .add_match(lost_rule, move |args: InterfacesRemoved, _c, _msg| {
                    trace!("Received 'InterfacesRemoved' signal");
                    let path = args.object;

                    if args.interfaces.iter().any(|s| s == ORG_BLUEZ_DEVICE1_NAME) {
                        adapter.remove_device(&path).unwrap();
                    }
                    /*else if args
                        .interfaces
                        .iter()
                        .any(|s| s == ORG_BLUEZ_GATT_SERVICE1_NAME)
                    {
                        // Ignore Services that get removed, the BTLEPlug API doesn't support that
                    } else if args
                        .interfaces
                        .iter()
                        .any(|s| s == ORG_BLUEZ_GATT_CHARACTERISTIC1_NAME)
                    {
                        // Ignore Characteristics that get removed, the BTLEPlug API doesn't support that
                    }*/

                    true
                })
                .unwrap(),
        );
    }

    pub(crate) fn proxy(&self) -> Proxy<&SyncConnection> {
        self.connection
            .with_proxy(BLUEZ_DEST, &self.path, DEFAULT_TIMEOUT)
    }

    /// Get the adapter's powered state. This also indicates the appropriate connectable state of the adapter.
    pub fn is_powered(&self) -> Result<bool> {
        Ok(self.proxy().powered()?)
    }

    /// Switch an adapter on or off. This will also set the appropriate connectable state of the adapter.
    pub fn set_powered(&self, powered: bool) -> Result<()> {
        Ok(self.proxy().set_powered(powered)?)
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
        let proxy = self.connection.with_proxy(BLUEZ_DEST, "/", DEFAULT_TIMEOUT);
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
            .filter_map(|(p, i)| i.get(ORG_BLUEZ_DEVICE1_NAME).map(|d| (p, d)))
            .map(|(path, device)| {
                Ok(self.add_device(path.as_str().unwrap(), OrgBluezDevice1Properties(device))?)
            })
            .collect::<Result<()>>()?;

        trace!("Fetching known peripheral services");
        // then, objects that implement org.bluez.GattService1 as they depend on devices being known first
        adapter_objects
            .clone()
            .filter_map(|(p, i)| i.get(ORG_BLUEZ_GATT_SERVICE1_NAME).map(|a| (p, a)))
            .map(|(path, attribute)| {
                Ok(self.add_attribute(
                    path.as_str().unwrap(),
                    OrgBluezGattCharacteristic1Properties(attribute),
                )?)
            })
            .collect::<Result<()>>()?;

        trace!("Fetching known peripheral characteristics");
        // then, objects that implement org.bluez.GattCharacteristic1 as they depend on devices being known first
        adapter_objects
            .clone()
            .filter_map(|(p, i)| i.get(ORG_BLUEZ_GATT_CHARACTERISTIC1_NAME).map(|a| (p, a)))
            .map(|(path, attribute)| {
                Ok(self.add_attribute(
                    path.as_str().unwrap(),
                    OrgBluezGattCharacteristic1Properties(attribute),
                )?)
            })
            .collect::<Result<()>>()?;

        // TODO: Descriptors are nested behind characteristics, and their UUID may be used more than once.
        //       btleplug will need to refactor it's API before descriptors may be supported.
        // trace!("Fetching known peripheral descriptors");
        // adapter_objects
        //     .clone()
        //     .filter_map(|(p, i)| i.get(BLUEZ_INTERFACE_DESCRIPTOR).map(|a| (p, a)))
        //     .map(|(path, descriptor)| {
        //         Ok(self.add_attribute(path.as_str().unwrap(), descriptor)?)
        //     })
        //     .collect::<Result<()>>()?;

        Ok(())
    }

    fn remove_device(&self, path: &str) -> Result<()> {
        if let Some(address) = path
            .strip_prefix(format!("{}/dev_", self.path).as_str())
            .and_then(|p| p[..17].replace("_", ":").parse::<BDAddr>().ok())
        {
            debug!("Removing device \"{:?}\"", address);
            let listener = self.listener.lock();
            debug!("Got listener lock");
            if let Some(peripheral) = self.manager.peripheral(address) {
                peripheral.stop_listening(&*listener).unwrap()
            } else {
                error!("Device \"{:?}\" not found!", address);
            }

            self.manager.emit(CentralEvent::DeviceLost(address));
        } else {
            error!("Could not parse path {:?}", path)
        }

        Ok(())
    }

    /// Helper function to add a org.bluez.Device1 object to the adapter manager
    fn add_device(&self, path: &str, device: OrgBluezDevice1Properties) -> Result<()> {
        if let Some(address) = device.address() {
            let address: BDAddr = address.parse()?;
            // Ignore devices that are blocked, else they'll make this library a bit harder to manage
            // TODO: Should we allow blocked devices to be "discovered"?
            if device.blocked().unwrap_or(false) {
                info!("Skipping blocked device \"{:?}\"", address);
                return Ok(());
            }
            let peripheral = self.manager.peripheral(address).unwrap_or_else(|| {
                Peripheral::new(self.manager.clone(), self.connection.clone(), path, address)
            });
            peripheral.update_properties(device);
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
            error!("Could not retrieve 'Address' from DBus 'InterfaceAdded' message with interface '{}'", ORG_BLUEZ_DEVICE1_NAME);
        }

        Ok(())
    }

    fn add_attribute(
        &self,
        path: &str,
        characteristic: OrgBluezGattCharacteristic1Properties,
    ) -> Result<()> {
        // Convert "/org/bluez/hciXX/dev_XX_XX_XX_XX_XX_XX/serviceXX" into "XX:XX:XX:XX:XX:XX"
        if let Some(device_id) = path.strip_prefix(format!("{}/dev_", self.path).as_str()) {
            let device_id: BDAddr = device_id[..17].replace("_", ":").parse()?;

            if let Some(device) = self.manager.peripheral(device_id) {
                trace!("Adding characteristic \"{}\" on \"{:?}\"", path, device_id);
                let uuid: Uuid = characteristic.uuid().unwrap().parse()?;
                let flags = if let Some(flags) = characteristic.flags() {
                    flags.iter().map(|s| s.parse::<CharPropFlags>()).fold(
                        Ok(CharPropFlags::default()),
                        |a, f| {
                            if f.is_ok() {
                                Ok(f.unwrap() | a.unwrap())
                            } else {
                                f
                            }
                        },
                    )?
                } else {
                    CharPropFlags::default()
                };

                device.add_attribute(path, uuid, flags)?;
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
            let (cvar, should_stop) = &*self.should_stop;
            // mimicing a cancelation token though a ConVar<bool>
            *should_stop.lock().unwrap() = true;
            cvar.notify_all();

            handle.join().unwrap();
        }
    }
}

impl Central for Adapter {
    type Peripheral = Peripheral;

    fn event_receiver(&self) -> Option<Receiver<CentralEvent>> {
        self.manager.event_receiver()
    }

    fn filter_duplicates(&self, _enabled: bool) {
        todo!()
        // self.filter_duplicates
        // .clone()
        // .store(enabled, Ordering::Relaxed);
    }

    fn start_scan<'a>(&'a self) -> Result<()> {
        use dbus::blocking::stdintf::org_freedesktop_dbus::ObjectManagerInterfacesAdded as InterfacesAdded;

        // remove the previous token if it's still awkwardly overstaying their welcome...
        if let Some((_t, token)) = self.match_tokens.remove(&TokenType::DeviceDiscovery) {
            warn!("Removing previous match token");
            self.listener.lock().remove_match(token)?;
        }

        // TODO: Should this be invoked earlier? Do we need to rely on the application to called 'start_scan()' before fetching peripherals that may already be known to bluez?
        self.get_existing_peripherals()?;

        trace!("Starting discovery listener");
        {
            let mut discovered_rule = InterfacesAdded::match_rule(None, None);
            discovered_rule.path = Some(Path::from("/"));

            let adapter = self.clone();
            self.match_tokens.insert(
                TokenType::DeviceDiscovery,
                self.listener.lock().add_match(
                    discovered_rule,
                    move |args: InterfacesAdded, _c, _msg| {
                        trace!("Received 'InterfacesAdded' signal");
                        let path = args.object;

                        if let Some(device) =
                            OrgBluezDevice1Properties::from_interfaces(&args.interfaces)
                        {
                            adapter.add_device(&path, device).unwrap();
                        } else if let Some(service) =
                            args.interfaces.get(ORG_BLUEZ_GATT_SERVICE1_NAME)
                        {
                            adapter
                                .add_attribute(
                                    &path,
                                    OrgBluezGattCharacteristic1Properties(service),
                                )
                                .unwrap();
                        } else if let Some(characteristic) =
                            OrgBluezGattCharacteristic1Properties::from_interfaces(&args.interfaces)
                        {
                            adapter.add_attribute(&path, characteristic).unwrap();
                        }

                        true
                    },
                )?,
            );
        }

        if let Err(error) = self.proxy().start_discovery() {
            match error.name() {
                // Don't error if BlueZ has already started scanning.
                Some("org.bluez.Error.InProgress") => Ok(()),
                _ => Err(error)?,
            }
        } else {
            debug!("Starting discovery");
            Ok(())
        }
    }

    fn stop_scan(&self) -> Result<()> {
        if let Some((_t, token)) = self.match_tokens.remove(&TokenType::DeviceDiscovery) {
            trace!("Stopping discovery listener");
            self.listener.lock().remove_match(token)?;
        }

        if let Err(error) = self.proxy().stop_discovery() {
            match error.name() {
                // Don't error if BlueZ has already stopped scanning.
                Some("org.bluez.Error.InProgress") => Ok(()),
                _ => Err(error)?,
            }
        } else {
            debug!("Stopping discovery");
            Ok(())
        }
    }

    fn peripherals(&self) -> Vec<Peripheral> {
        self.manager.peripherals()
    }

    fn peripheral(&self, address: BDAddr) -> Option<Peripheral> {
        self.manager.peripheral(address)
    }

    fn active(&self, _enabled: bool) {
        todo!()
    }
}
