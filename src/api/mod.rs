use std::fmt;
use std::fmt::{Display, Formatter, Debug};

use ::Result;
use std::collections::BTreeSet;
use api::UUID::B16;
use api::UUID::B128;

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum AddressType {
    Random,
    Public,
}

impl Default for AddressType {
    fn default() -> Self { AddressType::Public }
}

impl AddressType {
    pub fn from_u8(v: u8) -> Option<AddressType> {
        match v {
            0 => Some(AddressType::Public),
            1 => Some(AddressType::Random),
            _ => None,
        }
    }

    pub fn num(&self) -> u8 {
        match *self {
            AddressType::Public => 0,
            AddressType::Random => 1
        }
    }
}

/// Stores the 6 byte address used to identify Bluetooth devices.
#[derive(Copy, Clone, Hash, Eq, PartialEq, Default)]
#[repr(C)]
pub struct BDAddr {
    pub address: [ u8 ; 6usize ]
}

impl Display for BDAddr {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let a = self.address;
        write!(f, "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
               a[5], a[4], a[3], a[2], a[1], a[0])
    }
}

impl Debug for BDAddr {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        (self as &Display).fmt(f)
    }
}

/// A notification sent from a peripheral due to a change in a value.
#[derive(Clone, Debug)]
pub struct ValueNotification {
    /// The handle that has changed.
    pub handle: u16,
    /// The new value of the handle.
    pub value: Vec<u8>,
}

pub type Callback<T> = Box<Fn(Result<T>) + Send>;
pub type CommandCallback = Callback<()>;
pub type RequestCallback = Callback<Vec<u8>>;

pub type NotificationHandler = Box<Fn(ValueNotification) + Send>;

/// A Bluetooth UUID. These can either be 2 bytes or 16 bytes long. UUIDs uniquely identify various
/// objects in the Bluetooth universe.
#[derive(Ord, PartialOrd, Eq, PartialEq, Copy, Clone, Hash)]
pub enum UUID {
    B16(u16),
    B128([u8; 16]),
}

impl UUID {
    pub fn size(&self) -> usize {
        match *self {
            B16(_) => 2,
            B128(_) => 16,
        }
    }
}

impl Display for UUID {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match *self {
            B16(u) => write!(f, "{:02X}:{:02X}", u >> 8, u & 0xFF),
            B128(a) => {
                for i in (1..a.len()).rev() {
                    write!(f, "{:02X}:", a[i])?;
                }
                write!(f, "{:02X}", a[0])
            }
        }
    }
}

impl Debug for UUID {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        (self as &Display).fmt(f)
    }
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
    /// The start of the handle range that contains this characteristic.
    pub start_handle: u16,
    /// The end of the handle range that contains this characteristic.
    pub end_handle: u16,
    /// The value handle of the characteristic.
    pub value_handle: u16,
    /// The UUID for this characteristic. This uniquely identifies its behavior.
    pub uuid: UUID,
    /// The set of properties for this characteristic, which indicate what functionality it
    /// supports. If you attempt an operation that is not supported by the characteristics (for
    /// example setting notify on one without the NOTIFY flag), that operation will fail.
    pub properties: CharPropFlags,
}

impl Display for Characteristic {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "handle: 0x{:04X}, char properties: 0x{:02X}, \
                   char value handle: 0x{:04X}, end handle: 0x{:04X}, uuid: {:?}",
               self.start_handle, self.properties,
               self.value_handle, self.end_handle, self.uuid)
    }
}

/// The properties of this peripheral, as determined by the advertising reports we've received for
/// it.
#[derive(Debug, Default, Clone)]
pub struct PeripheralProperties {
    /// The address of this peripheral
    pub address: BDAddr,
    /// The type of address (either random or public)
    pub address_type: AddressType,
    /// The local name. This is generally a human-readable string that identifies the type of device.
    pub local_name: Option<String>,
    /// The transmission power level for the device
    pub tx_power_level: Option<i8>,
    /// Unstructured data set by the device manufacturer
    pub manufacturer_data: Option<Vec<u8>>,
    /// Number of times we've seen advertising reports for this device
    pub discovery_count: u32,
    /// True if we've discovered the device before
    pub has_scan_response: bool,
}

/// Peripheral is the device that you would like to communicate with (the "server" of BLE). This
/// struct contains both the current state of the device (its properties, characteristics, etc.)
/// as well as functions for communication.
pub trait Peripheral: Send + Sync + Clone + Debug {
    /// Returns the address of the peripheral.
    fn address(&self) -> BDAddr;

    /// Returns the set of properties associated with the peripheral. These may be updated over time
    /// as additional advertising reports are received.
    fn properties(&self) -> PeripheralProperties;

    /// The set of characteristics we've discovered for this device. This will be empty until
    /// `discover_characteristics` or `discover_characteristics_in_range` is called.
    fn characteristics(&self) -> BTreeSet<Characteristic>;

