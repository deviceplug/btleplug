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

use super::{
    advertisement_data_type, ble::characteristic::BLECharacteristic,
    ble::descriptor::BLEDescriptor, ble::device::BLEDevice, ble::service::BLEService, utils,
};
use crate::{
    api::{
        bleuuid::{uuid_from_u16, uuid_from_u32},
        AddressType, BDAddr, CentralEvent, Characteristic, Peripheral as ApiPeripheral,
        PeripheralProperties, Service, ValueNotification, WriteType,
    },
    common::{adapter_manager::AdapterManager, util::notifications_stream_from_broadcast_receiver},
    Error, Result,
};
use async_trait::async_trait;
use dashmap::DashMap;
use futures::stream::Stream;
use log::{error, trace};
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "serde")]
use serde_cr as serde;
use std::{
    collections::{BTreeSet, HashMap, HashSet},
    convert::TryInto,
    fmt::{self, Debug, Display, Formatter},
    pin::Pin,
    sync::atomic::{AtomicBool, Ordering},
    sync::{Arc, RwLock},
};
use tokio::sync::broadcast;
use uuid::Uuid;

use std::sync::Weak;
use windows::Devices::Bluetooth::{Advertisement::*, BluetoothAddressType};

#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(crate = "serde_cr")
)]
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PeripheralId(BDAddr);

impl Display for PeripheralId {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        Display::fmt(&self.0, f)
    }
}

/// Implementation of [api::Peripheral](crate::api::Peripheral).
#[derive(Clone)]
pub struct Peripheral {
    shared: Arc<Shared>,
}

struct Shared {
    device: tokio::sync::Mutex<Option<BLEDevice>>,
    adapter: Weak<AdapterManager<Peripheral>>,
    address: BDAddr,
    connected: AtomicBool,
    ble_services: DashMap<Uuid, BLEService>,
    notifications_channel: broadcast::Sender<ValueNotification>,

    // Mutable, advertised, state...
    address_type: RwLock<Option<AddressType>>,
    local_name: RwLock<Option<String>>,
    last_tx_power_level: RwLock<Option<i16>>, // XXX: would be nice to avoid lock here!
    last_rssi: RwLock<Option<i16>>,           // XXX: would be nice to avoid lock here!
    latest_manufacturer_data: RwLock<HashMap<u16, Vec<u8>>>,
    latest_service_data: RwLock<HashMap<Uuid, Vec<u8>>>,
    services: RwLock<HashSet<Uuid>>,
}

impl Peripheral {
    pub(crate) fn new(adapter: Weak<AdapterManager<Self>>, address: BDAddr) -> Self {
        let (broadcast_sender, _) = broadcast::channel(16);
        Peripheral {
            shared: Arc::new(Shared {
                adapter: adapter,
                device: tokio::sync::Mutex::new(None),
                address: address,
                connected: AtomicBool::new(false),
                ble_services: DashMap::new(),
                notifications_channel: broadcast_sender,
                address_type: RwLock::new(None),
                local_name: RwLock::new(None),
                last_tx_power_level: RwLock::new(None),
                last_rssi: RwLock::new(None),
                latest_manufacturer_data: RwLock::new(HashMap::new()),
                latest_service_data: RwLock::new(HashMap::new()),
                services: RwLock::new(HashSet::new()),
            }),
        }
    }

    // TODO: see if the other backends can also be similarly decoupled from PeripheralProperties
    // so it can potentially be replaced by individial state getters
    fn derive_properties(&self) -> PeripheralProperties {
        PeripheralProperties {
            address: self.address(),
            address_type: *self.shared.address_type.read().unwrap(),
            local_name: self.shared.local_name.read().unwrap().clone(),
            tx_power_level: *self.shared.last_tx_power_level.read().unwrap(),
            rssi: *self.shared.last_rssi.read().unwrap(),
            manufacturer_data: self.shared.latest_manufacturer_data.read().unwrap().clone(),
            service_data: self.shared.latest_service_data.read().unwrap().clone(),
            services: self
                .shared
                .services
                .read()
                .unwrap()
                .iter()
                .map(|uuid| *uuid)
                .collect(),
        }
    }

