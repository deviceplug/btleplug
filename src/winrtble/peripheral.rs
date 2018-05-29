use api::AddressType;
use api::BDAddr;
use std::fmt::Debug;
use std::fmt::Display;
use std::fmt::Formatter;
use std::fmt;
use api::PeripheralProperties;
use api::CommandCallback;
use api::NotificationHandler;
use api::RequestCallback;
use api::UUID;
use std::collections::BTreeSet;
use std::collections::HashMap;
use ::Result;
use api::{Peripheral as ApiPeripheral};
use std::sync::{Arc, Mutex};
use winrt::windows::devices::bluetooth::advertisement::*;
use winrt::windows::storage::streams::DataReader;
use winrtble::utils;
use std::sync::atomic::{AtomicBool, Ordering};
use ::Error;
use api::Characteristic;
use api::ValueNotification;
use winrtble::ble::device::BLEDevice;
use winrtble::ble::characteristic::BLECharacteristic;

#[derive(Clone)]
pub struct Peripheral {
    device: Arc<Mutex<Option<BLEDevice>>>,
    address: BDAddr,
    properties: Arc<Mutex<PeripheralProperties>>,
    characteristics: Arc<Mutex<BTreeSet<Characteristic>>>,
    connected: Arc<AtomicBool>,
    ble_characteristics: Arc<Mutex<HashMap<UUID, BLECharacteristic>>>,
    notification_handlers: Arc<Mutex<Vec<NotificationHandler>>>,
}

impl Peripheral {
    pub fn new(address: BDAddr) -> Self {
        let device = Arc::new(Mutex::new(None));
        let mut properties = PeripheralProperties::default();
        properties.address = address;
        let properties = Arc::new(Mutex::new(properties));
        let characteristics = Arc::new(Mutex::new(BTreeSet::new()));
        let connected = Arc::new(AtomicBool::new(false));
        let ble_characteristics = Arc::new(Mutex::new(HashMap::new()));
        let notification_handlers = Arc::new(Mutex::new(Vec::new()));
        Peripheral{ device, address, properties, characteristics, connected, ble_characteristics, notification_handlers }
    }

    pub fn update_properties(&self, args: &BluetoothLEAdvertisementReceivedEventArgs) {
        let mut properties = self.properties.lock().unwrap();
        let advertisement = args.get_advertisement().unwrap().unwrap();
        properties.local_name = advertisement.get_local_name().ok().and_then(|n| 
            if !n.is_empty() {
                Some(n.to_string()) 
            } else {
                None 
            }
        );

        properties.discovery_count += 1;
        // windows does not provide the address type in the advertisement event args but only in the device object
        // https://social.msdn.microsoft.com/Forums/en-US/c71d51a2-56a1-425a-9063-de44fda48766/bluetooth-address-public-or-random?forum=wdk
        properties.address_type = AddressType::default();
        properties.has_scan_response = args.get_advertisement_type().unwrap() == BluetoothLEAdvertisementType::ScanResponse;
        properties.tx_power_level = args.get_raw_signal_strength_in_dbm().ok().map(|rssi| rssi as i8);
        properties.manufacturer_data = if let Ok(Some(manufacturer_data)) = advertisement.get_manufacturer_data() {
            let mut data = Vec::new();
            for i in &manufacturer_data {
                let d = i.unwrap();
                let company_id = d.get_company_id().unwrap();
                let buffer = d.get_data().unwrap().unwrap();
                let reader = DataReader::from_buffer(&buffer).unwrap().unwrap();
                let len = reader.get_unconsumed_buffer_length().unwrap() as usize;
                let mut input = vec![0u8; len + 2];
                reader.read_bytes(&mut input[2..(len+2)]).unwrap();
                input[0] = company_id as u8;
                input[1] = (company_id >> 8) as u8;
                data.append(&mut input);
            }
            Some(data)
        } else {
            None
        };
    }
}

impl Display for Peripheral {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let connected = if self.is_connected() { " connected" } else { "" };
        let properties = self.properties.lock().unwrap();
        write!(f, "{} {}{}", self.address, properties.local_name.clone()
            .unwrap_or_else(|| "(unknown)".to_string()), connected)
    }
}

impl Debug for Peripheral {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let connected = if self.is_connected() { " connected" } else { "" };
        let properties = self.properties.lock().unwrap();
        let characteristics = self.characteristics.lock().unwrap();
        write!(f, "{} properties: {:?}, characteristics: {:?} {}", self.address, *properties,
               *characteristics, connected)
    }
}

impl ApiPeripheral for Peripheral {
    /// Returns the address of the peripheral.
    fn address(&self) -> BDAddr {
        self.address.clone()
    }

    /// Returns the set of properties associated with the peripheral. These may be updated over time
    /// as additional advertising reports are received.
    fn properties(&self) -> PeripheralProperties {
        let l = self.properties.lock().unwrap();
        l.clone()
    }

    /// The set of characteristics we've discovered for this device. This will be empty until
    /// `discover_characteristics` or `discover_characteristics_in_range` is called.
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
        let device = BLEDevice::new(self.address, Box::new(move |is_connected| {
            connected.store(is_connected, Ordering::Relaxed);
        }))?;

