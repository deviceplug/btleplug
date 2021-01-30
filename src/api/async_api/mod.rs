use async_trait::async_trait;
use futures::stream::Stream;
use std::collections::BTreeSet;
use std::fmt::Debug;
use std::pin::Pin;

use super::{
    BDAddr, CentralEvent, Characteristic, PeripheralProperties, ValueNotification, WriteType,
};
use crate::Result;

/// Central is the "client" of BLE. It's able to scan for and establish connections to peripherals.
#[async_trait]
pub trait Central: Send + Sync + Clone {
    type Peripheral: Peripheral;

    /// Retreive a stream of `CentralEvent`s. This stream will receive notifications when events
    /// occur for this Central module. See [`CentralEvent`](enum.CentralEvent.html) for the full set
    /// of possible events.
    async fn events(&self) -> Result<Pin<Box<dyn Stream<Item = CentralEvent>>>>;

    /// Starts a scan for BLE devices. This scan will generally continue until explicitly stopped,
    /// although this may depend on your Bluetooth adapter. Discovered devices will be announced
    /// to subscribers of `events` and will be available via `peripherals()`.
    async fn start_scan(&self) -> Result<()>;

    /// Control whether to use active or passive scan mode to find BLE devices. Active mode scan
    /// notifies advertisers about the scan, whereas passive scan only receives data from the
    /// advertiser. Defaults to use active mode.
    async fn active(&self, enabled: bool);

    /// Control whether to filter multiple advertisements by the same peer device. Duplicates can be
    /// useful for some applications, e.g. when using a scan to collect information from beacons
    /// that update data frequently. Defaults to filter duplicate advertisements.
    async fn filter_duplicates(&self, enabled: bool);

    /// Stops scanning for BLE devices.
    async fn stop_scan(&self) -> Result<()>;

    /// Returns the list of [`Peripherals`](trait.Peripheral.html) that have been discovered so far.
    /// Note that this list may contain peripherals that are no longer available.
    async fn peripherals(&self) -> Result<Vec<Self::Peripheral>>;

    /// Returns a particular [`Peripheral`](trait.Peripheral.html) by its address if it has been
    /// discovered.
    async fn peripheral(&self, address: BDAddr) -> Result<Self::Peripheral>;
}

/// Peripheral is the device that you would like to communicate with (the "server" of BLE). This
/// struct contains both the current state of the device (its properties, characteristics, etc.)
/// as well as functions for communication.
#[async_trait]
pub trait Peripheral: Send + Sync + Clone + Debug {
    /// Returns the MAC address of the peripheral.
    fn address(&self) -> BDAddr;

    /// Returns the set of properties associated with the peripheral. These may be updated over time
    /// as additional advertising reports are received.
    async fn properties(&self) -> Result<PeripheralProperties>;

    /// The set of characteristics we've discovered for this device. This will be empty until
    /// `discover_characteristics` is called.
    fn characteristics(&self) -> BTreeSet<Characteristic>;

    /// Returns true iff we are currently connected to the device.
    async fn is_connected(&self) -> Result<bool>;

    /// Creates a connection to the device. If this method returns Ok there has been successful
    /// connection. Note that peripherals allow only one connection at a time. Operations that
    /// attempt to communicate with a device will fail until it is connected.
    async fn connect(&self) -> Result<()>;

    /// Terminates a connection to the device.
    async fn disconnect(&self) -> Result<()>;

    /// Discovers all characteristics for the device.
    async fn discover_characteristics(&self) -> Result<Vec<Characteristic>>;

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
    /// a notification when a value notification or indication is received from the device. This
    /// method should only be used after a connection has been established.
    async fn notifications(&self) -> Result<Pin<Box<dyn Stream<Item = ValueNotification>>>>;
}
