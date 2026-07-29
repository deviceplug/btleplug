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
    collections::{BTreeSet, HashMap, HashSet},
    fmt::{self, Debug, Display, Formatter},
    hash::Hash,
    pin::Pin,
    time::Duration,
};
use uuid::Uuid;

pub use self::bdaddr::{BDAddr, ParseBDAddrError};

use crate::platform::PeripheralId;

/// The default MTU size for a peripheral.
pub const DEFAULT_MTU_SIZE: u16 = 23;

#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(crate = "serde_cr")
)]
#[derive(Debug, Clone, Copy, Eq, PartialEq, Default)]
pub enum AddressType {
    Random,
    #[default]
    Public,
}

impl AddressType {
    #[allow(clippy::should_implement_trait)]
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
    /// UUID of the service that contains the characteristic.
    pub service_uuid: Uuid,
    /// The new value of the characteristic.
    pub value: Vec<u8>,
}

bitflags! {
    /// A set of properties that indicate what operations are supported by a Characteristic.
    #[derive(Default, Debug, PartialEq, Eq, Ord, PartialOrd, Clone, Copy)]
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
    /// The GAP local name. This is generally a human-readable string that identifies the type of device.
    pub local_name: Option<String>,
    /// The advertisement name. May be different than local_name.
    pub advertisement_name: Option<String>,
    /// The GAP appearance reported by this peripheral.
    #[cfg_attr(feature = "serde", serde(default))]
    pub appearance: Option<u16>,
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
    pub class: Option<u32>,
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

/// Selects peripherals for [`Central::retrieve_peripherals`].
///
/// `None` leaves a selector unspecified; an explicitly empty selector matches nothing. Values
/// within a selector are OR'ed, while the identifier and service selectors are combined as a
/// union. Returned peripherals retain backend order and are deduplicated by identifier.
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct RetrievePeripheralsOptions {
    /// Known peripheral identifiers to retrieve.
    pub identifiers: Option<Vec<PeripheralId>>,
    /// Service UUIDs used to retrieve connected peripherals.
    pub services: Option<Vec<Uuid>>,
}

/// Returns whether a candidate identifier is included in an identifier selector.
#[allow(dead_code)] // Used by platform-gated backend implementations.
pub(crate) fn matches_identifier<T: Eq>(candidate: &T, requested: &[T]) -> bool {
    requested.iter().any(|requested| requested == candidate)
}

/// Returns whether a candidate service set contains one of the requested services.
#[allow(dead_code)] // Used by platform-gated backend implementations.
pub(crate) fn matches_service(candidate_services: &[Uuid], requested: &[Uuid]) -> bool {
    requested
        .iter()
        .any(|requested| candidate_services.contains(requested))
}

/// Returns whether a candidate matches either supplied selector.
///
/// A selector is considered supplied even when empty; in that case it matches nothing.
#[allow(dead_code)] // Used by platform-gated backend implementations.
pub(crate) fn matches_retrieval_selectors(
    candidate_id: &PeripheralId,
    candidate_services: &[Uuid],
    options: &RetrievePeripheralsOptions,
) -> bool {
    let id_match = options
        .identifiers
        .as_deref()
        .is_some_and(|requested| matches_identifier(candidate_id, requested));
    let service_match = options
        .services
        .as_deref()
        .is_some_and(|requested| matches_service(candidate_services, requested));

    if options.identifiers.is_none() && options.services.is_none() {
        true
    } else {
        id_match || service_match
    }
}

/// Merges retrieved peripherals while preserving the first occurrence of each identifier.
#[allow(dead_code)] // Used by platform-gated backend implementations.
pub(crate) fn merge_retrieved_peripherals<P, K, F>(
    peripherals: impl IntoIterator<Item = P>,
    id: F,
) -> Vec<P>
where
    K: Eq + Hash,
    F: Fn(&P) -> K,
{
    let mut seen = HashSet::new();
    peripherals
        .into_iter()
        .filter(|peripheral| seen.insert(id(peripheral)))
        .collect()
}

#[cfg(test)]
fn unsupported_retrieve_peripherals<P>() -> Result<Vec<P>> {
    Err(crate::Error::NotSupported(
        "retrieve_peripherals".to_string(),
    ))
}

/// Current BLE connection parameters as reported by the OS.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConnectionParameters {
    /// Connection interval in microseconds (typically 7_500..4_000_000).
    pub interval_us: u32,
    /// Slave latency in number of connection events (0..499).
    pub latency: u16,
    /// Supervision timeout in microseconds (100_000..32_000_000).
    pub supervision_timeout_us: u32,
}

