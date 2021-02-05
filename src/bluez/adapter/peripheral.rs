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

use dbus::{
    arg::{cast, RefArg},
    blocking::{stdintf::org_freedesktop_dbus::PropertiesPropertiesChanged, Proxy, SyncConnection},
    channel::Token,
    message::{Message, SignalArgs},
    Path,
};

use crate::{
    api::{
        AdapterManager, AddressType, BDAddr, CentralEvent, CharPropFlags, Characteristic,
        NotificationHandler, Peripheral as ApiPeripheral, PeripheralProperties, ValueNotification,
        UUID,
    },
    bluez::{
        bluez_dbus::device::OrgBluezDevice1, bluez_dbus::device::OrgBluezDevice1Properties,
        AttributeType, Handle, BLUEZ_DEST, DEFAULT_TIMEOUT,
    },
    common::util::invoke_handlers,
    Error, Result,
};

use std::{
    collections::{BTreeSet, HashMap},
    fmt::{self, Debug, Display, Formatter},
    sync::{Arc, Condvar, Mutex},
    time::Instant,
};

#[derive(Clone, Debug, PartialEq, PartialOrd)]
enum PeripheralState {
    NotConnected,
    Connected,
    ServicesResolved,
}

#[derive(Clone)]
pub struct Peripheral {
    adapter: AdapterManager<Self>,
    connection: Arc<SyncConnection>,
    path: String,
    address: BDAddr,
    properties: Arc<Mutex<PeripheralProperties>>,
    characteristics: Arc<Mutex<BTreeSet<Characteristic>>>,
    attributes_map: Arc<Mutex<HashMap<u16, (String, Handle, Characteristic)>>>,
    state: Arc<(Mutex<PeripheralState>, Condvar)>,
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
        let notification_handlers = Arc::new(Mutex::new(Vec::new()));

        Peripheral {
            adapter: adapter,
            connection: connection,
            path: path.to_string(),
            address: address,
            state: Arc::new((Mutex::new(PeripheralState::NotConnected), Condvar::new())),
            properties: properties,
            attributes_map: Arc::new(Mutex::new(HashMap::new())),
            characteristics: characteristics,
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
        let path = message.path().unwrap().into_static();
        let path = path.as_str().unwrap();
        if path.starts_with(self.path.as_str()) {
            if let Ok(handle) = path.parse::<Handle>() {
                if args.changed_properties.contains_key("Value") {
                    let notification = ValueNotification {
                        handle: Some(handle.handle),
                        uuid: self
                            .attributes_map
                            .lock()
                            .unwrap()
                            .get(&handle.handle)
                            .unwrap()
                            .2
                            .uuid,
                        value: dbus::arg::prop_cast::<Vec<u8>>(&args.changed_properties, "Value")
                            .cloned()
                            .unwrap_or_default(),
                    };
                    invoke_handlers(&self.notification_handlers, &notification);
                } else if args.changed_properties.contains_key("Notifying") {
                    // TODO: Keep track of subscribed and unsubscribed characteristics?
                } else {
                    warn!(
                        "Unhandled properties changed on an attribute\n\t{:?}\n\t{:?}",
                        path, args.changed_properties
                    );
                }
            } else {
                self.update_properties(OrgBluezDevice1Properties(&args.changed_properties));
                if !args.invalidated_properties.is_empty() {
                    warn!(
                        "TODO: Got some properties to invalidate\n\t{:?}",
                        args.invalidated_properties
                    );
                }
            }
        } else {
            error!(
                "Got properties changed for {}, but does not start with {}",
                path, self.path
            );
        }

        true
    }

    pub fn listen(&self, listener: &SyncConnection) -> Result<()> {
        let peripheral = self.clone();
        let mut rule = PropertiesPropertiesChanged::match_rule(None, None);
        // For some silly lifetime reasons, we need to assign path separately...
        rule.path = Some(Path::from(self.path.clone()));
        // And also, we're interested in properties changed on all sub elements
        rule.path_is_namespace = true;
        let token =
            listener.add_match(rule, move |a, s, m| peripheral.properties_changed(a, s, m))?;
        *self.listen_token.lock().unwrap() = Some(token);

        Ok(())
    }

    pub fn stop_listening(&self, listener: &SyncConnection) -> Result<()> {
        trace!("Stop listening for events");
        let mut token = self.listen_token.lock().unwrap();
        if token.is_some() {
            listener.remove_match(token.unwrap())?;
            *token = None;
        }

        Ok(())
    }

