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

//! The `api` module contains the traits and types which make up btleplug's API. These traits have a
//! different implementation for each supported platform, but only one implementation can be found
//! on any given platform. These implementations are in the [`platform`](crate::platform) module.
//!
//! You will may want to import both the traits and their implementations, like:
//! ```
//! use btleplug::api::{Central, Manager as _, Peripheral as _};
//! use btleplug::platform::{Adapter, Manager, Peripheral};
//! ```

pub(crate) mod bdaddr;
pub mod bleuuid;

use crate::Result;
use async_trait::async_trait;
use bitflags::bitflags;
use futures::stream::Stream;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "serde")]
use serde_cr as serde;
use std::{
    collections::{BTreeSet, HashMap},
    fmt::{self, Debug, Display, Formatter},
    pin::Pin,
};
use uuid::Uuid;

pub use self::bdaddr::{BDAddr, ParseBDAddrError};

use crate::platform::PeripheralId;

#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(crate = "serde_cr")
)]
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum AddressType {
    Random,
    Public,
}

impl Default for AddressType {
    fn default() -> Self {
        AddressType::Public
    }
}

impl AddressType {
    pub fn from_str(v: &str) -> Option<AddressType> {
        match v {
            "public" => Some(AddressType::Public),
            "random" => Some(AddressType::Random),
            _ => None,
        }
    }

    pub fn from_u8(v: u8) -> Option<AddressType> {
        match v {
            1 => Some(AddressType::Public),
            2 => Some(AddressType::Random),
            _ => None,
        }
    }

    pub fn num(&self) -> u8 {
        match *self {
            AddressType::Public => 1,
            AddressType::Random => 2,
        }
    }
}

/// A notification sent from a peripheral due to a change in a value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValueNotification {
    /// UUID of the characteristic that fired the notification.
    pub uuid: Uuid,
    /// The new value of the characteristic.
    pub value: Vec<u8>,
}

bitflags! {
    /// A set of properties that indicate what operations are supported by a Characteristic.
    pub struct CharPropFlags: u8 {
        const BROADCAST = 0x01;
        const READ = 0x02;
        const WRITE_WITHOUT_RESPONSE = 0x04;
        const WRITE = 0x08;
        const NOTIFY = 0x10;
        const INDICATE = 0x20;
        const AUTHENTICATED_SIGNED_WRITES = 0x40;
        const EXTENDED_PROPERTIES = 0x80;
    }
}

impl Default for CharPropFlags {
    fn default() -> Self {
        Self { bits: 0 }
    }
}

/// A GATT service. Services are groups of characteristics, which may be standard or
/// device-specific.
#[derive(Debug, Ord, PartialOrd, Eq, PartialEq, Clone)]
pub struct Service {
    /// The UUID for this service.
    pub uuid: Uuid,
    /// Whether this is a primary service.
    pub primary: bool,
    /// The characteristics of this service.
    pub characteristics: BTreeSet<Characteristic>,
}

/// A Bluetooth characteristic. Characteristics are the main way you will interact with other
/// bluetooth devices. Characteristics are identified by a UUID which may be standardized
/// (like 0x2803, which identifies a characteristic for reading heart rate measurements) but more
/// often are specific to a particular device. The standard set of characteristics can be found
/// [here](https://www.bluetooth.com/specifications/gatt/characteristics).
///
/// A characteristic may be interacted with in various ways depending on its properties. You may be
/// able to write to it, read from it, set its notify or indicate status, or send a command to it.
#[derive(Debug, Ord, PartialOrd, Eq, PartialEq, Clone)]
pub struct Characteristic {
    /// The UUID for this characteristic. This uniquely identifies its behavior.
    pub uuid: Uuid,
    /// The UUID of the service this characteristic belongs to.
    pub service_uuid: Uuid,
    /// The set of properties for this characteristic, which indicate what functionality it
    /// supports. If you attempt an operation that is not supported by the characteristics (for
    /// example setting notify on one without the NOTIFY flag), that operation will fail.
    pub properties: CharPropFlags,
    /// The descriptors of this characteristic.
    pub descriptors: BTreeSet<Descriptor>,
}

impl Display for Characteristic {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(
            f,
            "uuid: {:?}, char properties: {:?}",
            self.uuid, self.properties
        )
    }
}

