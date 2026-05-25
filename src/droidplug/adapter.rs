use super::{
    jni::{
        jvm,
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
    Env, jni_sig, jni_str,
    objects::{Global, JObject, JString},
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
    internal: Arc<Global<JObject<'static>>>,
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
        jvm()?.attach_current_thread(|env| {
            let obj = env.new_object(
                jni_str!("com/nonpolynomial/btleplug/android/impl/Adapter"),
                jni_sig!("()V"),
                &[],
            )?;
            let internal = Arc::new(env.new_global_ref(&obj)?);
            let adapter = Self {
                manager: Arc::new(AdapterManager::default()),
                internal,
            };
            unsafe { env.set_rust_field(&obj, jni_str!("handle"), adapter.clone()) }?;

            Ok(adapter)
        })
    }

    pub fn report_scan_result<'a>(
        &self,
        env: &mut Env<'a>,
        scan_result: JObject<'a>,
    ) -> Result<Peripheral> {
        let scan_result = env.cast_local::<JScanResult>(scan_result)?;
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
        jvm()?.attach_current_thread(|env| {
            let local_adapter = env.new_local_ref(self.internal.as_obj())?;
            let peripheral = Peripheral::new(env, local_adapter, address)?;
            self.manager.add_peripheral(peripheral.clone());
            Ok(peripheral)
        })
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
        jvm()?.attach_current_thread(|env| {
        let filter = JScanFilter::new(env, filter)?;
        let filter_obj: JObject = filter.into();
        match env.call_method(
            self.internal.as_obj(),
            jni_str!("startScan"),
            jni_sig!("(Lcom/nonpolynomial/btleplug/android/impl/ScanFilter;)V"),
            &[(&filter_obj).into()],
        ) {
            Ok(_) => Ok(()),
            Err(jni::errors::Error::JavaException) => {
                let ex = env.exception_occurred().unwrap();
                env.exception_clear();

                let no_adapter_class = <super::jni::objects::JNoBluetoothAdapterException as jni::objects::Reference>::lookup_class(
                    env,
                    &Default::default(),
                )?;

                if env.is_instance_of(&ex, &*no_adapter_class)? {
                    Err(Error::NoAdapterAvailable)
                } else if env.is_instance_of(&ex, jni_str!("java/lang/RuntimeException"))? {
                    let msg = env
                        .call_method(&ex, jni_str!("getMessage"), jni_sig!("()Ljava/lang/String;"), &[])?
                        .l()?;
                    let jstr = env.cast_local::<JString>(msg)?;
                    let msgstr = String::from(jstr.mutf8_chars(env)?);
                    Err(Error::RuntimeError(msgstr))
                } else {
                    let _ = env.throw(&ex);
                    Err(jni::errors::Error::JavaException.into())
                }
            }
            Err(e) => Err(e.into()),
        }
        })
    }

    async fn stop_scan(&self) -> Result<()> {
        jvm()?.attach_current_thread(|env| {
            env.call_method(
                self.internal.as_obj(),
                jni_str!("stopScan"),
                jni_sig!("()V"),
                &[],
            )?;
            Ok(())
        })
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
    env: &mut Env<'a>,
    obj: &JObject,
    scan_result: JObject<'a>,
) -> crate::Result<()> {
    let adapter = unsafe { env.get_rust_field::<_, _, Adapter>(obj, jni_str!("handle")) }?;
    let adapter_clone = adapter.clone();
    drop(adapter);
    adapter_clone.report_scan_result(env, scan_result)?;
    Ok(())
}

pub(crate) fn adapter_on_connection_state_changed_internal(
    env: &mut Env,
    obj: &JObject,
    addr: JString,
    connected: jboolean,
) -> crate::Result<()> {
    let addr_str = String::from(addr.mutf8_chars(env)?);
    let addr = BDAddr::from_str(&addr_str)?;
    let adapter = unsafe { env.get_rust_field::<_, _, Adapter>(obj, jni_str!("handle")) }?;
    adapter.manager.emit(if connected {
        CentralEvent::DeviceConnected(PeripheralId(addr))
    } else {
        CentralEvent::DeviceDisconnected(PeripheralId(addr))
    });
    Ok(())
}
