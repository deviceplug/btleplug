use async_trait::async_trait;
use bluez_async::{
    BluetoothEvent, BluetoothSession, CharacteristicEvent, CharacteristicFlags, CharacteristicId,
    CharacteristicInfo, DeviceId, DeviceInfo, MacAddress, ServiceInfo, WriteOptions,
};
use futures::future::ready;
use futures::stream::{Stream, StreamExt};
use std::collections::{BTreeSet, HashMap};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

use crate::api::{
    self, AddressType, BDAddr, CharPropFlags, Characteristic, PeripheralProperties, Service,
    ValueNotification, WriteType,
};
use crate::{Error, Result};

#[derive(Clone, Debug)]
struct ServiceInternal {
    info: ServiceInfo,
    characteristics: HashMap<Uuid, CharacteristicInfo>,
}

/// Implementation of [api::Peripheral](crate::api::Peripheral).
#[derive(Clone, Debug)]
pub struct Peripheral {
    session: BluetoothSession,
    device: DeviceId,
    mac_address: BDAddr,
    services: Arc<Mutex<HashMap<Uuid, ServiceInternal>>>,
}

impl Peripheral {
    pub(crate) fn new(session: BluetoothSession, device: DeviceInfo) -> Self {
        Peripheral {
            session,
            device: device.id,
            mac_address: (&device.mac_address).into(),
            services: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn characteristic_info(&self, characteristic: &Characteristic) -> Result<CharacteristicInfo> {
        let services = self.services.lock().unwrap();
        services
            .get(&characteristic.service_uuid)
            .ok_or_else(|| {
                Error::Other(
                    format!(
                        "Service with UUID {} not found.",
                        characteristic.service_uuid
                    )
                    .into(),
                )
            })?
            .characteristics
            .get(&characteristic.uuid)
            .cloned()
            .ok_or_else(|| {
                Error::Other(
                    format!(
                        "Characteristic with UUID {} not found.",
                        characteristic.uuid
                    )
                    .into(),
                )
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

    async fn properties(&self) -> Result<Option<PeripheralProperties>> {
        let device_info = self.device_info().await?;
        Ok(Some(PeripheralProperties {
            address: (&device_info.mac_address).into(),
            address_type: Some(device_info.address_type.into()),
            local_name: device_info.name,
            tx_power_level: device_info.tx_power,
            rssi: device_info.rssi,
            manufacturer_data: device_info.manufacturer_data,
            service_data: device_info.service_data,
            services: device_info.services,
        }))
    }

    fn services(&self) -> BTreeSet<Service> {
        self.services
            .lock()
            .unwrap()
            .values()
            .map(|service| service.into())
            .collect()
    }

    fn characteristics(&self) -> BTreeSet<Characteristic> {
        let services = &*self.services.lock().unwrap();
        services
            .values()
            .flat_map(|service| {
                service
                    .characteristics
                    .values()
                    .map(|characteristic| make_characteristic(characteristic, service.info.uuid))
                    .collect::<Vec<_>>()
            })
            .collect()
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
        let mut services_internal = HashMap::new();
        let mut converted_characteristics = vec![];
        let services = self.session.get_services(&self.device).await?;
        for service in services {
            let characteristics = self.session.get_characteristics(&service.id).await?;
            converted_characteristics.extend(
                characteristics
                    .iter()
                    .map(|characteristic| make_characteristic(characteristic, service.uuid)),
            );
            services_internal.insert(
                service.uuid,
                ServiceInternal {
                    info: service,
                    characteristics: characteristics
                        .into_iter()
                        .map(|characteristic| (characteristic.uuid, characteristic))
                        .collect(),
                },
            );
        }
        *self.services.lock().unwrap() = services_internal;
        Ok(converted_characteristics)
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

    async fn notifications(&self) -> Result<Pin<Box<dyn Stream<Item = ValueNotification> + Send>>> {
        let device_id = self.device.clone();
        let events = self.session.device_event_stream(&device_id).await?;
        let services = self.services.clone();
        Ok(Box::pin(events.filter_map(move |event| {
            ready(value_notification(event, &device_id, services.clone()))
        })))
    }
}

fn value_notification(
    event: BluetoothEvent,
    device_id: &DeviceId,
    services: Arc<Mutex<HashMap<Uuid, ServiceInternal>>>,
) -> Option<ValueNotification> {
    match event {
        BluetoothEvent::Characteristic {
            id,
            event: CharacteristicEvent::Value { value },
        } if id.service().device() == *device_id => {
            let services = services.lock().unwrap();
            let uuid = find_characteristic_by_id(&services, id)?.uuid;
            Some(ValueNotification { uuid, value })
        }
        _ => None,
    }
}

fn find_characteristic_by_id(
    services: &HashMap<Uuid, ServiceInternal>,
    characteristic_id: CharacteristicId,
) -> Option<&CharacteristicInfo> {
    for service in services.values() {
        for characteristic in service.characteristics.values() {
            if characteristic.id == characteristic_id {
                return Some(characteristic);
            }
        }
    }
    None
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

fn make_characteristic(info: &CharacteristicInfo, service_uuid: Uuid) -> Characteristic {
    Characteristic {
        uuid: info.uuid,
        properties: info.flags.into(),
        service_uuid,
    }
}

impl From<&ServiceInternal> for Service {
    fn from(service: &ServiceInternal) -> Self {
        Service {
            uuid: service.info.uuid,
            primary: service.info.primary,
            characteristics: service
                .characteristics
                .iter()
                .map(|(_, characteristic)| make_characteristic(characteristic, service.info.uuid))
                .collect(),
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
