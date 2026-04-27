use super::jni_utils::{
    arrays::byte_array_to_vec,
    future::{JFuture, JSendFuture},
    stream::JSendStream,
    task::JPollResult,
    uuid::JUuid,
};
use crate::{
    Error, Result,
    api::{
        self, BDAddr, Characteristic, ConnectionParameterPreset, ConnectionParameters, Descriptor,
        PeripheralProperties, Service, ValueNotification, WriteType,
    },
};
use async_trait::async_trait;
use futures::stream::Stream;
use jni::{
    Env, jni_sig, jni_str,
    objects::{Global, JObject, JString, JValue},
};
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "serde")]
use serde_cr as serde;
use std::{
    collections::BTreeSet,
    fmt::{self, Debug, Display, Formatter},
    pin::Pin,
    sync::atomic::{AtomicU16, Ordering},
    sync::{Arc, Mutex},
};
use super::jni::{
    jvm,
    objects::{JBluetoothGattCharacteristic, JBluetoothGattService, JPeripheral},
};

#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(crate = "serde_cr")
)]
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PeripheralId(pub(super) BDAddr);
impl Display for PeripheralId {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        Display::fmt(&self.0, f)
    }
}

fn get_poll_result<'a>(
    env: &mut Env<'a>,
    result_ref: &Global<JObject<'static>>,
) -> Result<JObject<'a>> {
    let result_obj = env.new_local_ref(result_ref)?;
    let poll_result = env.cast_local::<JPollResult>(result_obj)?;

    match poll_result.get(env) {
        Ok(obj) => Ok(obj),
        Err(jni::errors::Error::JavaException) => {
            let ex = env.exception_occurred().unwrap();
            env.exception_clear();

            use jni::objects::Reference;
            use super::jni::objects::*;

            let future_ex_class = <super::jni_utils::future::JFutureException as Reference>::lookup_class(
                env, &Default::default(),
            )?;

            if env.is_instance_of(&ex, &*future_ex_class)? {
                let cause = env
                    .call_method(&ex, jni_str!("getCause"), jni_sig!("()Ljava/lang/Throwable;"), &[])?
                    .l()?;

                macro_rules! check_exception {
                    ($type:ty, $env:expr, $cause:expr) => {
                        $env.is_instance_of(
                            $cause,
                            &*<$type as Reference>::lookup_class($env, &Default::default())?,
                        )?
                    };
                }

                if check_exception!(JNotConnectedException, env, &cause) {
                    Err(Error::NotConnected)
                } else if check_exception!(JPermissionDeniedException, env, &cause) {
                    Err(Error::PermissionDenied)
                } else if check_exception!(JUnexpectedCallbackException, env, &cause) {
                    Err(Error::UnexpectedCallback)
                } else if check_exception!(JUnexpectedCharacteristicException, env, &cause) {
                    Err(Error::UnexpectedCharacteristic)
                } else if check_exception!(JNoSuchCharacteristicException, env, &cause) {
                    Err(Error::NoSuchCharacteristic)
                } else if check_exception!(JNoBluetoothAdapterException, env, &cause) {
                    Err(Error::NoAdapterAvailable)
                } else if env.is_instance_of(&cause, jni_str!("java/lang/RuntimeException"))? {
                    let msg = env
                        .call_method(&cause, jni_str!("getMessage"), jni_sig!("()Ljava/lang/String;"), &[])?
                        .l()?;
                    let jstr = env.cast_local::<JString>(msg)?;
                    let msgstr = String::from(jstr.mutf8_chars(env)?);
                    Err(Error::RuntimeError(msgstr))
                } else {
                    let _ = env.throw(&ex);
                    Err(jni::errors::Error::JavaException.into())
                }
            } else {
                let _ = env.throw(&ex);
                Err(jni::errors::Error::JavaException.into())
            }
        }
        Err(e) => Err(e.into()),
    }
}

#[derive(Debug)]
struct PeripheralShared {
    services: BTreeSet<Service>,
    characteristics: BTreeSet<Characteristic>,
    properties: Option<PeripheralProperties>,
    mtu: AtomicU16,
}

#[derive(Clone)]
pub struct Peripheral {
    addr: BDAddr,
    internal: Arc<Global<JObject<'static>>>,
    shared: Arc<Mutex<PeripheralShared>>,
    mtu: Arc<AtomicU16>,
}