    pub(crate) fn update_properties(&self, args: &BluetoothLEAdvertisementReceivedEventArgs) {
        let advertisement = args.Advertisement().unwrap();

        // Advertisements are cumulative: set/replace data only if it's set
        if let Ok(name) = advertisement.LocalName() {
            if !name.is_empty() {
                // XXX: we could probably also assume that we've seen the
                // advertisement before and speculatively take a read lock
                // to confirm that the name hasn't changed...

                let mut local_name_guard = self.shared.local_name.write().unwrap();
                *local_name_guard = Some(name.to_string());
            }
        }
        if let Ok(manufacturer_data) = advertisement.ManufacturerData() {
            let mut manufacturer_data_guard = self.shared.latest_manufacturer_data.write().unwrap();

            *manufacturer_data_guard = manufacturer_data
                .into_iter()
                .map(|d| {
                    let manufacturer_id = d.CompanyId().unwrap();
                    let data = utils::to_vec(&d.Data().unwrap());

                    (manufacturer_id, data)
                })
                .collect();

            // Emit event of newly received advertisement
            self.emit_event(CentralEvent::ManufacturerDataAdvertisement {
                id: self.shared.address.into(),
                manufacturer_data: manufacturer_data_guard.clone(),
            });
        }

        // The Windows Runtime API (as of 19041) does not directly expose Service Data as a friendly API (like Manufacturer Data above)
        // Instead they provide data sections for access to raw advertising data. That is processed here.
        if let Ok(data_sections) = advertisement.DataSections() {
            // See if we have any advertised service data before taking a lock to update...
            let mut found_service_data = false;
            for section in &data_sections {
                match section.DataType().unwrap() {
                    advertisement_data_type::SERVICE_DATA_16_BIT_UUID
                    | advertisement_data_type::SERVICE_DATA_32_BIT_UUID
                    | advertisement_data_type::SERVICE_DATA_128_BIT_UUID => {
                        found_service_data = true;
                        break;
                    }
                    _ => {}
                }
            }
            if found_service_data {
                let mut service_data_guard = self.shared.latest_service_data.write().unwrap();

                *service_data_guard = data_sections
                    .into_iter()
                    .filter_map(|d| {
                        let data = utils::to_vec(&d.Data().unwrap());

                        match d.DataType().unwrap() {
                            advertisement_data_type::SERVICE_DATA_16_BIT_UUID => {
                                let (uuid, data) = data.split_at(2);
                                let uuid =
                                    uuid_from_u16(u16::from_le_bytes(uuid.try_into().unwrap()));
                                Some((uuid, data.to_owned()))
                            }
                            advertisement_data_type::SERVICE_DATA_32_BIT_UUID => {
                                let (uuid, data) = data.split_at(4);
                                let uuid =
                                    uuid_from_u32(u32::from_le_bytes(uuid.try_into().unwrap()));
                                Some((uuid, data.to_owned()))
                            }
                            advertisement_data_type::SERVICE_DATA_128_BIT_UUID => {
                                let (uuid, data) = data.split_at(16);
                                let uuid = Uuid::from_slice(uuid).unwrap();
                                Some((uuid, data.to_owned()))
                            }
                            _ => None,
                        }
                    })
                    .collect();

                // Emit event of newly received advertisement
                self.emit_event(CentralEvent::ServiceDataAdvertisement {
                    id: self.shared.address.into(),
                    service_data: service_data_guard.clone(),
                });
            }
        }

        if let Ok(services) = advertisement.ServiceUuids() {
            let mut found_new_service = false;

            // Limited scope for read-only lock...
            {
                let services_guard_ro = self.shared.services.read().unwrap();

                // In all likelihood we've already seen all the advertised services before so lets
                // check to see if we can avoid taking the write lock and emitting an event...
                for uuid in &services {
                    if !services_guard_ro.contains(&utils::to_uuid(&uuid)) {
                        found_new_service = true;
                        break;
                    }
                }
            }

            if found_new_service {
                let mut services_guard = self.shared.services.write().unwrap();

                // ServicesUuids combines all the 16, 32 and 128 bit, 'complete' and 'incomplete'
                // service IDs that may be part of this advertisement into one single list with
                // a consistent (128bit) format. Considering that we don't practically know
                // whether the aggregate list is ever complete we always union the IDs with the
                // IDs already tracked.
                for uuid in services {
                    services_guard.insert(utils::to_uuid(&uuid));
                }

                self.emit_event(CentralEvent::ServicesAdvertisement {
                    id: self.shared.address.into(),
                    services: services_guard.iter().map(|uuid| *uuid).collect(),
                });
            }
        }

        if let Ok(address_type) = args.BluetoothAddressType() {
            let mut address_type_guard = self.shared.address_type.write().unwrap();
            *address_type_guard = match address_type {
                BluetoothAddressType::Public => Some(AddressType::Public),
                BluetoothAddressType::Random => Some(AddressType::Random),
                _ => None,
            };
        }

        if let Ok(tx_reference) = args.TransmitPowerLevelInDBm() {
            // IReference is (ironically) a crazy foot gun in Rust since it very easily
            // panics if you look at it wrong. Calling GetInt16(), IsNumericScalar() or Type()
            // all panic here without returning a Result as documented.
            // Value() is apparently the _right_ way to extract something from an IReference<T>...
            if let Ok(tx) = tx_reference.Value() {
                let mut tx_power_level_guard = self.shared.last_tx_power_level.write().unwrap();
                *tx_power_level_guard = Some(tx);
            }
        }
        if let Ok(rssi) = args.RawSignalStrengthInDBm() {
            let mut rssi_guard = self.shared.last_rssi.write().unwrap();
            *rssi_guard = Some(rssi);
        }
    }