/// Add doc
#[derive(Debug, Ord, PartialOrd, Eq, PartialEq, Clone)]
pub struct Descriptor {
    /// The UUID for this descriptor. This uniquely identifies its behavior.
    pub uuid: Uuid,
    /// The UUID of the service this descriptor belongs to.
    pub service_uuid: Uuid,
    /// The UUID of the characteristic this descriptor belongs to.
    pub characteristic_uuid: Uuid,
}

impl Display for Descriptor {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "uuid: {:?}", self.uuid)
    }
}

/// The properties of this peripheral, as determined by the advertising reports we've received for
/// it.
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(crate = "serde_cr")
)]
#[derive(Debug, Default, Clone)]
pub struct PeripheralProperties {
    /// The address of this peripheral
    pub address: BDAddr,
    /// The type of address (either random or public)
    pub address_type: Option<AddressType>,
    /// The local name. This is generally a human-readable string that identifies the type of device.
    pub local_name: Option<String>,
    /// The transmission power level for the device
    pub tx_power_level: Option<i16>,
    /// The most recent Received Signal Strength Indicator for the device
    pub rssi: Option<i16>,
    /// Advertisement data specific to the device manufacturer. The keys of this map are
    /// 'manufacturer IDs', while the values are arbitrary data.
    pub manufacturer_data: HashMap<u16, Vec<u8>>,
    /// Advertisement data specific to a service. The keys of this map are
    /// 'Service UUIDs', while the values are arbitrary data.
    pub service_data: HashMap<Uuid, Vec<u8>>,
    /// Advertised services for this device
    pub services: Vec<Uuid>,
}

#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(crate = "serde_cr")
)]
/// The filter used when scanning for BLE devices.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ScanFilter {
    /// If the filter contains at least one service UUID, only devices supporting at least one of
    /// the given services will be available.
    pub services: Vec<Uuid>,
}

/// The type of write operation to use.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WriteType {
    /// A write operation where the device is expected to respond with a confirmation or error. Also
    /// known as a request.
    WithResponse,
    /// A write-without-response, also known as a command.
    WithoutResponse,
}

/// Peripheral is the device that you would like to communicate with (the "server" of BLE). This
/// struct contains both the current state of the device (its properties, characteristics, etc.)
/// as well as functions for communication.
#[async_trait]
pub trait Peripheral: Send + Sync + Clone + Debug {
    /// Returns the unique identifier of the peripheral.
    fn id(&self) -> PeripheralId;

    /// Returns the MAC address of the peripheral.
    fn address(&self) -> BDAddr;

    /// Returns the set of properties associated with the peripheral. These may be updated over time
    /// as additional advertising reports are received.
    async fn properties(&self) -> Result<Option<PeripheralProperties>>;

    /// The set of services we've discovered for this device. This will be empty until
    /// `discover_services` is called.
    fn services(&self) -> BTreeSet<Service>;

    /// The set of characteristics we've discovered for this device. This will be empty until
    /// `discover_services` is called.
    fn characteristics(&self) -> BTreeSet<Characteristic> {
        self.services()
            .iter()
            .flat_map(|service| service.characteristics.clone().into_iter())
            .collect()
    }

    /// Returns true iff we are currently connected to the device.
    async fn is_connected(&self) -> Result<bool>;

    /// Creates a connection to the device. If this method returns Ok there has been successful
    /// connection. Note that peripherals allow only one connection at a time. Operations that
    /// attempt to communicate with a device will fail until it is connected.
    async fn connect(&self) -> Result<()>;

    /// Terminates a connection to the device.
    async fn disconnect(&self) -> Result<()>;

    /// Discovers all services for the device, including their characteristics.
    async fn discover_services(&self) -> Result<()>;

    /// Write some data to the characteristic. Returns an error if the write couldn't be sent or (in
    /// the case of a write-with-response) if the device returns an error.
    async fn write(
        &self,
        characteristic: &Characteristic,
        data: &[u8],
        write_type: WriteType,
    ) -> Result<()>;

    /// Sends a read request to the device. Returns either an error if the request was not accepted
    /// or the response from the device.
    async fn read(&self, characteristic: &Characteristic) -> Result<Vec<u8>>;

    /// Enables either notify or indicate (depending on support) for the specified characteristic.
    async fn subscribe(&self, characteristic: &Characteristic) -> Result<()>;

    /// Disables either notify or indicate (depending on support) for the specified characteristic.
    async fn unsubscribe(&self, characteristic: &Characteristic) -> Result<()>;

    /// Returns a stream of notifications for characteristic value updates. The stream will receive
    /// a notification when a value notification or indication is received from the device.
    /// The stream will remain valid across connections and can be queried before any connection
    /// is made.
    async fn notifications(&self) -> Result<Pin<Box<dyn Stream<Item = ValueNotification> + Send>>>;