impl Peripheral {
    pub(crate) fn new<'a>(env: &mut Env<'a>, adapter: JObject<'a>, addr: BDAddr) -> Result<Self> {
        let obj = JPeripheral::create(env, adapter, addr)?;
        let internal = Arc::new(env.new_global_ref(&*obj)?);
        Ok(Self {
            addr,
            internal,
            shared: Arc::new(Mutex::new(PeripheralShared {
                services: BTreeSet::new(),
                characteristics: BTreeSet::new(),
                properties: None,
                mtu: AtomicU16::new(crate::api::DEFAULT_MTU_SIZE),
            })),
            mtu: Arc::new(AtomicU16::new(crate::api::DEFAULT_MTU_SIZE)),
        })
    }

    pub(crate) fn report_properties(&self, properties: PeripheralProperties) {
        let mut guard = self.shared.lock().unwrap();
        guard.properties = Some(properties);
    }

    fn with_obj<T, E>(
        &self,
        f: impl for<'env> FnOnce(&mut Env<'env>, &JPeripheral<'env>) -> std::result::Result<T, E>,
    ) -> std::result::Result<T, E>
    where
        E: From<::jni::errors::Error>,
    {
        jvm()?.attach_current_thread(|env| {
            let local_obj = env.new_local_ref(self.internal.as_obj())?;
            let obj = env.cast_local::<JPeripheral>(local_obj)?;
            f(env, &obj)
        })
    }

    async fn set_characteristic_notification(
        &self,
        characteristic: &Characteristic,
        enable: bool,
    ) -> Result<()> {
        let future = self.with_obj(|env, obj| {
            let uuid_obj = JUuid::new(env, characteristic.uuid)?;
            let future = obj.set_characteristic_notification(env, &uuid_obj, enable)?;
            JSendFuture::new(env, &future)
        })?;
        let result_ref = future.await?;
        self.with_obj(|env, _obj| get_poll_result(env, &result_ref).map(|_| {}))
    }
}

impl Debug for Peripheral {
    fn fmt(&self, fmt: &mut Formatter) -> std::result::Result<(), std::fmt::Error> {
        write!(fmt, "{:?}", self.internal.as_obj())
    }
}

#[async_trait]
impl api::Peripheral for Peripheral {
    fn id(&self) -> PeripheralId {
        PeripheralId(self.addr)
    }

    fn address(&self) -> BDAddr {
        self.addr
    }

    fn mtu(&self) -> u16 {
        self.mtu.load(Ordering::Relaxed)
    }

    async fn properties(&self) -> Result<Option<PeripheralProperties>> {
        let guard = self.shared.lock().map_err(Into::<Error>::into)?;
        Ok((&guard.properties).clone())
    }

    fn characteristics(&self) -> BTreeSet<Characteristic> {
        let guard = self.shared.lock().unwrap();
        (&guard.characteristics).clone()
    }

    async fn is_connected(&self) -> Result<bool> {
        self.with_obj(|env, obj| Ok(obj.is_connected(env)?))
    }

    async fn connect(&self) -> Result<()> {
        {
            let future = self.with_obj(|env, obj| {
                let future = obj.connect(env)?;
                JSendFuture::new(env, &future)
            })?;
            let result_ref = future.await?;
            self.with_obj(|env, _obj| get_poll_result(env, &result_ref).map(|_| {}))?;
        }
        // Query the system-cached device name and update local_name
        self.with_obj(|env, obj| -> std::result::Result<(), Error> {
            if let Ok(Some(name)) = obj.get_device_name(env) {
                let mut guard = self.shared.lock().map_err(Into::<Error>::into)?;
                if let Some(ref mut props) = guard.properties {
                    props.local_name = Some(name);
                }
            }
            Ok(())
        })?;
        // Auto-negotiate maximum MTU (517) after connection
        {
            let mtu_future = self.with_obj(|env, obj| {
                let mtu_obj = obj.request_mtu(env, 517)?;
                let mtu_future = env.cast_local::<JFuture>(mtu_obj)?;
                JSendFuture::new(env, &mtu_future)
            })?;
            let mtu_result_ref = mtu_future.await?;
            self.with_obj(|env, _obj| -> Result<()> {
                let mtu_obj = get_poll_result(env, &mtu_result_ref)?;
                let mtu_val = env.call_method(&mtu_obj, jni_str!("intValue"), jni_sig!("()I"), &[])?.i()?;
                self.mtu.store(mtu_val as u16, Ordering::Relaxed);
                Ok(())
            })?;
        }
        Ok(())
    }