    pub fn add_attribute(&self, path: &str, uuid: UUID, properties: CharPropFlags) -> Result<()> {
        trace!(
            "Adding attribute {} ({:?}) under {}",
            uuid,
            properties,
            path
        );
        let mut path_uuid_map = self.attributes_map.lock().unwrap();
        let handle: Handle = path.parse()?;
        // Create a placeholder attribute to store properties and uuid
        let attribute = Characteristic {
            uuid: uuid,
            value_handle: handle.handle,
            properties: properties,
            end_handle: 0,
            start_handle: 0,
        };

        path_uuid_map.insert(handle.handle, (path.to_string(), handle, attribute));
        Ok(())
    }

    fn build_characteristic_ranges(&self) -> Result<()> {
        let handles = self.attributes_map.lock().unwrap();

        let mut services = handles
            .iter()
            .filter(|(_k, (_p, h, _v))| h.typ == AttributeType::Service)
            .map(|(_h, (_p, k, v))| (k, v))
            .peekable();

        let mut result = self.characteristics.lock().unwrap();
        result.clear();

        // TODO: Verify that service attributes are returned as characteristics
        while let Some((handle, attribute)) = services.next() {
            let next = services.peek();
            result.insert(Characteristic {
                start_handle: handle.handle,
                end_handle: next.map_or(u16::MAX, |n| n.0.handle - 1),
                value_handle: handle.handle,
                properties: attribute.properties,
                uuid: attribute.uuid.clone(),
            });
        }

        let mut characteristics = handles
            .iter()
            .filter(|(_h, (_p, k, _v))| k.typ == AttributeType::Characteristic)
            .map(|(_h, (_p, k, v))| (k, v))
            .peekable();

        while let Some((handle, attribute)) = characteristics.next() {
            let next = characteristics.peek();
            result.insert(Characteristic {
                start_handle: handle.handle,
                end_handle: next.map_or(u16::MAX, |n| n.0.handle - 1),
                value_handle: handle.handle,
                properties: attribute.properties,
                uuid: attribute.uuid.clone(),
            });
        }

        Ok(())
    }