    /// Write some data to the descriptor. Returns an error if the write couldn't be sent or (in
    /// the case of a write-with-response) if the device returns an error.
    async fn write_descriptor(&self, descriptor: &Descriptor, data: &[u8]) -> Result<()>;

    /// Sends a read descriptor request to the device. Returns either an error if the request
    /// was not accepted or the response from the device.
    async fn read_descriptor(&self, descriptor: &Descriptor) -> Result<Vec<u8>>;
}

#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(crate = "serde_cr")
)]
#[derive(Debug, Clone)]
pub enum CentralEvent {
    DeviceDiscovered(PeripheralId),
    DeviceUpdated(PeripheralId),
    DeviceConnected(PeripheralId),
    DeviceDisconnected(PeripheralId),
    /// Emitted when a Manufacturer Data advertisement has been received from a device
    ManufacturerDataAdvertisement {
        id: PeripheralId,
        manufacturer_data: HashMap<u16, Vec<u8>>,
    },
    /// Emitted when a Service Data advertisement has been received from a device
    ServiceDataAdvertisement {
        id: PeripheralId,
        service_data: HashMap<Uuid, Vec<u8>>,
    },
    /// Emitted when the advertised services for a device has been updated
    ServicesAdvertisement {
        id: PeripheralId,
        services: Vec<Uuid>,
    },
}

/// Central is the "client" of BLE. It's able to scan for and establish connections to peripherals.
/// A Central can be obtained from [`Manager::adapters()`].
#[async_trait]
pub trait Central: Send + Sync + Clone {
    type Peripheral: Peripheral;

    /// Retrieve a stream of `CentralEvent`s. This stream will receive notifications when events
    /// occur for this Central module. See [`CentralEvent`] for the full set of possible events.
    async fn events(&self) -> Result<Pin<Box<dyn Stream<Item = CentralEvent> + Send>>>;

    /// Starts a scan for BLE devices. This scan will generally continue until explicitly stopped,
    /// although this may depend on your Bluetooth adapter. Discovered devices will be announced
    /// to subscribers of `events` and will be available via `peripherals()`.
    /// The filter can be used to scan only for specific devices. While some implementations might
    /// ignore (parts of) the filter and make additional devices available, other implementations
    /// might require at least one filter for security reasons. Cross-platform code should provide
    /// a filter, but must be able to handle devices, which do not fit into the filter.
    async fn start_scan(&self, filter: ScanFilter) -> Result<()>;

    /// Stops scanning for BLE devices.
    async fn stop_scan(&self) -> Result<()>;

    /// Returns the list of [`Peripheral`]s that have been discovered so far. Note that this list
    /// may contain peripherals that are no longer available.
    async fn peripherals(&self) -> Result<Vec<Self::Peripheral>>;

    /// Returns a particular [`Peripheral`] by its address if it has been discovered.
    async fn peripheral(&self, id: &PeripheralId) -> Result<Self::Peripheral>;

    /// Add a [`Peripheral`] from a MAC address without a scan result. Not supported on all Bluetooth systems.
    async fn add_peripheral(&self, address: &PeripheralId) -> Result<Self::Peripheral>;

    /// Get information about the Bluetooth adapter being used, such as the model or type.
    ///
    /// The details of this are platform-specific andyou should not attempt to parse it, but it may
    /// be useful for debug logs.
    async fn adapter_info(&self) -> Result<String>;
}

/// The Manager is the entry point to the library, providing access to all the Bluetooth adapters on
/// the system. You can obtain an instance from [`platform::Manager::new()`](crate::platform::Manager::new).
///
/// ## Usage
/// ```
/// use btleplug::api::Manager as _;
/// use btleplug::platform::Manager;
/// # use std::error::Error;
///
/// # async fn example() -> Result<(), Box<dyn Error>> {
/// let manager = Manager::new().await?;
/// let adapter_list = manager.adapters().await?;
/// if adapter_list.is_empty() {
///    eprintln!("No Bluetooth adapters");
/// }
/// # Ok(())
/// # }
/// ```
#[async_trait]
pub trait Manager {
    /// The concrete type of the [`Central`] implementation.
    type Adapter: Central;

    /// Get a list of all Bluetooth adapters on the system. Each adapter implements [`Central`].
    async fn adapters(&self) -> Result<Vec<Self::Adapter>>;
}