/// Preferred connection parameter presets for requesting updates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionParameterPreset {
    /// Balanced between throughput and power (default).
    Balanced,
    /// Low latency, high throughput. Use temporarily for bulk transfers.
    ThroughputOptimized,
    /// Reduced power consumption, higher latency.
    PowerOptimized,
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

    /// Returns the currently negotiated mtu size
    fn mtu(&self) -> u16;

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

    /// Like [`connect`](Peripheral::connect), but returns [`Error::TimedOut`](crate::Error::TimedOut)
    /// if the connection is not established within the given duration.
    async fn connect_with_timeout(&self, timeout: Duration) -> Result<()> {
        tokio::time::timeout(timeout, self.connect())
            .await
            .map_err(|_| crate::Error::TimedOut(timeout))?
    }

    /// Terminates a connection to the device.
    async fn disconnect(&self) -> Result<()>;

    /// Discovers all services for the device, including their characteristics.
    async fn discover_services(&self) -> Result<()>;

    /// Like [`discover_services`](Peripheral::discover_services), but returns
    /// [`Error::TimedOut`](crate::Error::TimedOut) if discovery does not complete within the
    /// given duration.
    async fn discover_services_with_timeout(&self, timeout: Duration) -> Result<()> {
        tokio::time::timeout(timeout, self.discover_services())
            .await
            .map_err(|_| crate::Error::TimedOut(timeout))?
    }

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

    /// Returns current connection parameters, if available on this platform.
    /// Returns `Ok(None)` if the platform doesn't support reading parameters.
    /// Returns `Err` if not connected.
    async fn connection_parameters(&self) -> Result<Option<ConnectionParameters>> {
        Err(crate::Error::NotSupported(
            "connection_parameters".to_string(),
        ))
    }

    /// Request a connection parameter update using a preset.
    /// This is a request — the remote device may accept or reject.
    /// Returns `Err(NotSupported)` on platforms that don't support this.
    async fn request_connection_parameters(
        &self,
        _preset: ConnectionParameterPreset,
    ) -> Result<()> {
        Err(crate::Error::NotSupported(
            "request_connection_parameters".to_string(),
        ))
    }

    /// Read the current RSSI (signal strength) for this peripheral, in dBm.
    ///
    /// Behavior varies by platform:
    /// - **macOS/iOS/Android**: Actively reads RSSI from the connected device.
    /// - **Linux**: Returns the latest RSSI from BlueZ device properties.
    /// - **Windows**: Returns the most recent RSSI from advertisements
    ///   (requires scanning to be active for fresh values).
    ///
    /// Returns `Err(NotConnected)` if not connected (except Windows, which may
    /// return a cached scan value).
    async fn read_rssi(&self) -> Result<i16> {
        Err(crate::Error::NotSupported("read_rssi".to_string()))
    }
}

