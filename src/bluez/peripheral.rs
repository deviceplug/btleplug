use async_trait::async_trait;
use bluez_async::{
    BluetoothEvent, BluetoothSession, CharacteristicEvent, CharacteristicFlags, CharacteristicInfo,
    DeviceId, DeviceInfo, MacAddress, WriteOptions,
};
use futures::future::ready;
use futures::stream::{Stream, StreamExt};
use std::collections::BTreeSet;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use crate::api::{
    self, AddressType, BDAddr, CharPropFlags, Characteristic, PeripheralProperties,
    ValueNotification, WriteType,
};
use crate::{Error, Result};

/// Implementation of [api::Peripheral](crate::api::Peripheral).
#[derive(Clone, Debug)]
pub struct Peripheral {
    session: BluetoothSession,
    device: DeviceId,
    mac_address: BDAddr,
    characteristics: Arc<Mutex<Vec<CharacteristicInfo>>>,
}

impl Peripheral {
    pub(crate) fn new(session: BluetoothSession, device: DeviceInfo) -> Self {
        Peripheral {
            session,
            device: device.id,
            mac_address: (&device.mac_address).into(),
            characteristics: Arc::new(Mutex::new(vec![])),
        }
    }

    fn characteristic_info(&self, characteristic: &Characteristic) -> Result<CharacteristicInfo> {
        let characteristics = self.characteristics.lock().unwrap();
        characteristics
            .iter()
            .find(|info| info.uuid == characteristic.uuid)
            .cloned()
            .ok_or_else(|| {
                Error::Other(format!(
                    "Characteristic with UUID {} not found.",
                    characteristic.uuid
                ))
            })
    }

    async fn device_info(&self) -> Result<DeviceInfo> {
        Ok(self.session.get_device_info(&self.device).await?)
    }
}

#[async_trait]
impl api::Peripheral for Peripheral {
    fn address(&self) -> BDAddr {
        self.mac_address
    }

    async fn properties(&self) -> Result<PeripheralProperties> {
        let device_info = self.device_info().await?;
        Ok(PeripheralProperties {
            address: (&device_info.mac_address).into(),
            address_type: device_info.address_type.into(),
            local_name: device_info.name,
            tx_power_level: device_info.tx_power.map(|tx_power| tx_power as i8),
            manufacturer_data: device_info.manufacturer_data,
            service_data: device_info.service_data,
            services: device_info.services,
            discovery_count: 0,
            has_scan_response: true,
        })
    }

    fn characteristics(&self) -> BTreeSet<Characteristic> {
        let characteristics = &*self.characteristics.lock().unwrap();
        characteristics.iter().map(Characteristic::from).collect()
    }

    async fn is_connected(&self) -> Result<bool> {
        let device_info = self.device_info().await?;
        Ok(device_info.connected)
    }

    async fn connect(&self) -> Result<()> {
        self.session.connect(&self.device).await?;
        Ok(())
    }

    async fn disconnect(&self) -> Result<()> {
        self.session.disconnect(&self.device).await?;
        Ok(())
    }

    async fn discover_characteristics(&self) -> Result<Vec<Characteristic>> {
        let mut characteristics = vec![];
        let services = self.session.get_services(&self.device).await?;
        for service in services {
            characteristics.extend(self.session.get_characteristics(&service.id).await?);
        }
        let converted = characteristics.iter().map(Characteristic::from).collect();
        *self.characteristics.lock().unwrap() = characteristics;
        Ok(converted)
    }

    async fn write(
        &self,
        characteristic: &Characteristic,
        data: &[u8],
        write_type: WriteType,
    ) -> Result<()> {
        let characteristic_info = self.characteristic_info(characteristic)?;
        let options = WriteOptions {
            write_type: Some(write_type.into()),
            ..Default::default()
        };
        Ok(self
            .session
            .write_characteristic_value_with_options(&characteristic_info.id, data, options)
            .await?)
    }