    fn emit_event(&self, event: CentralEvent) {
        if let Some(manager) = self.shared.adapter.upgrade() {
            manager.emit(event);
        } else {
            trace!("Could not emit an event. AdapterManager has been dropped");
        }
    }
}

impl Display for Peripheral {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let connected = if self.shared.connected.load(Ordering::Relaxed) {
            " connected"
        } else {
            ""
        };
        write!(
            f,
            "{} {}{}",
            self.shared.address,
            self.shared
                .local_name
                .read()
                .unwrap()
                .clone()
                .unwrap_or_else(|| "(unknown)".to_string()),
            connected
        )
    }
}

impl Debug for Peripheral {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let connected = if self.shared.connected.load(Ordering::Relaxed) {
            " connected"
        } else {
            ""
        };
        let properties = self.derive_properties();
        write!(
            f,
            "{} properties: {:?}, services: {:?} {}",
            self.shared.address, properties, self.shared.ble_services, connected
        )
    }
}

#[async_trait]
impl ApiPeripheral for Peripheral {
    fn id(&self) -> PeripheralId {
        PeripheralId(self.shared.address)
    }

    /// Returns the address of the peripheral.
    fn address(&self) -> BDAddr {
        self.shared.address
    }

    /// Returns the set of properties associated with the peripheral. These may be updated over time
    /// as additional advertising reports are received.
    async fn properties(&self) -> Result<Option<PeripheralProperties>> {
        Ok(Some(self.derive_properties()))
    }

    fn services(&self) -> BTreeSet<Service> {
        self.shared
            .ble_services
            .iter()
            .map(|item| item.value().to_service())
            .collect()
    }

    /// Returns true iff we are currently connected to the device.
    async fn is_connected(&self) -> Result<bool> {
        Ok(self.shared.connected.load(Ordering::Relaxed))
    }

    /// Creates a connection to the device. This is a synchronous operation; if this method returns
    /// Ok there has been successful connection. Note that peripherals allow only one connection at
    /// a time. Operations that attempt to communicate with a device will fail until it is connected.
    async fn connect(&self) -> Result<()> {
        let shared_clone = Arc::downgrade(&self.shared);
        let adapter_clone = self.shared.adapter.clone();
        let address = self.shared.address;
        let device = BLEDevice::new(
            self.shared.address,
            Box::new(move |is_connected| {
                if let Some(shared) = shared_clone.upgrade() {
                    shared.connected.store(is_connected, Ordering::Relaxed);
                }

                if !is_connected {
                    if let Some(adapter) = adapter_clone.upgrade() {
                        adapter.emit(CentralEvent::DeviceDisconnected(address.into()));
                    }
                }
            }),
        )
        .await?;

        device.connect().await?;
        let mut d = self.shared.device.lock().await;
        *d = Some(device);
        self.shared.connected.store(true, Ordering::Relaxed);
        self.emit_event(CentralEvent::DeviceConnected(self.shared.address.into()));
        Ok(())
    }

    /// Terminates a connection to the device. This is a synchronous operation.
    async fn disconnect(&self) -> Result<()> {
        let mut device = self.shared.device.lock().await;
        *device = None;
        self.shared.connected.store(false, Ordering::Relaxed);
        self.emit_event(CentralEvent::DeviceDisconnected(self.shared.address.into()));
        Ok(())
    }

