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
    advertisement_data_type, bindings, ble::characteristic::BLECharacteristic,
    ble::device::BLEDevice, utils,
};
use crate::{
    api::{
        bleuuid::{uuid_from_u16, uuid_from_u32},
        AdapterManager, AddressType, BDAddr, CentralEvent, Characteristic, NotificationHandler,
        Peripheral as ApiPeripheral, PeripheralProperties, ValueNotification, WriteType,
    },
    common::util,
    Error, Result,
};
use dashmap::DashMap;
use std::{
    collections::BTreeSet,
    convert::TryInto,
    fmt::{self, Debug, Display, Formatter},
    sync::atomic::{AtomicBool, Ordering},
    sync::{Arc, Mutex},
};
use uuid::Uuid;

use bindings::windows::devices::bluetooth::advertisement::*;

#[derive(Clone)]
pub struct Peripheral {
    device: Arc<Mutex<Option<BLEDevice>>>,
    adapter: AdapterManager<Self>,
    address: BDAddr,
    properties: Arc<Mutex<PeripheralProperties>>,
    characteristics: Arc<Mutex<BTreeSet<Characteristic>>>,
    connected: Arc<AtomicBool>,
    ble_characteristics: Arc<DashMap<Uuid, BLECharacteristic>>,
    notification_handlers: Arc<Mutex<Vec<NotificationHandler>>>,
}

impl Peripheral {
    pub(crate) fn new(adapter: AdapterManager<Self>, address: BDAddr) -> Self {
        let device = Arc::new(Mutex::new(None));
        let mut properties = PeripheralProperties::default();
        properties.address = address;
        let properties = Arc::new(Mutex::new(properties));
        let characteristics = Arc::new(Mutex::new(BTreeSet::new()));
        let connected = Arc::new(AtomicBool::new(false));
        let ble_characteristics = Arc::new(DashMap::new());
        let notification_handlers = Arc::new(Mutex::new(Vec::new()));
        Peripheral {
            device,
            adapter,
            address,
            properties,
            characteristics,
            connected,
            ble_characteristics,
            notification_handlers,
        }
    }

