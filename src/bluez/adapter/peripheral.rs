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

use crate::{Error, api::CharPropFlags, bluez::{BLUEZ_DEST, bluez_dbus::device::OrgBluezDevice1}};
use bimap::{BiHashMap, Overwritten};
use dashmap::DashMap;
use dbus::{
    arg::{Array, RefArg, Variant},
    blocking::{stdintf::org_freedesktop_dbus::PropertiesPropertiesChanged, Proxy, SyncConnection},
    channel::Token,
    message::{MatchRule, Message, SignalArgs},
    Path,
};

use bytes::BufMut;

use crate::{
    api::{
        AdapterManager, AddressType, BDAddr, Characteristic, CommandCallback, NotificationHandler,
        Peripheral as ApiPeripheral, PeripheralProperties, RequestCallback, UUID,
    },
    bluez::util,
    Result,
};

use std::{
    collections::{BTreeSet, HashMap},
    fmt::{self, Debug, Display, Formatter},
    sync::{
        atomic::{AtomicBool, Ordering},
        Condvar,
        mpsc::{Receiver, Sender},
        Arc, Mutex,
    },
    time::Duration,
};

#[derive(Clone)]
pub struct Peripheral {
    adapter: AdapterManager<Self>,
    connection: Arc<SyncConnection>,
    path: String,
    address: BDAddr,
    connected: Arc<AtomicBool>,
    properties: Arc<Mutex<PeripheralProperties>>,
    characteristics: Arc<Mutex<BTreeSet<Characteristic>>>,
    characteristics_map: Arc<Mutex<BiHashMap<String, UUID>>>,
    characteristics_discovered_wait: Arc<(Mutex<bool>, Condvar)>,
    notification_handlers: Arc<Mutex<Vec<NotificationHandler>>>,
    listen_token: Arc<Mutex<Option<Token>>>,
}

impl Peripheral {
    pub fn new(
        adapter: AdapterManager<Self>,
        connection: Arc<SyncConnection>,
        path: &str,
        address: BDAddr,
    ) -> Self {
        let mut properties = PeripheralProperties::default();
        properties.address = address;
        let properties = Arc::new(Mutex::new(properties));
        let characteristics = Arc::new(Mutex::new(BTreeSet::new()));
        let connected = Arc::new(AtomicBool::new(false));
        let notification_handlers = Arc::new(Mutex::new(Vec::new()));

        Peripheral {
            adapter: adapter,
            connection: connection,
            path: path.to_string(),
            address: address,
            connected: connected,
            properties: properties,
            characteristics_map: Arc::new(Mutex::new(BiHashMap::new())),
            characteristics: characteristics,
            characteristics_discovered_wait: Arc::new((Mutex::new(false),Condvar::new())),
            notification_handlers: notification_handlers,
            listen_token: Arc::new(Mutex::new(None)),
        }
    }

    pub fn properties_changed(
        &self,
        args: PropertiesPropertiesChanged,
        _connection: &SyncConnection,
        message: &Message,
    ) -> bool {
        assert_eq!(
            self.path,
            message.path().unwrap().into_static().as_str().unwrap(),
            "Message path did not match!"
        );

        self.update_properties(&args.changed_properties);
        if !args.invalidated_properties.is_empty() {
            warn!(
                "TODO: Got some properties to invalidate!\n\t{:?}",
                args.invalidated_properties
            );
        }

        true
    }

    pub fn listen(&mut self, listener: &SyncConnection) -> Result<()> {
        let peripheral = self.clone();
        let mut rule = PropertiesPropertiesChanged::match_rule(None, None);
        // Due to weird lifetime decisions, it was easier to set rule.path, than to pass it in as an argument...
        rule.path = Some(Path::new(&self.path).unwrap());
        let token =
            listener.add_match(rule, move |a, s, m| peripheral.properties_changed(a, s, m))?;
        *self.listen_token.lock().unwrap() = Some(token);

        Ok(())
    }

    pub fn stop_listening(&mut self, listener: &SyncConnection) -> Result<()> {
        let mut token = self.listen_token.lock().unwrap();
        if token.is_some() {
            listener.remove_match(token.unwrap())?;
            *token = None;
        }

        Ok(())
    }

