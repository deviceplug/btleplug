use super::peripheral::{Peripheral, PeripheralId};
use crate::api::{Central, CentralEvent, CentralState, ScanFilter};
use crate::{Error, Result};
use async_trait::async_trait;
use bluez_async::{
    AdapterEvent, AdapterId, BluetoothError, BluetoothEvent, BluetoothSession, DeviceEvent,
    DiscoveryFilter, Transport,
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

fn get_central_state(powered: bool) -> CentralState {
    match powered {
        true => CentralState::PoweredOn,
        false => CentralState::PoweredOff,
    }
}

#[async_trait]
impl Central for Adapter {
    type Peripheral = Peripheral;

    async fn events(&self) -> Result<Pin<Box<dyn Stream<Item = CentralEvent> + Send>>> {
        // There's a race between getting this event stream and getting the current set of devices.
        // Get the stream first, on the basis that it's better to have a duplicate DeviceDiscovered
        // event than to miss one. It's unlikely to happen in any case.
        let events = self.session.adapter_event_stream(&self.adapter).await?;

        // Synthesise `DeviceDiscovered' and `DeviceConnected` events for existing peripherals.
        let devices = self.session.get_devices().await?;
        let adapter_id = self.adapter.clone();
        let initial_events = stream::iter(
            devices
                .into_iter()
                .filter(move |device| device.id.adapter() == adapter_id)
                .flat_map(|device| {
                    let mut events = vec![CentralEvent::DeviceDiscovered(device.id.clone().into())];
                    if device.connected {
                        events.push(CentralEvent::DeviceConnected(device.id.into()));
                    }
                    events.into_iter()
                }),
        );

        let session = self.session.clone();
        let adapter_id = self.adapter.clone();
        let events = events
            .filter_map(move |event| central_event(event, session.clone(), adapter_id.clone()));

        Ok(Box::pin(initial_events.chain(events)))
    }

    async fn start_scan(&self, filter: ScanFilter) -> Result<()> {
        let filter = DiscoveryFilter {
            service_uuids: filter.services,
            duplicate_data: Some(true),
            transport: Some(Transport::Auto),
            ..Default::default()
        };
        self.session
            .start_discovery_on_adapter_with_filter(&self.adapter, &filter)
            .await?;
        Ok(())
    }

    async fn stop_scan(&self) -> Result<()> {
        self.session
            .stop_discovery_on_adapter(&self.adapter)
            .await?;
        Ok(())
    }

    async fn peripherals(&self) -> Result<Vec<Peripheral>> {
        let devices = self.session.get_devices_on_adapter(&self.adapter).await?;
        Ok(devices
            .into_iter()
            .map(|device| Peripheral::new(self.session.clone(), device))
            .collect())
    }

    async fn peripheral(&self, id: &PeripheralId) -> Result<Peripheral> {
        let device = self.session.get_device_info(&id.0).await.map_err(|e| {
            if let BluetoothError::DbusError(_) = e {
                Error::DeviceNotFound
            } else {
                e.into()
            }
        })?;
        Ok(Peripheral::new(self.session.clone(), device))
    }

    async fn add_peripheral(&self, _address: &PeripheralId) -> Result<Peripheral> {
        Err(Error::NotSupported(
            "Can't add a Peripheral from a PeripheralId".to_string(),
        ))
    }

    async fn adapter_info(&self) -> Result<String> {
        let adapter_info = self.session.get_adapter_info(&self.adapter).await?;
        Ok(format!("{} ({})", adapter_info.id, adapter_info.modalias))
    }

    async fn adapter_state(&self) -> Result<CentralState> {
        let mut powered = false;
        if let Ok(info) = self.session.get_adapter_info(&self.adapter).await {
            powered = info.powered;
        }
        Ok(get_central_state(powered))
    }
}

impl From<BluetoothError> for Error {
    fn from(error: BluetoothError) -> Self {
        Error::Other(Box::new(error))
    }
}

async fn central_event(
    event: BluetoothEvent,
    session: BluetoothSession,
    adapter_id: AdapterId,
) -> Option<CentralEvent> {
    match event {
        BluetoothEvent::Device {
            id,
            event: device_event,
        } if id.adapter() == adapter_id => match device_event {
            DeviceEvent::Discovered => {
                let device = session.get_device_info(&id).await.ok()?;
                Some(CentralEvent::DeviceDiscovered(device.id.into()))
            }
            DeviceEvent::Connected { connected } => {
                let device = session.get_device_info(&id).await.ok()?;
                if connected {
                    Some(CentralEvent::DeviceConnected(device.id.into()))
                } else {
                    Some(CentralEvent::DeviceDisconnected(device.id.into()))
                }
            }
            DeviceEvent::Rssi { rssi: _ } => {
                let device = session.get_device_info(&id).await.ok()?;
                Some(CentralEvent::DeviceUpdated(device.id.into()))
            }
            DeviceEvent::ManufacturerData { manufacturer_data } => {
                let device = session.get_device_info(&id).await.ok()?;
                Some(CentralEvent::ManufacturerDataAdvertisement {
                    id: device.id.into(),
                    manufacturer_data,
                })
            }
            DeviceEvent::ServiceData { service_data } => {
                let device = session.get_device_info(&id).await.ok()?;
                Some(CentralEvent::ServiceDataAdvertisement {
                    id: device.id.into(),
                    service_data,
                })
            }
            DeviceEvent::Services { services } => {
                let device = session.get_device_info(&id).await.ok()?;
                Some(CentralEvent::ServicesAdvertisement {
                    id: device.id.into(),
                    services,
                })
            }
            _ => None,
        },
        BluetoothEvent::Adapter {
            id,
            event: adapter_event,
        } if id == adapter_id => match adapter_event {
            AdapterEvent::Powered { powered } => {
                let state = get_central_state(powered);
                Some(CentralEvent::StateUpdate(state))
            }
            _ => None,
        },
        _ => None,
    }
}