#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(crate = "serde_cr")
)]
/// The state of the Central
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CentralState {
    Unknown = 0,
    PoweredOn = 1,
    PoweredOff = 2,
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
    /// Only emitted on the corebluetooth subsystem
    DeviceServicesModified(PeripheralId),
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
    /// Emitted when an RSSI (signal strength) update is received for a device.
    /// This may come from advertisements during scanning, or from an active
    /// `read_rssi()` call on connected platforms.
    RssiUpdate {
        id: PeripheralId,
        rssi: i16,
    },
    StateUpdate(CentralState),
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

    /// Retrieves peripherals from the backend's connected-device or known-device source.
    ///
    /// Selectors are combined as a union: a peripheral is returned when its identifier matches
    /// any requested identifier or its backend-reported services contain any requested service.
    /// Results are in backend order and deduplicated by [`Peripheral::id`]. An explicitly empty
    /// selector matches nothing. Backends without a retrieval source return
    /// [`Error::NotSupported`](crate::Error::NotSupported) with `"retrieve_peripherals"`.
    async fn retrieve_peripherals(
        &self,
        _options: RetrievePeripheralsOptions,
    ) -> Result<Vec<Self::Peripheral>> {
        Err(crate::Error::NotSupported(
            "retrieve_peripherals".to_string(),
        ))
    }

    /// Returns a particular [`Peripheral`] by its address if it has been discovered.
    async fn peripheral(&self, id: &PeripheralId) -> Result<Self::Peripheral>;

    /// Add a [`Peripheral`] from a MAC address without a scan result. Not supported on all Bluetooth systems.
    async fn add_peripheral(&self, address: &PeripheralId) -> Result<Self::Peripheral>;

    /// Clears the list of [`Peripheral`]s that have been discovered so far. Connected peripherals
    /// should be disconnected before calling this method. On platforms that do not cache peripherals
    /// locally (e.g. BlueZ on Linux), this is a no-op.
    async fn clear_peripherals(&self) -> Result<()>;

    /// Get information about the Bluetooth adapter being used, such as the model or type.
    ///
    /// The details of this are platform-specific andyou should not attempt to parse it, but it may
    /// be useful for debug logs.
    async fn adapter_info(&self) -> Result<String>;

    /// Retrieve the Bluetooth address exposed by the local adapter, when available.
    ///
    /// `Ok(Some(address))` means the platform exposed a usable adapter Bluetooth address.
    /// `Ok(None)` means the platform or its public API does not expose one. `Err` means
    /// retrieving the adapter metadata failed operationally. This value is optional and is
    /// not the portable adapter identity: callers should continue using the platform adapter
    /// handle and [`PeripheralId`] for identity and lookup.
    ///
    /// ```no_run
    /// # use btleplug::api::Central as _;
    /// # async fn example(adapter: impl btleplug::api::Central) {
    /// match adapter.adapter_address().await {
    ///     Ok(Some(address)) => println!("adapter address: {address}"),
    ///     Ok(None) => println!("adapter address is unavailable on this platform"),
    ///     Err(error) => eprintln!("could not query adapter address: {error}"),
    /// }
    /// # }
    /// ```
    async fn adapter_address(&self) -> Result<Option<BDAddr>> {
        Ok(None)
    }

    /// Get information about the Bluetooth adapter state.
    async fn adapter_state(&self) -> Result<CentralState>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retrieve_options_are_publicly_constructible() {
        let options = RetrievePeripheralsOptions {
            identifiers: Some(Vec::new()),
            services: Some(vec![Uuid::nil()]),
        };
        assert_eq!(options.services, Some(vec![Uuid::nil()]));
        assert_eq!(options.identifiers, Some(Vec::new()));
    }

    #[test]
    fn retrieve_options_default_is_explicit() {
        assert_eq!(RetrievePeripheralsOptions::default().identifiers, None);
        assert_eq!(RetrievePeripheralsOptions::default().services, None);
    }

    #[test]
    fn retrieve_empty_identifier_selector_matches_nothing() {
        assert!(!matches_identifier(&1_u8, &[]));
    }

    #[test]
    fn retrieve_empty_service_selector_matches_nothing() {
        assert!(!matches_service(&[Uuid::nil()], &[]));
    }

    #[test]
    fn retrieve_selector_matching_uses_any_value() {
        assert!(matches_identifier(&2_u8, &[1, 2, 3]));
        assert!(matches_service(
            &[Uuid::nil()],
            &[Uuid::from_u128(1), Uuid::nil()],
        ));
    }

    #[test]
    fn retrieve_unknown_identifiers_are_omitted() {
        let requested = [1_u8, 2_u8];
        assert!(!matches_identifier(&3, &requested));
    }

    #[test]
    fn retrieve_order_is_preserved() {
        let merged = merge_retrieved_peripherals([3_u8, 1, 2], |value| *value);
        assert_eq!(merged, vec![3, 1, 2]);
    }

    #[test]
    fn retrieve_combined_selectors_use_union() {
        assert!(matches_service(&[Uuid::nil()], &[Uuid::nil()]));
        assert!(!matches_service(&[], &[Uuid::nil()]));
    }

    #[test]
    fn retrieve_results_are_deduplicated() {
        let merged = merge_retrieved_peripherals([1_u8, 2, 1, 3, 2], |value| *value);
        assert_eq!(merged, vec![1, 2, 3]);
    }

    #[test]
    fn retrieve_peripherals_default_is_not_supported() {
        let error = unsupported_retrieve_peripherals::<u8>().unwrap_err();
        assert!(matches!(
            error,
            crate::Error::NotSupported(operation) if operation == "retrieve_peripherals"
        ));
    }
}

/// The Manager is the entry point for the library, providing access to all Bluetooth adapters on
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

#[cfg(all(test, feature = "serde"))]
mod tests {
    use super::PeripheralProperties;

    #[test]
    fn peripheral_properties_round_trip_appearance() {
        let properties = PeripheralProperties {
            appearance: Some(0x0340),
            ..PeripheralProperties::default()
        };

        let value = serde_json::to_value(&properties).unwrap();
        assert_eq!(value["appearance"], 0x0340);

        let decoded: PeripheralProperties = serde_json::from_value(value).unwrap();
        assert_eq!(decoded.appearance, Some(0x0340));
    }

    #[test]
    fn peripheral_properties_missing_appearance_defaults_to_none() {
        let mut value = serde_json::to_value(PeripheralProperties::default()).unwrap();
        value.as_object_mut().unwrap().remove("appearance").unwrap();

        let properties: PeripheralProperties = serde_json::from_value(value).unwrap();
        assert_eq!(properties.appearance, None);
    }
}
