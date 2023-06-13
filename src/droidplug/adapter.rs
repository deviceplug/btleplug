use super::{
    jni::{
        global_jvm,
        objects::{JScanFilter, JScanResult},
    },
    peripheral::{Peripheral, PeripheralId},
};
use crate::{
    api::{BDAddr, Central, CentralEvent, PeripheralProperties, ScanFilter},
    common::adapter_manager::AdapterManager,
    Error, Result,
};
use async_trait::async_trait;
use futures::stream::Stream;
use jni::{
    objects::{GlobalRef, JObject, JString},
    strings::JavaStr,
    sys::jboolean,
    JNIEnv,
};
use std::{
    fmt::{Debug, Formatter},
    pin::Pin,
    str::FromStr,
    sync::Arc,
};

#[derive(Clone)]
pub struct Adapter {
    manager: Arc<AdapterManager<Peripheral>>,
    internal: GlobalRef,
}

impl Debug for Adapter {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        f.debug_struct("Adapter")
            .field("manager", &self.manager)
            .finish()
    }
}

impl Adapter {
    pub(crate) fn new() -> Result<Self> {
        let env = global_jvm().get_env()?;

        let obj = env.new_object(
            "com/nonpolynomial/btleplug/android/impl/Adapter",
            "()V",
            &[],
        )?;
        let internal = env.new_global_ref(obj)?;
        let adapter = Self {
            manager: Arc::new(AdapterManager::default()),
            internal,
        };
        env.set_rust_field(obj, "handle", adapter.clone())?;

        Ok(adapter)
    }

    pub fn report_scan_result(&self, scan_result: JObject) -> Result<Peripheral> {
        use std::convert::TryInto;

        let env = global_jvm().get_env()?;
        let scan_result = JScanResult::from_env(&env, scan_result)?;

        let (addr, properties): (BDAddr, Option<PeripheralProperties>) = scan_result.try_into()?;

        match self.manager.peripheral(&PeripheralId(addr)) {
            Some(p) => match properties {
                Some(properties) => {
                    self.report_properties(&p, properties, false);
                    Ok(p)
                }
                None => {
                    //self.manager.emit(CentralEvent::DeviceDisconnected(addr));
                    Err(Error::DeviceNotFound)
                }
            },
            None => match properties {
                Some(properties) => {
                    let p = self.add(addr)?;
                    self.report_properties(&p, properties, true);
                    Ok(p)
                }
                None => Err(Error::DeviceNotFound),
            },
        }
    }

    fn add(&self, address: BDAddr) -> Result<Peripheral> {
        let env = global_jvm().get_env()?;
        let peripheral = Peripheral::new(&env, self.internal.as_obj(), address)?;
        self.manager.add_peripheral(peripheral.clone());
        Ok(peripheral)
    }

    fn report_properties(
        &self,
        peripheral: &Peripheral,
        properties: PeripheralProperties,
        new: bool,
    ) {
        peripheral.report_properties(properties.clone());
        self.manager.emit(if new {
            CentralEvent::DeviceDiscovered(PeripheralId(properties.address))
        } else {
            CentralEvent::DeviceUpdated(PeripheralId(properties.address))
        });
        self.manager
            .emit(CentralEvent::ManufacturerDataAdvertisement {
                id: PeripheralId(properties.address),
                manufacturer_data: properties.manufacturer_data,
            });
        self.manager.emit(CentralEvent::ServiceDataAdvertisement {
            id: PeripheralId(properties.address),
            service_data: properties.service_data,
        });
        self.manager.emit(CentralEvent::ServicesAdvertisement {
            id: PeripheralId(properties.address),
            services: properties.services,
        });
    }
}

#[async_trait]
impl Central for Adapter {
    type Peripheral = Peripheral;

    async fn adapter_info(&self) -> Result<String> {
        // TODO: Get information about the adapter.
        Ok("Android".to_string())
    }

    async fn events(&self) -> Result<Pin<Box<dyn Stream<Item = CentralEvent> + Send>>> {
        Ok(self.manager.event_stream())
    }

    async fn start_scan(&self, filter: ScanFilter) -> Result<()> {
        let env = global_jvm().get_env()?;
        let filter = JScanFilter::new(&env, filter)?;
        env.call_method(
            &self.internal,
            "startScan",
            "(Lcom/nonpolynomial/btleplug/android/impl/ScanFilter;)V",
            &[filter.into()],
        )?;
        Ok(())
    }

    async fn stop_scan(&self) -> Result<()> {
        let env = global_jvm().get_env()?;
        env.call_method(&self.internal, "stopScan", "()V", &[])?;
        Ok(())
    }

    async fn peripherals(&self) -> Result<Vec<Peripheral>> {
        Ok(self.manager.peripherals())
    }

    async fn peripheral(&self, address: &PeripheralId) -> Result<Peripheral> {
        self.manager
            .peripheral(address)
            .ok_or(Error::DeviceNotFound)
    }

    async fn add_peripheral(&self, address: &PeripheralId) -> Result<Peripheral> {
        self.add(address.0)
    }
}

pub(crate) fn adapter_report_scan_result_internal(
    env: &JNIEnv,
    obj: JObject,
    scan_result: JObject,
) -> crate::Result<()> {
    let adapter = env.get_rust_field::<_, _, Adapter>(obj, "handle")?;
    adapter.report_scan_result(scan_result)?;
    Ok(())
}

pub(crate) fn adapter_on_connection_state_changed_internal(
    env: &JNIEnv,
    obj: JObject,
    addr: JString,
    connected: jboolean,
) -> crate::Result<()> {
    let adapter = env.get_rust_field::<_, _, Adapter>(obj, "handle")?;
    let addr_str = JavaStr::from_env(env, addr)?;
    let addr_str = addr_str.to_str().map_err(|e| Error::Other(e.into()))?;
    let addr = BDAddr::from_str(addr_str)?;
    adapter.manager.emit(if connected != 0 {
        CentralEvent::DeviceConnected(PeripheralId(addr))
    } else {
        CentralEvent::DeviceDisconnected(PeripheralId(addr))
    });
    Ok(())
}