    pub fn add_characteristic(
        &self,
        path: &str,
        characteristic: UUID,
        properties: CharPropFlags,
    ) -> Result<()> {
        trace!(
            "Adding characteristic {} ({:?}) under {}",
            characteristic, properties, path
        );
        let mut path_uuid_map = self.characteristics_map.lock().unwrap();

        // Insert the DBus-Path and UUID pair. neither values should already exist.
        let result = path_uuid_map.insert(path.to_string(), characteristic);
        match result {
            Overwritten::Left(old_path, old_characteristic) => error!(
                "Found (and replaced) existing DBus characteristic mapping!\n\tOld: {} -> {}\n\tNew: {} -> {}",
                old_path, old_characteristic,
                path, characteristic,
            ),
            Overwritten::Right(old_path, old_characteristic) => error!(
                "Found (and replaced) existing DBus characteristic mapping!\n\tOld: {} -> {}\n\tNew: {} -> {}",
                old_path, old_characteristic,
                path, characteristic,
            ),
            Overwritten::Both((old_path, new_characteristic), (new_path, old_characteristic)) => error!(
                "Found (and replaced) two existing DBus characteristic mapping!\n\t First: {} -> {}\n\tSecond: {} -> {}\n\t   New: {} -> {}",
                old_path, new_characteristic,
                new_path, old_characteristic,
                path, characteristic,
            ),
            _ => {},
        }

        // DBus doesn't directly expose "handles" for devices, just a DBus path that indirectly contains them.
        // It is not worth the effort to parse and figure out ranges...
        self.characteristics.lock().unwrap().insert(Characteristic {
            start_handle: 0,
            end_handle: 0,
            value_handle: 0,
            uuid: characteristic,
            properties: properties,
        });

        Ok(())
    }

    pub fn update_properties(
        &self,
        args: &::std::collections::HashMap<String, Variant<Box<dyn RefArg + 'static>>>,
    ) {
        trace!("Updating peripheral properties");
        let mut properties = self.properties.lock().unwrap();

        properties.discovery_count += 1;

        if let Some(connected) = args.get("Connected") {
            debug!("Updating \"{}\" connected to \"{:?}\"", self.address, connected.0);
            self.connected
                .store(connected.0.as_u64().unwrap() > 0, Ordering::Relaxed);
        }

        if let Some(name) = args.get("Name") {
            debug!("Updating \"{}\" local name to \"{:?}\"", self.address, name);
            properties.local_name = name.as_str().map(|s| s.to_string());
        }

        if let Some(services_resolved) = args.get("ServicesResolved") {
            // All services have been discovered, time to inform anyone waiting.
            let (lock, cvar) = &*self.characteristics_discovered_wait;
            *lock.lock().unwrap() = services_resolved.0.as_u64().unwrap() > 0;
            cvar.notify_all();
        }

        // if let Some(services) = args.get("ServiceData") {
        //     debug!("Updating services to \"{:?}\"", services);

        //     if let Some(mut iter) = services.0.as_iter() {
        //         loop {
        //             if let (Some(uuid), ())
        //         }
        //     }
        // }

        // As of writing this: ManufacturerData returns a 'Variant({<manufacturer_id>: Variant([<manufacturer_data>])})'.
        // This Variant wrapped dictionary and array is difficult to navigate. So uh.. trust me, this works on my machine™.
        if let Some(manufacturer_data) = args.get("ManufacturerData") {
            debug!("Updating \"{}\" manufacturer data \"{:?}\"", self.address, manufacturer_data);
            let mut result = Vec::<u8>::new();
            // dbus-rs doesn't really have a dictionary API... so need to iterate two at a time and make a key-value pair.
            if let Some(mut iter) = manufacturer_data.0.as_iter() {
                loop {
                    if let (Some(id), Some(data)) = (iter.next(), iter.next()) {
                        // This API is terrible.. why can't I just get an array out, why is it all wrapped in a Variant?
                        let data: Vec<u8> = data
                            .as_iter() // 🎶 The Variant is connected to the
                            .unwrap() // Array type!
                            .next() // The Array type is connected to the
                            .unwrap() // Array of integers!
                            .as_iter() // Lets convert the
                            .unwrap() // integers to a
                            .map(|b| b.as_u64().unwrap() as u8) // array of bytes...
                            .collect(); // I got too lazy to make it rhyme... 🎶

                        result.put_u16_le(id.as_u64().map(|v| v as u16).unwrap());
                        result.extend(data);
                    } else {
                        break;
                    }
                }
            }
            // 🎉
            properties.manufacturer_data = Some(result);
        }

        if let Some(address_type) = args.get("AddressType") {
            debug!("Updating \"{}\" address type \"{:?}\"", self.address, address_type);
            properties.address_type = address_type
                .as_str()
                .map(|address_type| AddressType::from_str(address_type).unwrap_or_default())
                .unwrap_or_default();
        }

        if let Some(rssi) = args.get("RSSI") {
            debug!("Updating \"{}\" RSSI \"{:?}\"", self.address, rssi);
            properties.tx_power_level = rssi.as_i64().map(|rssi| rssi as i8);
        }
    }