    pub(crate) fn update_properties(&self, args: &BluetoothLEAdvertisementReceivedEventArgs) {
        let mut properties = self.properties.lock().unwrap();
        let advertisement = args.advertisement().unwrap();

        properties.discovery_count += 1;

        // Advertisements are cumulative: set/replace data only if it's set
        if let Ok(name) = advertisement.local_name() {
            if !name.is_empty() {
                properties.local_name = Some(name.to_string());
            }
        }
        if let Ok(manufacturer_data) = advertisement.manufacturer_data() {
            properties.manufacturer_data = manufacturer_data
                .into_iter()
                .map(|d| {
                    let manufacturer_id = d.company_id().unwrap();
                    let data = utils::to_vec(&d.data().unwrap());

                    (manufacturer_id, data)
                })
                .collect();

            // Emit event of newly received advertisement
            self.adapter
                .emit(CentralEvent::ManufacturerDataAdvertisement {
                    address: self.address,
                    manufacturer_data: properties.manufacturer_data.clone(),
                });
        }

        // The Windows Runtime API (as of 19041) does not directly expose Service Data as a friendly API (like Manufacturer Data above)
        // Instead they provide data sections for access to raw advertising data. That is processed here.
        if let Ok(data_sections) = advertisement.data_sections() {
            properties.service_data = data_sections
                .into_iter()
                .filter_map(|d| {
                    let data = utils::to_vec(&d.data().unwrap());

                    match d.data_type().unwrap() {
                        advertisement_data_type::SERVICE_DATA_16_BIT_UUID => {
                            let (uuid, data) = data.split_at(2);
                            let uuid = uuid_from_u16(u16::from_le_bytes(uuid.try_into().unwrap()));
                            Some((uuid, data.to_owned()))
                        }
                        advertisement_data_type::SERVICE_DATA_32_BIT_UUID => {
                            let (uuid, data) = data.split_at(4);
                            let uuid = uuid_from_u32(u32::from_le_bytes(uuid.try_into().unwrap()));
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
            self.adapter.emit(CentralEvent::ServiceDataAdvertisement {
                address: self.address,
                service_data: properties.service_data.clone(),
            });
        }

        if let Ok(services) = advertisement.service_uuids() {
            properties.services = services
                .into_iter()
                .map(|uuid| utils::to_uuid(&uuid))
                .collect();

            self.adapter.emit(CentralEvent::ServicesAdvertisement {
                address: self.address,
                services: properties.services.clone(),
            });
        }

        // windows does not provide the address type in the advertisement event args but only in the device object
        // https://social.msdn.microsoft.com/Forums/en-US/c71d51a2-56a1-425a-9063-de44fda48766/bluetooth-address-public-or-random?forum=wdk
        properties.address_type = AddressType::default();
        properties.has_scan_response =
            args.advertisement_type().unwrap() == BluetoothLEAdvertisementType::ScanResponse;
        properties.tx_power_level = args
            .raw_signal_strength_in_dbm()
            .ok()
            .map(|rssi| rssi as i8);
    }
}

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
                .unwrap_or_else(|| "(unknown)".to_string()),
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
    /// Returns the address of the peripheral.
    fn address(&self) -> BDAddr {
        self.address
    }

    /// Returns the set of properties associated with the peripheral. These may be updated over time
    /// as additional advertising reports are received.
    fn properties(&self) -> PeripheralProperties {
        let l = self.properties.lock().unwrap();
        l.clone()
    }

    /// The set of characteristics we've discovered for this device. This will be empty until
    /// `discover_characteristics` is called.
    fn characteristics(&self) -> BTreeSet<Characteristic> {
        let l = self.characteristics.lock().unwrap();
        l.clone()
    }

    /// Returns true iff we are currently connected to the device.
    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Relaxed)
    }

    /// Creates a connection to the device. This is a synchronous operation; if this method returns
    /// Ok there has been successful connection. Note that peripherals allow only one connection at
    /// a time. Operations that attempt to communicate with a device will fail until it is connected.
    fn connect(&self) -> Result<()> {
        let connected = self.connected.clone();
        let adapter_clone = self.adapter.clone();
        let address = self.address;
        let device = BLEDevice::new(
            self.address,
            Box::new(move |is_connected| {
                connected.store(is_connected, Ordering::Relaxed);
                if !is_connected {
                    adapter_clone.emit(CentralEvent::DeviceDisconnected(address));
                }
            }),
        )?;

        device.connect()?;
        let mut d = self.device.lock().unwrap();
        *d = Some(device);
        self.adapter
            .emit(CentralEvent::DeviceConnected(self.address));
        Ok(())
    }

    /// Terminates a connection to the device. This is a synchronous operation.
    fn disconnect(&self) -> Result<()> {
        let winrt_error = |e| Error::Other(format!("{:?}", e));
        let mut device = self.device.lock().map_err(winrt_error)?;
        *device = None;
        self.adapter
            .emit(CentralEvent::DeviceDisconnected(self.address));
        Ok(())
    }

    /// Discovers all characteristics for the device. This is a synchronous operation.
    fn discover_characteristics(&self) -> Result<Vec<Characteristic>> {
        let device = self.device.lock().unwrap();
        if let Some(ref device) = *device {
            let mut characteristics_result = vec![];
            let characteristics = device.discover_characteristics()?;
            for characteristic in characteristics {
                let uuid = utils::to_uuid(&characteristic.uuid().unwrap());
                let properties =
                    utils::to_char_props(&characteristic.characteristic_properties().unwrap());
                let chara = Characteristic {
                    uuid,
                    start_handle: 0,
                    end_handle: 0,
                    value_handle: 0,
                    properties,
                };
                characteristics_result.push(chara);
                self.ble_characteristics
                    .entry(uuid)
                    .or_insert_with(|| BLECharacteristic::new(characteristic));
            }
            return Ok(characteristics_result);
        }
        Err(Error::NotConnected)
    }

    /// Write some data to the characteristic. Returns an error if the write couldn't be send or (in
    /// the case of a write-with-response) if the device returns an error.
    fn write(
        &self,
        characteristic: &Characteristic,
        data: &[u8],
        write_type: WriteType,
    ) -> Result<()> {
        if let Some(ble_characteristic) = self.ble_characteristics.get(&characteristic.uuid) {
            ble_characteristic.write_value(data, write_type)
        } else {
            Err(Error::NotSupported("write".into()))
        }
    }

    /// Sends a read-by-type request to device for the range of handles covered by the
    /// characteristic and for the specified declaration UUID. See
    /// [here](https://www.bluetooth.com/specifications/gatt/declarations) for valid UUIDs.
    /// Synchronously returns either an error or the device response.
    fn read_by_type(&self, characteristic: &Characteristic, _uuid: Uuid) -> Result<Vec<u8>> {
        if let Some(ble_characteristic) = self.ble_characteristics.get(&characteristic.uuid) {
            return ble_characteristic.read_value();
        } else {
            Err(Error::NotSupported("read_by_type".into()))
        }
    }

    /// Enables either notify or indicate (depending on support) for the specified characteristic.
    /// This is a synchronous call.
    fn subscribe(&self, characteristic: &Characteristic) -> Result<()> {
        if let Some(mut ble_characteristic) = self.ble_characteristics.get_mut(&characteristic.uuid)
        {
            let notification_handlers = self.notification_handlers.clone();
            let uuid = characteristic.uuid;
            ble_characteristic.subscribe(Box::new(move |value| {
                let notification = ValueNotification {
                    uuid: uuid,
                    handle: None,
                    value,
                };
                util::invoke_handlers(&notification_handlers, &notification);
            }))
        } else {
            Err(Error::NotSupported("subscribe".into()))
        }
    }

    /// Disables either notify or indicate (depending on support) for the specified characteristic.
    /// This is a synchronous call.
    fn unsubscribe(&self, characteristic: &Characteristic) -> Result<()> {
        if let Some(mut ble_characteristic) = self.ble_characteristics.get_mut(&characteristic.uuid)
        {
            ble_characteristic.unsubscribe()
        } else {
            Err(Error::NotSupported("unsubscribe".into()))
        }
    }

    /// Registers a handler that will be called when value notification messages are received from
    /// the device. This method should only be used after a connection has been established. Note
    /// that the handler will be called in a common thread, so it should not block.
    fn on_notification(&self, handler: NotificationHandler) {
        let mut list = self.notification_handlers.lock().unwrap();
        list.push(handler);
    }

    fn read(&self, characteristic: &Characteristic) -> Result<Vec<u8>> {
        if let Some(ble_characteristic) = self.ble_characteristics.get(&characteristic.uuid) {
            return ble_characteristic.read_value();
        } else {
            Err(Error::NotSupported("read".into()))
        }
    }
}