        device.connect()?;
        let mut d = self.device.lock().unwrap();
        *d = Some(device);
        Ok(())
    }

    /// Terminates a connection to the device. This is a synchronous operation.
    fn disconnect(&self) -> Result<()> {
        let winrt_error = |e| Error::Other(format!("{:?}", e));
        let mut device = self.device.lock().map_err(winrt_error)?;
        *device = None;
        Ok(())
    }

    /// Discovers all characteristics for the device. This is a synchronous operation.
    fn discover_characteristics(&self) -> Result<Vec<Characteristic>> {
        let device = self.device.lock().unwrap();
        if let Some(ref device) = *device {
            let mut characteristics_result = vec![];
            let mut ble_characteristics = self.ble_characteristics.lock().unwrap();
            let characteristics = device.discover_characteristics()?;
            for characteristic in characteristics {
                let uuid = utils::to_uuid(&characteristic.get_uuid().unwrap());
                let properties = utils::to_char_props(&characteristic.get_characteristic_properties().unwrap());
                let chara = Characteristic { uuid, start_handle: 0, end_handle: 0, value_handle: 0, properties };
                characteristics_result.push(chara);
                ble_characteristics.entry(uuid).or_insert_with(|| {
                    BLECharacteristic::new(characteristic)
                });
            }
            return Ok(characteristics_result);
        }
        Err(Error::NotConnected)
    }

    /// Discovers characteristics within the specified range of handles. This is a synchronous
    /// operation.
    fn discover_characteristics_in_range(&self, _start: u16, _end: u16) -> Result<Vec<Characteristic>> {
        Ok(Vec::new())
    }

    /// Sends a command (`write-without-response`) to the characteristic. Takes an optional callback
    /// that will be notified in case of error or when the command has been successfully acked by the
    /// device.
    fn command_async(&self, _characteristic: &Characteristic, _data: &[u8], _handler: Option<CommandCallback>) {

    }

    /// Sends a command (write without response) to the characteristic. Synchronously returns a
    /// `Result` with an error set if the command was not accepted by the device.
    fn command(&self, _characteristic: &Characteristic, _data: &[u8]) -> Result<()> {
        Ok(())
    }

    /// Sends a request (write) to the device. Takes an optional callback with either an error if
    /// the request was not accepted or the response from the device.
    fn request_async(&self, _characteristic: &Characteristic,
                     _data: &[u8], _handler: Option<RequestCallback>) {

                     }

    /// Sends a request (write) to the device. Synchronously returns either an error if the request
    /// was not accepted or the response from the device.
    fn request(&self, _characteristic: &Characteristic, _data: &[u8]) -> Result<Vec<u8>> {
        Ok(Vec::new())
    }

    /// Sends a read-by-type request to device for the range of handles covered by the
    /// characteristic and for the specified declaration UUID. See
    /// [here](https://www.bluetooth.com/specifications/gatt/declarations) for valid UUIDs.
    /// Takes an optional callback that will be called with an error or the device response.
    fn read_by_type_async(&self, _characteristic: &Characteristic,
                          _uuid: UUID, _handler: Option<RequestCallback>) {
    }

    /// Sends a read-by-type request to device for the range of handles covered by the
    /// characteristic and for the specified declaration UUID. See
    /// [here](https://www.bluetooth.com/specifications/gatt/declarations) for valid UUIDs.
    /// Synchronously returns either an error or the device response.
    fn read_by_type(&self, characteristic: &Characteristic,
                    _uuid: UUID) -> Result<Vec<u8>> {
        let ble_characteristics = self.ble_characteristics.lock().unwrap();
        if let Some(ble_characteristic) = ble_characteristics.get(&characteristic.uuid) {
            return ble_characteristic.read_value();
        } else 
            Err(Error::NotSupported("read_by_type".into()))
        }
    }

    /// Enables either notify or indicate (depending on support) for the specified characteristic.
    /// This is a synchronous call.
    fn subscribe(&self, characteristic: &Characteristic) -> Result<()> {
        let mut ble_characteristics = self.ble_characteristics.lock().unwrap();
        if let Some(ble_characteristic) = ble_characteristics.get_mut(&characteristic.uuid) {
            let notification_handlers = self.notification_handlers.clone();
            ble_characteristic.subscribe(Box::new(move |value| {
                let notification = ValueNotification{ handle: 0, value };
                let handlers = notification_handlers.lock().unwrap();
                handlers.iter().for_each(|h| h(notification.clone()));
            }))
        } else {
            Err(Error::NotSupported("subscribe".into()))
        }
    }

    /// Disables either notify or indicate (depending on support) for the specified characteristic.
    /// This is a synchronous call.
    fn unsubscribe(&self, characteristic: &Characteristic) -> Result<()> {
        let mut ble_characteristics = self.ble_characteristics.lock().unwrap();
        if let Some(ble_characteristic) = ble_characteristics.get_mut(&characteristic.uuid) {
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
}