    pub fn proxy(&self) -> Proxy<&SyncConnection> {
        self.connection
            .with_proxy(BLUEZ_DEST, &self.path, Duration::from_secs(30))
    }

    pub fn proxy_for(&self, characteristic: &UUID) -> Option<Proxy<&SyncConnection>> {
        self.characteristics_map
            .lock().unwrap()
            .get_by_right(characteristic)
            .map(|path| self.connection.with_proxy(BLUEZ_DEST, path.clone(), Duration::from_secs(30)))
    }
}

assert_impl_all!(Peripheral: Sync, Send);

impl Display for Peripheral {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let connected = if self.is_connected() {
            " connected"
        } else {
            ""
        };
        let properties = self.properties.lock().unwrap();
        write!(
            f,
            "{} {}{}",
            self.address,
            properties
                .local_name
                .clone()
                .unwrap_or("(unknown)".to_string()),
            connected
        )
    }
}

impl Debug for Peripheral {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let connected = if self.is_connected() {
            " connected"
        } else {
            ""
        };
        let properties = self.properties.lock().unwrap();
        let characteristics = self.characteristics.lock().unwrap();
        write!(
            f,
            "{} properties: {:?}, characteristics: {:?} {}",
            self.address, *properties, *characteristics, connected
        )
    }
}

impl ApiPeripheral for Peripheral {
    fn address(&self) -> BDAddr {
        self.address.clone()
    }

    fn properties(&self) -> PeripheralProperties {
        let l = self.properties.lock().unwrap();
        l.clone()
    }

    fn characteristics(&self) -> BTreeSet<Characteristic> {
        let l = self.characteristics.lock().unwrap();
        l.clone()
    }

    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Relaxed)
    }

    fn connect(&self) -> Result<()> {
        Ok(self.proxy().connect()?)
    }

    fn disconnect(&self) -> Result<()> {
        Ok(self.proxy().disconnect()?)
    }

    fn discover_characteristics(&self) -> Result<Vec<Characteristic>> {
        self.discover_characteristics_in_range(0x0001, 0xFFFF)
    }

    fn discover_characteristics_in_range(
        &self,
        _start: u16,
        _end: u16,
    ) -> Result<Vec<Characteristic>> {
        let (lock, cvar) = &*self.characteristics_discovered_wait;
        let has_services = lock.lock().unwrap();
        if !*has_services {
            let _guard = cvar.wait_while(has_services, |b| !*b).unwrap();
        }

        Ok(self.characteristics.lock().unwrap().clone().into_iter().collect())
    }

    fn command_async(
        &self,
        _characteristic: &Characteristic,
        _data: &[u8],
        _handler: Option<CommandCallback>,
    ) {
        unimplemented!()
    }

    fn command(&self, characteristic: &Characteristic, _data: &[u8]) -> Result<()> {
        use crate::bluez::bluez_dbus::gatt_characteristic::OrgBluezGattCharacteristic1;
        Ok(self.proxy_for(&characteristic.uuid)
            .map(|p| p.write_value(Vec::from(_data), HashMap::new()))
            .ok_or(Error::DeviceNotFound)??)
    }

    fn request_async(
        &self,
        _characteristic: &Characteristic,
        _data: &[u8],
        _handler: Option<RequestCallback>,
    ) {
        unimplemented!()
    }

    fn request(&self, _characteristic: &Characteristic, _data: &[u8]) -> Result<Vec<u8>> {
        unimplemented!()
    }

    fn read_async(&self, _characteristic: &Characteristic, _handler: Option<RequestCallback>) {
        unimplemented!()
    }

    fn read(&self, characteristic: &Characteristic) -> Result<Vec<u8>> {
        use crate::bluez::bluez_dbus::gatt_characteristic::OrgBluezGattCharacteristic1;
        Ok(self.proxy_for(&characteristic.uuid)
            .map(|p| p.read_value(HashMap::new()))
            .ok_or(Error::DeviceNotFound)??)
    }

    fn read_by_type_async(
        &self,
        _characteristic: &Characteristic,
        _uuid: UUID,
        _handler: Option<RequestCallback>,
    ) {
        unimplemented!()
    }

    fn read_by_type(&self, _characteristic: &Characteristic, _uuid: UUID) -> Result<Vec<u8>> {
        unimplemented!()
    }

    fn subscribe(&self, _characteristic: &Characteristic) -> Result<()> {
        unimplemented!()
    }

    fn unsubscribe(&self, _characteristic: &Characteristic) -> Result<()> {
        unimplemented!()
    }

    fn on_notification(&self, _handler: NotificationHandler) {
        unimplemented!()
    }
}
