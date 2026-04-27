use super::{
    jni::{
        global_jvm,
        objects::{JScanFilter, JScanResult},
    },
    peripheral::{Peripheral, PeripheralId},
};
use crate::{
    Error, Result,
    api::{BDAddr, Central, CentralEvent, CentralState, PeripheralProperties, ScanFilter},
    common::adapter_manager::AdapterManager,
};
use async_trait::async_trait;
use futures::stream::Stream;
use jni::{
    JNIEnv,
    objects::{GlobalRef, JClass, JObject, JString},
    sys::jboolean,
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
        let mut env = global_jvm().get_env()?;

        let obj = env.new_object(
            "com/nonpolynomial/btleplug/android/impl/Adapter",
            "()V",
            &[],
        )?;
        let internal = env.new_global_ref(&obj)?;
        let adapter = Self {
            manager: Arc::new(AdapterManager::default()),
            internal,
        };
        unsafe { env.set_rust_field(&obj, "handle", adapter.clone()) }?;

        Ok(adapter)
    }

    pub fn report_scan_result<'a>(
        &self,
        env: &mut JNIEnv<'a>,
        scan_result: JObject<'a>,
    ) -> Result<Peripheral> {
        let scan_result = JScanResult::from_env(env, scan_result)?;
        let (addr, properties): (BDAddr, Option<PeripheralProperties>) =
            scan_result.to_peripheral_properties(env)?;

        match self.manager.peripheral(&PeripheralId(addr)) {
            Some(p) => match properties {
                Some(properties) => {
                    self.report_properties(&p, properties, false);
                    Ok(p)
                }
                None => Err(Error::DeviceNotFound),
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
        let mut env = global_jvm().get_env()?;
        let local_adapter = env.new_local_ref(&self.internal)?;
        let peripheral = Peripheral::new(&mut env, local_adapter, address)?;
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
        Ok("Android".to_string())
    }

    async fn events(&self) -> Result<Pin<Box<dyn Stream<Item = CentralEvent> + Send>>> {
        Ok(self.manager.event_stream())
    }

    async fn start_scan(&self, filter: ScanFilter) -> Result<()> {
        let mut env = global_jvm().get_env()?;
        let filter = JScanFilter::new(&mut env, filter)?;
        let filter_obj: JObject = filter.into();
        match env.call_method(
            &self.internal,
            "startScan",
            "(Lcom/nonpolynomial/btleplug/android/impl/ScanFilter;)V",
            &[(&filter_obj).into()],
        ) {
            Ok(_) => Ok(()),
            Err(jni::errors::Error::JavaException) => {
                let ex = env.exception_occurred()?;
                env.exception_clear()?;

                let no_adapter_class = super::jni_utils::classcache::get_class(
                    "com/nonpolynomial/btleplug/android/impl/NoBluetoothAdapterException",
                )
                .unwrap();

                if env.is_instance_of(&ex, <&JClass>::from(no_adapter_class.as_obj()))? {
                    Err(Error::NoAdapterAvailable)
                } else if env.is_instance_of(&ex, "java/lang/RuntimeException")? {
                    let msg = env
                        .call_method(&ex, "getMessage", "()Ljava/lang/String;", &[])?
                        .l()?;
                    let jstr: JString = msg.into();
                    let msgstr: String = env.get_string(&jstr)?.into();
                    Err(Error::RuntimeError(msgstr))
                } else {
                    env.throw(&ex)?;
                    Err(jni::errors::Error::JavaException.into())
                }
            }
            Err(e) => Err(e.into()),
        }
    }

    async fn stop_scan(&self) -> Result<()> {
        let mut env = global_jvm().get_env()?;
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

    async fn clear_peripherals(&self) -> Result<()> {
        self.manager.clear_peripherals();
        Ok(())
    }

    async fn adapter_state(&self) -> Result<CentralState> {
        Ok(CentralState::Unknown)
    }
}

pub(crate) fn adapter_report_scan_result_internal<'a>(
    env: &mut JNIEnv<'a>,
    obj: &JObject,
    scan_result: JObject<'a>,
) -> crate::Result<()> {
    let adapter = unsafe { env.get_rust_field::<_, _, Adapter>(obj, "handle") }?;
    let adapter_clone = adapter.clone();
    drop(adapter);
    adapter_clone.report_scan_result(env, scan_result)?;
    Ok(())
}

pub(crate) fn adapter_on_connection_state_changed_internal(
    env: &mut JNIEnv,
    obj: &JObject,
    addr: JString,
    connected: jboolean,
) -> crate::Result<()> {
    let addr_str: String = env.get_string(&addr)?.into();
    let addr = BDAddr::from_str(&addr_str)?;
    let adapter = unsafe { env.get_rust_field::<_, _, Adapter>(obj, "handle") }?;
    adapter.manager.emit(if connected != 0 {
        CentralEvent::DeviceConnected(PeripheralId(addr))
    } else {
        CentralEvent::DeviceDisconnected(PeripheralId(addr))
    });
    Ok(())
}