    async fn disconnect(&self) -> Result<()> {
        let future = self.with_obj(|env, obj| {
            let future = obj.disconnect(env)?;
            JSendFuture::new(env, &future)
        })?;
        let result_ref = future.await?;
        self.with_obj(|env, _obj| get_poll_result(env, &result_ref).map(|_| {}))
    }

    fn services(&self) -> BTreeSet<Service> {
        let guard = self.shared.lock().unwrap();
        (&guard.services).clone()
    }

    async fn discover_services(&self) -> Result<()> {
        let future = self.with_obj(|env, obj| {
            let future = obj.discover_services(env)?;
            JSendFuture::new(env, &future)
        })?;
        let result_ref = future.await?;
        self.with_obj(|env, _obj| {
            use std::iter::FromIterator;

            let obj = get_poll_result(env, &result_ref)?;
            let size = env.call_method(&obj, jni_str!("size"), jni_sig!("()I"), &[])?.i()?;
            let mut peripheral_services = Vec::new();
            let mut peripheral_characteristics = Vec::new();

            for i in 0..size {
                let svc_obj = env
                    .call_method(&obj, jni_str!("get"), jni_sig!("(I)Ljava/lang/Object;"), &[JValue::from(i)])?
                    .l()?;
                let service = env.cast_local::<JBluetoothGattService>(svc_obj)?;
                let mut characteristics = BTreeSet::<Characteristic>::new();
                for characteristic in service.get_characteristics(env)? {
                    let mut descriptors = BTreeSet::new();
                    for descriptor in characteristic.get_descriptors(env)? {
                        descriptors.insert(Descriptor {
                            uuid: descriptor.get_uuid(env)?,
                            service_uuid: service.get_uuid(env)?,
                            characteristic_uuid: characteristic.get_uuid(env)?,
                        });
                    }
                    let char = Characteristic {
                        service_uuid: service.get_uuid(env)?,
                        uuid: characteristic.get_uuid(env)?,
                        properties: characteristic.get_properties(env)?,
                        descriptors: descriptors.clone(),
                    };
                    if characteristics
                        .iter()
                        .filter(|c| c.service_uuid == char.service_uuid && c.uuid == char.uuid)
                        .count()
                        == 0
                    {
                        characteristics.insert(char.clone());
                        peripheral_characteristics.push(char.clone());
                    }
                }
                peripheral_services.push(Service {
                    uuid: service.get_uuid(env)?,
                    primary: service.is_primary()?,
                    characteristics,
                })
            }
            let mut guard = self.shared.lock().map_err(Into::<Error>::into)?;
            guard.services = BTreeSet::from_iter(peripheral_services.clone());
            guard.characteristics = BTreeSet::from_iter(peripheral_characteristics.clone());
            Ok(())
        })
    }

    async fn write(
        &self,
        characteristic: &Characteristic,
        data: &[u8],
        write_type: WriteType,
    ) -> Result<()> {
        let future = self.with_obj(|env, obj| {
            let uuid = JUuid::new(env, characteristic.uuid)?;
            let data_obj = super::jni_utils::arrays::slice_to_byte_array(env, data)?;
            let write_type = match write_type {
                WriteType::WithResponse => 2,
                WriteType::WithoutResponse => 1,
            };
            let future = obj.write(env, &uuid, &data_obj.into(), write_type)?;
            JSendFuture::new(env, &future)
        })?;
        let result_ref = future.await?;
        self.with_obj(|env, _obj| get_poll_result(env, &result_ref).map(|_| {}))
    }

    async fn read(&self, characteristic: &Characteristic) -> Result<Vec<u8>> {
        let future = self.with_obj(|env, obj| {
            let uuid = JUuid::new(env, characteristic.uuid)?;
            let future = obj.read(env, &uuid)?;
            JSendFuture::new(env, &future)
        })?;
        let result_ref = future.await?;
        self.with_obj(|env, _obj| {
            let bytes_obj = get_poll_result(env, &result_ref)?;
            let bytes_arr = unsafe { jni::objects::JByteArray::from_raw(env, bytes_obj.into_raw()) };
            Ok(byte_array_to_vec(env, &bytes_arr)?)
        })
    }

    async fn subscribe(&self, characteristic: &Characteristic) -> Result<()> {
        self.set_characteristic_notification(characteristic, true)
            .await
    }

    async fn unsubscribe(&self, characteristic: &Characteristic) -> Result<()> {
        self.set_characteristic_notification(characteristic, false)
            .await
    }