    /// Returns true iff we are currently connected to the device.
    fn is_connected(&self) -> bool;

    /// Creates a connection to the device. This is a synchronous operation; if this method returns
    /// Ok there has been successful connection. Note that peripherals allow only one connection at
    /// a time. Operations that attempt to communicate with a device will fail until it is connected.
    fn connect(&self) -> Result<()>;

    /// Terminates a connection to the device. This is a synchronous operation.
    fn disconnect(&self) -> Result<()>;

    /// Discovers all characteristics for the device. This is a synchronous operation.
    fn discover_characteristics(&self) -> Result<Vec<Characteristic>>;

    /// Discovers characteristics within the specified range of handles. This is a synchronous
    /// operation.
    fn discover_characteristics_in_range(&self, start: u16, end: u16) -> Result<Vec<Characteristic>>;

    /// Sends a command (`write-without-response`) to the characteristic. Takes an optional callback
    /// that will be notified in case of error or when the command has been successfully acked by the
    /// device.
    fn command_async(&self, characteristic: &Characteristic, data: &[u8], handler: Option<CommandCallback>);

    /// Sends a command (write without response) to the characteristic. Synchronously returns a
    /// `Result` with an error set if the command was not accepted by the device.
    fn command(&self, characteristic: &Characteristic, data: &[u8]) -> Result<()>;

    /// Sends a request (write) to the device. Takes an optional callback with either an error if
    /// the request was not accepted or the response from the device.
    fn request_async(&self, characteristic: &Characteristic,
                     data: &[u8], handler: Option<RequestCallback>);

    /// Sends a request (write) to the device. Synchronously returns either an error if the request
    /// was not accepted or the response from the device.
    fn request(&self, characteristic: &Characteristic,
               data: &[u8]) -> Result<Vec<u8>>;

    /// Sends a read-by-type request to device for the range of handles covered by the
    /// characteristic and for the specified declaration UUID. See
    /// [here](https://www.bluetooth.com/specifications/gatt/declarations) for valid UUIDs.
    /// Takes an optional callback that will be called with an error or the device response.
    fn read_by_type_async(&self, characteristic: &Characteristic,
                          uuid: UUID, handler: Option<RequestCallback>);

    /// Sends a read-by-type request to device for the range of handles covered by the
    /// characteristic and for the specified declaration UUID. See
    /// [here](https://www.bluetooth.com/specifications/gatt/declarations) for valid UUIDs.
    /// Synchronously returns either an error or the device response.
    fn read_by_type(&self, characteristic: &Characteristic,
                    uuid: UUID) -> Result<Vec<u8>>;

    /// Enables either notify or indicate (depending on support) for the specified characteristic.
    /// This is a synchronous call.
    fn subscribe(&self, characteristic: &Characteristic) -> Result<()>;

    /// Disables either notify or indicate (depending on support) for the specified characteristic.
    /// This is a synchronous call.
    fn unsubscribe(&self, characteristic: &Characteristic) -> Result<()>;

    /// Registers a handler that will be called when value notification messages are received from
    /// the device. This method should only be used after a connection has been established. Note
    /// that the handler will be called in a common thread, so it should not block.
    fn on_notification(&self, handler: NotificationHandler);
}

#[derive(Debug, Copy, Clone)]
pub enum CentralEvent {
    DeviceDiscovered(BDAddr),
    DeviceLost(BDAddr),
    DeviceUpdated(BDAddr),
    DeviceConnected(BDAddr),
    DeviceDisconnected(BDAddr),
}

pub type EventHandler = Box<Fn(CentralEvent) + Send>;

/// Central is the "client" of BLE. It's able to scan for and establish connections to peripherals.
pub trait Central<P : Peripheral>: Send + Sync + Clone {
    /// Registers a function that will receive notifications when events occur for this Central
    /// module. See [`Event`](enum.CentralEvent.html) for the full set of events. Note that the
    /// handler will be called in a common thread, so it should not block.
    fn on_event(&self, handler: EventHandler);

    /// Starts a scan for BLE devices. This scan will generally continue until explicitly stopped,
    /// although this may depend on your bluetooth adapter. Discovered devices will be announced
    /// to subscribers of `on_event` and will be available via `peripherals()`.
    fn start_scan(&self) -> Result<()>;

    /// Stops scanning for BLE devices.
    fn stop_scan(&self) -> Result<()>;

    /// Returns the list of [`Peripherals`](trait.Peripheral.html) that have been discovered so far.
    /// Note that this list may contain peripherals that are no longer available.
    fn peripherals(&self) -> Vec<P>;

    /// Returns a particular [`Peripheral`](trait.Peripheral.html) by its address if it has been
    /// discovered.
    fn peripheral(&self, address: BDAddr) -> Option<P>;
}
