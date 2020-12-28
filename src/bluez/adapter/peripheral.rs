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

use crate::{
    api::{
        BDAddr, Characteristic, CommandCallback, NotificationHandler, Peripheral as ApiPeripheral,
        PeripheralProperties, RequestCallback, UUID,
    },
    Result,
};

use std::{
    collections::BTreeSet,
    fmt::{self, Debug, Display, Formatter},
    sync::{
        mpsc::{Receiver, Sender},
        Arc, Mutex,
    },
};

use super::Adapter;

#[derive(Clone)]
pub struct Peripheral {
    adapter: Adapter,
    address: BDAddr,
    properties: Arc<Mutex<PeripheralProperties>>,
    characteristics: Arc<Mutex<BTreeSet<Characteristic>>>,
    connection_tx: Arc<Mutex<Sender<u16>>>,
    connection_rx: Arc<Mutex<Receiver<u16>>>,
    notification_handlers: Arc<Mutex<Vec<NotificationHandler>>>,
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
        unimplemented!()
    }

    fn connect(&self) -> Result<()> {
        unimplemented!()
    }

    fn disconnect(&self) -> Result<()> {
        unimplemented!()
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