    /// Discovers all characteristics for the device. This is a synchronous operation.
    async fn discover_services(&self) -> Result<()> {
        let device = self.shared.device.lock().await;
        if let Some(ref device) = *device {
            let gatt_services = device.discover_services().await?;
            for service in &gatt_services {
                let uuid = utils::to_uuid(&service.Uuid().unwrap());
                if !self.shared.ble_services.contains_key(&uuid) {
                    match BLEDevice::get_characteristics(&service).await {
                        Ok(characteristics) => {
                            let characteristics =
                                characteristics.into_iter().map(|characteristic| async {
                                    match BLEDevice::get_characteristic_descriptors(&characteristic)
                                        .await
                                    {
                                        Ok(descriptors) => {
                                            let descriptors: HashMap<Uuid, BLEDescriptor> =
                                                descriptors
                                                    .into_iter()
                                                    .map(|descriptor| {
                                                        let descriptor =
                                                            BLEDescriptor::new(descriptor);
                                                        (descriptor.uuid(), descriptor)
                                                    })
                                                    .collect();
                                            Ok((characteristic, descriptors))
                                        }
                                        Err(e) => {
                                            error!("get_characteristic_descriptors_async {:?}", e);
                                            Err(e)
                                        }
                                    }
                                });
                            let characteristics = futures::future::try_join_all(characteristics)
                                .await?
                                .into_iter()
                                .map(|(characteristic, descriptors)| {
                                    let characteristic =
                                        BLECharacteristic::new(characteristic, descriptors);
                                    (characteristic.uuid(), characteristic)
                                })
                                .collect();

                            self.shared.ble_services.insert(
                                uuid,
                                BLEService {
                                    uuid,
                                    characteristics,
                                },
                            );
                        }
                        Err(e) => {
                            error!("get_characteristics_async {:?}", e);
                        }
                    }
                }
            }
            return Ok(());
        }
        Err(Error::NotConnected)
    }

    /// Write some data to the characteristic. Returns an error if the write couldn't be send or (in
    /// the case of a write-with-response) if the device returns an error.
    async fn write(
        &self,
        characteristic: &Characteristic,
        data: &[u8],
        write_type: WriteType,
    ) -> Result<()> {
        let ble_service = &*self
            .shared
            .ble_services
            .get(&characteristic.service_uuid)
            .ok_or_else(|| Error::NotSupported("Service not found for write".into()))?;
        let ble_characteristic = ble_service
            .characteristics
            .get(&characteristic.uuid)
            .ok_or_else(|| Error::NotSupported("Characteristic not found for write".into()))?;
        ble_characteristic.write_value(data, write_type).await
    }

    /// Enables either notify or indicate (depending on support) for the specified characteristic.
    /// This is a synchronous call.
    async fn subscribe(&self, characteristic: &Characteristic) -> Result<()> {
        let ble_service = &mut *self
            .shared
            .ble_services
            .get_mut(&characteristic.service_uuid)
            .ok_or_else(|| Error::NotSupported("Service not found for subscribe".into()))?;
        let ble_characteristic = ble_service
            .characteristics
            .get_mut(&characteristic.uuid)
            .ok_or_else(|| Error::NotSupported("Characteristic not found for subscribe".into()))?;
        let notifications_sender = self.shared.notifications_channel.clone();
        let uuid = characteristic.uuid;
        ble_characteristic
            .subscribe(Box::new(move |value| {
                let notification = ValueNotification { uuid: uuid, value };
                // Note: we ignore send errors here which may happen while there are no
                // receivers...
                let _ = notifications_sender.send(notification);
            }))
            .await
    }

    /// Disables either notify or indicate (depending on support) for the specified characteristic.
    /// This is a synchronous call.
    async fn unsubscribe(&self, characteristic: &Characteristic) -> Result<()> {
        let ble_service = &mut *self
            .shared
            .ble_services
            .get_mut(&characteristic.service_uuid)
            .ok_or_else(|| Error::NotSupported("Service not found for unsubscribe".into()))?;
        let ble_characteristic = ble_service
            .characteristics
            .get_mut(&characteristic.uuid)
            .ok_or_else(|| {
                Error::NotSupported("Characteristic not found for unsubscribe".into())
            })?;
        ble_characteristic.unsubscribe().await
    }

    async fn read(&self, characteristic: &Characteristic) -> Result<Vec<u8>> {
        let ble_service = &*self
            .shared
            .ble_services
            .get(&characteristic.service_uuid)
            .ok_or_else(|| Error::NotSupported("Service not found for read".into()))?;
        let ble_characteristic = ble_service
            .characteristics
            .get(&characteristic.uuid)
            .ok_or_else(|| Error::NotSupported("Characteristic not found for read".into()))?;
        ble_characteristic.read_value().await
    }

    async fn notifications(&self) -> Result<Pin<Box<dyn Stream<Item = ValueNotification> + Send>>> {
        let receiver = self.shared.notifications_channel.subscribe();
        Ok(notifications_stream_from_broadcast_receiver(receiver))
    }
}

impl From<BDAddr> for PeripheralId {
    fn from(address: BDAddr) -> Self {
        PeripheralId(address)
    }
}