    async fn read(&self, characteristic: &Characteristic) -> Result<Vec<u8>> {
        let characteristic_info = self.characteristic_info(characteristic)?;
        Ok(self
            .session
            .read_characteristic_value(&characteristic_info.id)
            .await?)
    }

    async fn subscribe(&self, characteristic: &Characteristic) -> Result<()> {
        let characteristic_info = self.characteristic_info(characteristic)?;
        Ok(self.session.start_notify(&characteristic_info.id).await?)
    }

    async fn unsubscribe(&self, characteristic: &Characteristic) -> Result<()> {
        let characteristic_info = self.characteristic_info(characteristic)?;
        Ok(self.session.stop_notify(&characteristic_info.id).await?)
    }

    async fn notifications(&self) -> Result<Pin<Box<dyn Stream<Item = ValueNotification>>>> {
        let device_id = self.device.clone();
        let events = self.session.device_event_stream(&device_id).await?;
        let characteristics = self.characteristics.clone();
        Ok(Box::pin(events.filter_map(move |event| {
            ready(value_notification(
                event,
                &device_id,
                characteristics.clone(),
            ))
        })))
    }
}

fn value_notification(
    event: BluetoothEvent,
    device_id: &DeviceId,
    characteristics: Arc<Mutex<Vec<CharacteristicInfo>>>,
) -> Option<ValueNotification> {
    match event {
        BluetoothEvent::Characteristic {
            id,
            event: CharacteristicEvent::Value { value },
        } if id.service().device() == *device_id => {
            let characteristics = characteristics.lock().unwrap();
            let uuid = characteristics
                .iter()
                .find(|characteristic| characteristic.id == id)?
                .uuid;
            Some(ValueNotification { uuid, value })
        }
        _ => None,
    }
}

impl From<WriteType> for bluez_async::WriteType {
    fn from(write_type: WriteType) -> Self {
        match write_type {
            WriteType::WithoutResponse => bluez_async::WriteType::WithoutResponse,
            WriteType::WithResponse => bluez_async::WriteType::WithResponse,
        }
    }
}

impl From<&MacAddress> for BDAddr {
    fn from(mac_address: &MacAddress) -> Self {
        mac_address.to_string().parse().unwrap()
    }
}

impl From<bluez_async::AddressType> for AddressType {
    fn from(address_type: bluez_async::AddressType) -> Self {
        match address_type {
            bluez_async::AddressType::Public => AddressType::Public,
            bluez_async::AddressType::Random => AddressType::Random,
        }
    }
}

impl From<&CharacteristicInfo> for Characteristic {
    fn from(characteristic: &CharacteristicInfo) -> Self {
        Characteristic {
            uuid: characteristic.uuid,
            properties: characteristic.flags.into(),
        }
    }
}

impl From<CharacteristicFlags> for CharPropFlags {
    fn from(flags: CharacteristicFlags) -> Self {
        let mut result = CharPropFlags::default();
        if flags.contains(CharacteristicFlags::BROADCAST) {
            result.insert(CharPropFlags::BROADCAST);
        }
        if flags.contains(CharacteristicFlags::READ) {
            result.insert(CharPropFlags::READ);
        }
        if flags.contains(CharacteristicFlags::WRITE_WITHOUT_RESPONSE) {
            result.insert(CharPropFlags::WRITE_WITHOUT_RESPONSE);
        }
        if flags.contains(CharacteristicFlags::WRITE) {
            result.insert(CharPropFlags::WRITE);
        }
        if flags.contains(CharacteristicFlags::NOTIFY) {
            result.insert(CharPropFlags::NOTIFY);
        }
        if flags.contains(CharacteristicFlags::INDICATE) {
            result.insert(CharPropFlags::INDICATE);
        }
        if flags.contains(CharacteristicFlags::SIGNED_WRITE) {
            result.insert(CharPropFlags::AUTHENTICATED_SIGNED_WRITES);
        }
        if flags.contains(CharacteristicFlags::EXTENDED_PROPERTIES) {
            result.insert(CharPropFlags::EXTENDED_PROPERTIES);
        }
        result
    }
}
