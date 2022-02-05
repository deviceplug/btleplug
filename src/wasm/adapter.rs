use super::peripheral::{Peripheral, PeripheralId};
use super::utils::wrap_promise;
use crate::api::{BDAddr, Central, CentralEvent, Peripheral as _, ScanFilter};
use crate::common::adapter_manager::AdapterManager;
use crate::{Error, Result};
use async_trait::async_trait;
use futures::channel::oneshot;
use futures::stream::Stream;
use js_sys::Array;
use std::pin::Pin;
use std::sync::Arc;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;
use web_sys::{BluetoothDevice, BluetoothLeScanFilterInit, RequestDeviceOptions};

macro_rules! spawn_local {
    ($a:expr) => {{
        let (sender, receiver) = oneshot::channel();
        spawn_local(async move {
            let _ = sender.send($a);
        });
        receiver.await.unwrap()
    }};
}

/// Implementation of [api::Central](crate::api::Central).
#[derive(Clone, Debug)]
pub struct Adapter {
    manager: Arc<AdapterManager<Peripheral>>,
}

fn bluetooth() -> Option<web_sys::Bluetooth> {
    web_sys::window().unwrap().navigator().bluetooth()
}

#[async_trait]
trait AddPeripheralAndEmit {
    async fn add_inital_periperals(&self) -> Vec<PeripheralId>;
    fn add_device(&self, device: JsValue) -> Option<PeripheralId>;
}

#[async_trait]
impl AddPeripheralAndEmit for Arc<AdapterManager<Peripheral>> {
    async fn add_inital_periperals(&self) -> Vec<PeripheralId> {
        if !self.peripherals().is_empty() {
            return vec![];
        }

        let self_clone = self.clone();
        spawn_local!({
            wrap_promise::<Array>(bluetooth().unwrap().get_devices())
                .await
                .map_or(vec![], |devices| {
                    devices
                        .iter()
                        .map(|device| self_clone.add_device(device).unwrap())
                        .collect()
                })
        })
    }

    fn add_device(&self, device: JsValue) -> Option<PeripheralId> {
        let p = Peripheral::new(Arc::downgrade(self), BluetoothDevice::from(device));
        let id = p.id();
        if self.peripheral(&id).is_none() {
            self.add_peripheral(p);
            Some(id)
        } else {
            None
        }
    }
}

impl Adapter {
    pub(crate) fn try_new() -> Option<Self> {
        if let Some(_) = bluetooth() {
            Some(Self {
                manager: Arc::new(AdapterManager::default()),
            })
        } else {
            None
        }
    }
}

#[async_trait]
impl Central for Adapter {
    type Peripheral = Peripheral;

    async fn events(&self) -> Result<Pin<Box<dyn Stream<Item = CentralEvent> + Send>>> {
        Ok(self.manager.event_stream())
    }

    async fn start_scan(&self, filter: ScanFilter) -> Result<()> {
        let manager = self.manager.clone();
        spawn_local!({
            for id in manager.add_inital_periperals().await {
                manager.emit(CentralEvent::DeviceDiscovered(id));
            }

            let mut options = RequestDeviceOptions::new();
            let optional_services = Array::new();
            let filters = Array::new();

            for uuid in filter.services.iter() {
                let mut filter = BluetoothLeScanFilterInit::new();
                let filter_services = Array::new();
                filter_services.push(&uuid.to_string().into());
                filter.services(&filter_services.into());
                filters.push(&filter.into());
                optional_services.push(&uuid.to_string().into());
            }

            options.filters(&filters.into());
            options.optional_services(&optional_services.into());

            wrap_promise(bluetooth().unwrap().request_device(&options))
                .await
                .map(|device| {
                    if let Some(id) = manager.add_device(device) {
                        manager.emit(CentralEvent::DeviceDiscovered(id));
                    }
                    ()
                })
        })
    }

    async fn stop_scan(&self) -> Result<()> {
        Ok(())
    }

    async fn peripherals(&self) -> Result<Vec<Peripheral>> {
        self.manager.add_inital_periperals().await;
        Ok(self.manager.peripherals())
    }

    async fn peripheral(&self, id: &PeripheralId) -> Result<Peripheral> {
        self.manager.add_inital_periperals().await;
        self.manager.peripheral(id).ok_or(Error::DeviceNotFound)
    }

    async fn add_peripheral(&self, _address: BDAddr) -> Result<Peripheral> {
        Err(Error::NotSupported(
            "Can't add a Peripheral from a BDAddr".to_string(),
        ))
    }

    async fn adapter_info(&self) -> Result<String> {
        Ok("WebBluetooth".to_string())
    }
}