    async fn notifications(&self) -> Result<Pin<Box<dyn Stream<Item = ValueNotification> + Send>>> {
        use futures::stream::StreamExt;
        let shared = self.shared.clone();
        let stream = self.with_obj(|env, obj| {
            let stream = obj.get_notifications(env)?;
            JSendStream::new(env, &stream)
        })?;
        let stream = stream
            .map(move |item| match item {
                Ok(item) => {
                    jvm()?.attach_current_thread(|env| {
                        let local_obj = env.new_local_ref(item.as_obj())?;
                        let characteristic =
                            env.cast_local::<JBluetoothGattCharacteristic>(local_obj)?;
                        let uuid = characteristic.get_uuid(env)?;
                        let value = characteristic.get_value(env)?;
                        let service_uuid = shared
                            .lock()
                            .ok()
                            .and_then(|guard| {
                                guard
                                    .services
                                    .iter()
                                    .find(|s| s.characteristics.iter().any(|c| c.uuid == uuid))
                                    .map(|s| s.uuid)
                            })
                            .unwrap_or_default();
                        Ok(ValueNotification {
                            uuid,
                            service_uuid,
                            value,
                        })
                    })
                }
                Err(err) => Err(err),
            })
            .filter_map(|item| async { item.ok() });
        Ok(Box::pin(stream))
    }

    async fn read_rssi(&self) -> Result<i16> {
        let future = self.with_obj(|env, obj| {
            let rssi_obj = obj.read_remote_rssi(env)?;
            let rssi_future = env.cast_local::<JFuture>(rssi_obj)?;
            JSendFuture::new(env, &rssi_future)
        })?;
        let result_ref = future.await?;
        self.with_obj(|env, _obj| {
            let rssi_obj = get_poll_result(env, &result_ref)?;
            let rssi_val = env.call_method(&rssi_obj, jni_str!("intValue"), jni_sig!("()I"), &[])?.i()?;
            Ok(rssi_val as i16)
        })
    }

    async fn write_descriptor(&self, descriptor: &Descriptor, data: &[u8]) -> Result<()> {
        let future = self.with_obj(|env, obj| {
            let characteristic = JUuid::new(env, descriptor.characteristic_uuid)?;
            let uuid = JUuid::new(env, descriptor.uuid)?;
            let data_obj = super::jni_utils::arrays::slice_to_byte_array(env, data)?;
            let future = obj.write_descriptor(env, &characteristic, &uuid, &data_obj.into())?;
            JSendFuture::new(env, &future)
        })?;
        let result_ref = future.await?;
        self.with_obj(|env, _obj| get_poll_result(env, &result_ref).map(|_| {}))
    }

    async fn read_descriptor(&self, descriptor: &Descriptor) -> Result<Vec<u8>> {
        let future = self.with_obj(|env, obj| {
            let characteristic = JUuid::new(env, descriptor.characteristic_uuid)?;
            let uuid = JUuid::new(env, descriptor.uuid)?;
            let future = obj.read_descriptor(env, &characteristic, &uuid)?;
            JSendFuture::new(env, &future)
        })?;
        let result_ref = future.await?;
        self.with_obj(|env, _obj| {
            let bytes_obj = get_poll_result(env, &result_ref)?;
            let bytes_arr = unsafe { jni::objects::JByteArray::from_raw(env, bytes_obj.into_raw()) };
            Ok(byte_array_to_vec(env, &bytes_arr)?)
        })
    }

    async fn connection_parameters(&self) -> Result<Option<ConnectionParameters>> {
        self.with_obj(|env, obj| {
            Ok(obj
                .get_connection_parameters(env)
                .map_err(|e| Error::Other(format!("{:?}", e).into()))?)
        })
    }

    async fn request_connection_parameters(&self, preset: ConnectionParameterPreset) -> Result<()> {
        let priority = match preset {
            ConnectionParameterPreset::Balanced => 0,
            ConnectionParameterPreset::ThroughputOptimized => 1,
            ConnectionParameterPreset::PowerOptimized => 2,
        };
        self.with_obj(|env, obj| {
            let success = obj
                .request_connection_priority(env, priority)
                .map_err(|e| Error::Other(format!("{:?}", e).into()))?;
            if success {
                Ok(())
            } else {
                Err(Error::RuntimeError(
                    "requestConnectionPriority returned false".to_string(),
                ))
            }
        })
    }
}