    pub fn update_properties(&self, args: OrgBluezDevice1Properties) {
        trace!("Updating peripheral properties");
        let mut properties = self.properties.lock().unwrap();

        properties.discovery_count += 1;

        if let Some(connected) = args.connected() {
            debug!(
                "Updating \"{}\" connected to \"{:?}\"",
                self.address, connected
            );
            let (ref lock, ref cvar) = *self.state;
            let mut state = lock.lock().unwrap();
            if connected {
                if *state < PeripheralState::Connected {
                    self.adapter
                        .emit(CentralEvent::DeviceConnected(self.address));
                    *state = PeripheralState::Connected;
                }
            } else {
                if *state >= PeripheralState::Connected {
                    self.adapter
                        .emit(CentralEvent::DeviceDisconnected(self.address));
                }
                *state = PeripheralState::NotConnected;
            }
            cvar.notify_all();
        }

        if let Some(name) = args.name() {
            debug!("Updating \"{}\" local name to \"{:?}\"", self.address, name);
            properties.local_name = Some(name.to_owned());
        }

        if let Some(services_resolved) = args.services_resolved() {
            if services_resolved {
                // Need to prase and figure out handle ranges for all discovered characteristics.
                self.build_characteristic_ranges().unwrap();
            }
            // All services have been discovered, time to inform anyone waiting.
            let (ref lock, ref cvar) = *self.state;
            if services_resolved {
                *lock.lock().unwrap() = PeripheralState::ServicesResolved;
            }
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
        if let Some(manufacturer_data) = args.manufacturer_data() {
            debug!(
                "Updating \"{}\" manufacturer data \"{:?}\"",
                self.address, manufacturer_data
            );
            properties.manufacturer_data = manufacturer_data
                .into_iter()
                .filter_map(|(&k, v)| {
                    if let Some(v) = cast::<Vec<u8>>(&v.0) {
                        Some((k, v.to_owned()))
                    } else {
                        warn!("Manufacturer data had wrong type: {:?}", &v.0);
                        None
                    }
                })
                .collect();
        }

        if let Some(address_type) = args.address_type() {
            let address_type = AddressType::from_str(address_type).unwrap_or_default();

            debug!(
                "Updating \"{}\" address type \"{:?}\"",
                self.address, address_type
            );

            properties.address_type = address_type;
        }

        if let Some(rssi) = args.rssi() {
            let rssi = rssi as i8;
            debug!("Updating \"{}\" RSSI \"{:?}\"", self.address, rssi);
            properties.tx_power_level = Some(rssi);
        }

        self.adapter.emit(CentralEvent::DeviceUpdated(self.address));
    }

    pub fn proxy(&self) -> Proxy<&SyncConnection> {
        self.connection
            .with_proxy(BLUEZ_DEST, &self.path, DEFAULT_TIMEOUT)
    }

    pub fn proxy_for(&self, characteristic: &Characteristic) -> Option<Proxy<&SyncConnection>> {
        let map = self.attributes_map.lock().unwrap();
        map.get(&characteristic.value_handle).map(|(path, _h, _c)| {
            self.connection
                .with_proxy(BLUEZ_DEST, path.clone(), DEFAULT_TIMEOUT)
        })
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
        *self.state.0.lock().unwrap() >= PeripheralState::Connected
    }

    fn connect(&self) -> Result<()> {
        let started = Instant::now();
        if let Err(error) = self.proxy().connect() {
            match error.name() {
                Some("org.bluez.Error.AlreadyConnected") => Ok(()),
                Some("org.bluez.Error.Failed") => {
                    error!(
                        "BlueZ Failed to connect to \"{:?}\": {}",
                        self.address,
                        error.message().unwrap()
                    );
                    Err(Error::NotConnected)
                }
                Some("org.freedesktop.DBus.Error.NoReply") => Err(Error::TimedOut(DEFAULT_TIMEOUT)),
                _ => Err(error)?,
            }
        } else {
            Ok(())
        }?;
        // For somereason, BlueZ may return an Okay result before the the device is actually connected...
        // So lets wait for the "connected" property to update to true
        let (ref lock, ref cvar) = *self.state;
        let timeout = DEFAULT_TIMEOUT - Instant::now().duration_since(started);
        // Map the result of the wait_timeout_while(...) to match our Result<()>
        cvar.wait_timeout_while(lock.lock().unwrap(), timeout, |c| {
            *c < PeripheralState::Connected
        })
        .map_err(|_| Error::TimedOut(DEFAULT_TIMEOUT))
        .map(|_| ())
    }

    fn disconnect(&self) -> Result<()> {
        Ok(self.proxy().disconnect()?)
    }

    fn discover_characteristics(&self) -> Result<Vec<Characteristic>> {
        let (ref lock, ref cvar) = *self.state;
        trace!("Waiting for all services to be resolved");
        let _guard = cvar
            .wait_while(lock.lock().unwrap(), |b| *b == PeripheralState::Connected)
            .unwrap();

        if *_guard == PeripheralState::NotConnected {
            return Err(Error::NotConnected);
        }

        debug!("All services are now resolved!");

        Ok(self
            .characteristics
            .lock()
            .unwrap()
            .clone()
            .into_iter()
            .collect())
    }

    fn command(&self, characteristic: &Characteristic, data: &[u8]) -> Result<()> {
        use crate::bluez::bluez_dbus::gatt_characteristic::OrgBluezGattCharacteristic1;
        Ok(self
            .proxy_for(&characteristic)
            .map(|p| p.write_value(Vec::from(data), HashMap::new()))
            .ok_or(Error::NotSupported("write_without_response".to_string()))??)
    }

    fn request(&self, characteristic: &Characteristic, data: &[u8]) -> Result<()> {
        self.command(characteristic, data)
    }

    fn read(&self, characteristic: &Characteristic) -> Result<Vec<u8>> {
        use crate::bluez::bluez_dbus::gatt_characteristic::OrgBluezGattCharacteristic1;
        Ok(self
            .proxy_for(&characteristic)
            .map(|p| p.read_value(HashMap::new()))
            .ok_or(Error::NotSupported("read".to_string()))??)
    }

    // Is this looking for a characteristic with a descriptor? or a service with a characteristic?
    fn read_by_type(&self, characteristic: &Characteristic, uuid: UUID) -> Result<Vec<u8>> {
        if let Some(characteristic) =
            self.attributes_map
                .lock()
                .unwrap()
                .iter()
                .find_map(|(_k, (_p, _h, c))| {
                    if c.uuid == uuid
                        && c.value_handle >= characteristic.start_handle
                        && c.value_handle <= characteristic.end_handle
                    {
                        Some(c)
                    } else {
                        None
                    }
                })
        {
            self.read(characteristic)
        } else {
            Err(Error::NotSupported("read_by_type".to_string()))
        }
    }

    fn subscribe(&self, characteristic: &Characteristic) -> Result<()> {
        use crate::bluez::bluez_dbus::gatt_characteristic::OrgBluezGattCharacteristic1;
        Ok(self
            .proxy_for(characteristic)
            .ok_or(Error::NotSupported("subscribe".to_string()))?
            .start_notify()?)
    }

    fn unsubscribe(&self, _characteristic: &Characteristic) -> Result<()> {
        use crate::bluez::bluez_dbus::gatt_characteristic::OrgBluezGattCharacteristic1;
        Ok(self
            .proxy_for(_characteristic)
            .ok_or(Error::NotSupported("unsubscribe".to_string()))?
            .stop_notify()?)
    }

    fn on_notification(&self, handler: NotificationHandler) {
        let mut list = self.notification_handlers.lock().unwrap();
        list.push(handler);
    }
}
