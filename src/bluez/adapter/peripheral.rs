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

mod dbus;
use self::dbus::OrgBluezDevice1;

use ::dbus::{
    arg::{Array, RefArg, Variant},
    blocking::{Proxy, SyncConnection},
    Path,
};
use bytes::BufMut;

use crate::{
    api::{
        AdapterManager, AddressType, BDAddr, Characteristic, CommandCallback, NotificationHandler,
        Peripheral as ApiPeripheral, PeripheralProperties, RequestCallback, UUID,
    },
    Result,
};

use std::{
    collections::{BTreeSet, HashMap},
    fmt::{self, Debug, Display, Formatter},
    sync::{
        atomic::{AtomicBool, Ordering},
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
    notification_handlers: Arc<Mutex<Vec<NotificationHandler>>>,
}

impl Peripheral {
    pub fn new(
        adapter: AdapterManager<Self>,
        connection: Arc<SyncConnection>,
        path: &Path,
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
            characteristics: characteristics,
            notification_handlers: notification_handlers,
        }
    }

    pub fn update_properties(
        &self,
        args: &::std::collections::HashMap<String, Variant<Box<dyn RefArg + 'static>>>,
    ) {
        let mut properties = self.properties.lock().unwrap();

        properties.discovery_count += 1;

        if let Some(name) = args.get("Name") {
            properties.local_name = name.as_str().map(|s| s.to_string());
        }

        // As of writing this: ManufacturerData returns a 'Variant({<manufacturer_id>: Variant([<manufacturer_data>])})'.
        // This Variant wrapped dictionary and array is difficult to navigate. So uh.. trust me, this works on my machine™.
        if let Some(manufacturer_data) = args.get("ManufacturerData") {
            let mut result = Vec::<u8>::new();
            // dbus-rs doesn't really have a dictionary API... so need to iterate two at a time and make a key-value pair.
            if let Some(mut iter) = manufacturer_data.0.as_iter() {
                loop {
                    if let (Some(id), Some(data)) = (iter.next(), iter.next()) {
                        // This API is terrible.. why can't I just get an array out, why is it all wrapped in a Variant?
                        let data: Vec<u8> = data
                            .as_iter()
                            .unwrap()
                            .next()
                            .unwrap()
                            .as_iter()
                            .unwrap()
                            .map(|b| b.as_u64().unwrap() as u8)
                            .collect();

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
            properties.address_type = address_type
                .as_str()
                .map(|address_type| AddressType::from_str(address_type).unwrap_or_default())
                .unwrap_or_default();
        }

        if let Some(rssi) = args.get("RSSI") {
            properties.tx_power_level = rssi.as_i64().map(|rssi| rssi as i8);
        }
    }

    pub fn proxy(&self) -> Proxy<&SyncConnection> {
        self.connection
            .with_proxy("org.bluez", &self.path, Duration::from_secs(5))
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
        unimplemented!()
    }

    fn command_async(
        &self,
        _characteristic: &Characteristic,
        _data: &[u8],
        _handler: Option<CommandCallback>,
    ) {
        unimplemented!()
    }

    fn command(&self, _characteristic: &Characteristic, _data: &[u8]) -> Result<()> {
        unimplemented!()
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

    fn read(&self, _characteristic: &Characteristic) -> Result<Vec<u8>> {
        unimplemented!()
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
