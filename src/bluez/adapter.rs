use super::peripheral::{Peripheral, PeripheralId};
use crate::api::{BDAddr, Central, CentralEvent, ScanFilter};
use crate::{Error, Result};
use async_trait::async_trait;
use bluez_async::{
    AdapterId, BluetoothError, BluetoothEvent, BluetoothSession, DeviceEvent, DiscoveryFilter,
    Transport,
};
use futures::stream::{self, Stream, StreamExt};
use std::pin::Pin;

/// Implementation of [api::Central](crate::api::Central).
#[derive(Clone, Debug)]
pub struct Adapter {
    session: BluetoothSession,
    adapter: AdapterId,
}

impl Adapter {
    pub(crate) fn new(session: BluetoothSession, adapter: AdapterId) -> Self {
        Self { session, adapter }
    }
}

#[async_trait]
impl Central for Adapter {
    type Peripheral = Peripheral;

    async fn events(&self) -> Result<Pin<Box<dyn Stream<Item = CentralEvent> + Send>>> {
        // There's a race between getting this event stream and getting the current set of devices.
        // Get the stream first, on the basis that it's better to have a duplicate DeviceDiscovered
        // event than to miss one. It's unlikely to happen in any case.
        let events = self.session.event_stream().await?;

        // Synthesise `DeviceDiscovered' and `DeviceConnected` events for existing peripherals.
        let devices = self.session.get_devices().await?;
        let initial_events = stream::iter(devices.into_iter().flat_map(|device| {
            let id: PeripheralId = device.mac_address.into();
            let mut events = vec![CentralEvent::DeviceDiscovered(id.clone())];
            if device.connected {
                events.push(CentralEvent::DeviceConnected(id));
            }
            events.into_iter()
        }));

        let session = self.session.clone();
        let events = events.filter_map(move |event| central_event(event, session.clone()));

        Ok(Box::pin(initial_events.chain(events)))
    }

    async fn start_scan(&self, filter: ScanFilter) -> Result<()> {
        let filter = DiscoveryFilter {
            service_uuids: filter.services,
            transport: Some(Transport::Auto),
            ..Default::default()
        };
        self.session.start_discovery_with_filter(&filter).await?;
        Ok(())
    }

    async fn stop_scan(&self) -> Result<()> {
        self.session.stop_discovery().await?;
        Ok(())
    }

    async fn peripherals(&self) -> Result<Vec<Peripheral>> {
        let devices = self.session.get_devices().await?;
        Ok(devices
            .into_iter()
            .map(|device| Peripheral::new(self.session.clone(), device))
            .collect())
    }

    async fn peripheral(&self, id: &PeripheralId) -> Result<Peripheral> {
        let devices = self.session.get_devices().await?;
        devices
            .into_iter()
            .find_map(|device| {
                if PeripheralId::from(device.mac_address) == *id {
                    Some(Peripheral::new(self.session.clone(), device))
                } else {
                    None
                }
            })
            .ok_or(Error::DeviceNotFound)
    }

    async fn add_peripheral(&self, _address: BDAddr) -> Result<Peripheral> {
        Err(Error::NotSupported(
            "Can't add a Peripheral from a BDAddr".to_string(),
        ))
    }

    async fn adapter_info(&self) -> Result<String> {
        let adapter_info = self.session.get_adapter_info(&self.adapter).await?;
        Ok(format!("{} ({})", adapter_info.id, adapter_info.modalias))
    }
}

impl From<BluetoothError> for Error {
    fn from(error: BluetoothError) -> Self {
        Error::Other(Box::new(error))
    }
}

async fn central_event(event: BluetoothEvent, session: BluetoothSession) -> Option<CentralEvent> {
    match event {
        BluetoothEvent::Device {
            id,
            event: DeviceEvent::Discovered,
        } => {
            let device = session.get_device_info(&id).await.ok()?;
            Some(CentralEvent::DeviceDiscovered(device.mac_address.into()))
        }
        BluetoothEvent::Device {
            id,
            event: DeviceEvent::Connected { connected },
        } => {
            let device = session.get_device_info(&id).await.ok()?;
            if connected {
                Some(CentralEvent::DeviceConnected(device.mac_address.into()))
            } else {
                Some(CentralEvent::DeviceDisconnected(device.mac_address.into()))
            }
        }
        BluetoothEvent::Device {
            id,
            event: DeviceEvent::Rssi { rssi: _ },
        } => {
            let device = session.get_device_info(&id).await.ok()?;
            Some(CentralEvent::DeviceUpdated(device.mac_address.into()))
        }
        BluetoothEvent::Device {
            id,
            event: DeviceEvent::ManufacturerData { manufacturer_data },
        } => {
            let device = session.get_device_info(&id).await.ok()?;
            Some(CentralEvent::ManufacturerDataAdvertisement {
                id: device.mac_address.into(),
                manufacturer_data,
            })
        }
        BluetoothEvent::Device {
            id,
            event: DeviceEvent::ServiceData { service_data },
        } => {
            let device = session.get_device_info(&id).await.ok()?;
            Some(CentralEvent::ServiceDataAdvertisement {
                id: device.mac_address.into(),
                service_data,
            })
        }
        BluetoothEvent::Device {
            id,
            event: DeviceEvent::Services { services },
        } => {
            let device = session.get_device_info(&id).await.ok()?;
            Some(CentralEvent::ServicesAdvertisement {
                id: device.mac_address.into(),
                services,
            })
        }
        _ => None,
    }
}
