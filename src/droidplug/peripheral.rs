use super::jni_utils::{
    arrays::byte_array_to_vec,
    exceptions::try_block,
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
    JNIEnv,
    objects::{GlobalRef, JList, JObject},
};
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "serde")]
use serde_cr as serde;
use std::{
    collections::BTreeSet,
    convert::TryFrom,
    fmt::{self, Debug, Display, Formatter},
    pin::Pin,
    sync::atomic::{AtomicU16, Ordering},
    sync::{Arc, Mutex},
};
use uuid::Uuid;

use super::jni::{
    global_jvm,
    objects::{JBluetoothGattCharacteristic, JBluetoothGattService, JPeripheral},
};
use jni::objects::JClass;
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

fn get_poll_result<'a: 'b, 'b>(
    env: &'b JNIEnv<'a>,
    result: JPollResult<'a, 'b>,
) -> Result<JObject<'a>> {
    try_block(env, || Ok(Ok(result.get()?)))
        .catch(
            JClass::from(
                super::jni_utils::classcache::get_class(
                    "io/github/gedgygedgy/rust/future/FutureException",
                )
                .unwrap()
                .as_obj(),
            ),
            |ex| {
                let cause = env
                    .call_method(ex, "getCause", "()Ljava/lang/Throwable;", &[])?
                    .l()?;
                if env.is_instance_of(
                    cause,
                    JClass::from(
                        super::jni_utils::classcache::get_class(
                            "com/nonpolynomial/btleplug/android/impl/NotConnectedException",
                        )
                        .unwrap()
                        .as_obj(),
                    ),
                )? {
                    Ok(Err(Error::NotConnected))
                } else if env.is_instance_of(
                    cause,
                    JClass::from(
                        super::jni_utils::classcache::get_class(
                            "com/nonpolynomial/btleplug/android/impl/PermissionDeniedException",
                        )
                        .unwrap()
                        .as_obj(),
                    ),
                )? {
                    Ok(Err(Error::PermissionDenied))
                } else if env.is_instance_of(
                    cause,
                    JClass::from(
                        super::jni_utils::classcache::get_class(
                            "com/nonpolynomial/btleplug/android/impl/UnexpectedCallbackException",
                        )
                        .unwrap()
                        .as_obj(),
                    ),
                )? {
                    Ok(Err(Error::UnexpectedCallback))
                } else if env.is_instance_of(
                    cause,
                    JClass::from(
                        super::jni_utils::classcache::get_class(
                            "com/nonpolynomial/btleplug/android/impl/UnexpectedCharacteristicException",
                        )
                        .unwrap()
                        .as_obj(),
                    ),
                )? {
                    Ok(Err(Error::UnexpectedCharacteristic))
                } else if env.is_instance_of(
                    cause,
                    JClass::from(
                        super::jni_utils::classcache::get_class(
                            "com/nonpolynomial/btleplug/android/impl/NoSuchCharacteristicException",
                        )
                        .unwrap()
                        .as_obj(),
                    ),
                )? {
                    Ok(Err(Error::NoSuchCharacteristic))
                } else if env.is_instance_of(
                    cause,
                    JClass::from(
                        super::jni_utils::classcache::get_class(
                            "com/nonpolynomial/btleplug/android/impl/NoBluetoothAdapterException",
                        )
                        .unwrap()
                        .as_obj(),
                    ),
                )? {
                    Ok(Err(Error::NoAdapterAvailable))
                } else if env.is_instance_of(
                    cause,
                    "java/lang/RuntimeException",
                )? {
                    let msg = env
                        .call_method(cause, "getMessage", "()Ljava/lang/String;", &[])
                        .unwrap()
                        .l()
                        .unwrap();
                    let msgstr:String = env.get_string(msg.into()).unwrap().into();
                    Ok(Err(Error::RuntimeError(msgstr)))
                } else {
                    env.throw(ex)?;
                    Err(jni::errors::Error::JavaException)
                }
            },
        )
        .result()?
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
    internal: GlobalRef,
    shared: Arc<Mutex<PeripheralShared>>,
    mtu: Arc<AtomicU16>,
}

impl Peripheral {
    pub(crate) fn new(env: &JNIEnv, adapter: JObject, addr: BDAddr) -> Result<Self> {
        let obj = JPeripheral::new(env, adapter, addr)?;
        Ok(Self {
            addr,
            internal: env.new_global_ref(obj)?,
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
        f: impl FnOnce(&JNIEnv, JPeripheral) -> std::result::Result<T, E>,
    ) -> std::result::Result<T, E>
    where
        E: From<::jni::errors::Error>,
    {
        let env = global_jvm().get_env()?;
        let obj = JPeripheral::from_env(&env, self.internal.as_obj())?;
        f(&env, obj)
    }

    async fn set_characteristic_notification(
        &self,
        characteristic: &Characteristic,
        enable: bool,
    ) -> Result<()> {
        let future = self.with_obj(|env, obj| {
            let uuid_obj = JUuid::new(env, characteristic.uuid)?;
            JSendFuture::try_from(obj.set_characteristic_notification(uuid_obj, enable)?)
        })?;
        let result_ref = future.await?;
        self.with_obj(|env, _obj| {
            let result = JPollResult::from_env(env, result_ref.as_obj())?;
            get_poll_result(env, result).map(|_| {})
        })
    }
}

impl Debug for Peripheral {
    fn fmt(&self, fmt: &mut Formatter) -> std::result::Result<(), std::fmt::Error> {
        write!(fmt, "{:?}", self.internal.as_obj())
    }
}

#[async_trait]
impl api::Peripheral for Peripheral {
    /// Returns the unique identifier of the peripheral.
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
        self.with_obj(|_env, obj| Ok(obj.is_connected()?))
    }

    async fn connect(&self) -> Result<()> {
        let future = self.with_obj(|env, obj| JSendFuture::try_from(obj.connect()?))?;
        let result_ref = future.await?;
        self.with_obj(|env, _obj| {
            let result = JPollResult::from_env(env, result_ref.as_obj())?;
            get_poll_result(env, result).map(|_| {})
        })?;
        // Query the system-cached device name and update local_name
        self.with_obj(|_env, obj| -> std::result::Result<(), Error> {
            if let Ok(Some(name)) = obj.get_device_name() {
                let mut guard = self.shared.lock().map_err(Into::<Error>::into)?;
                if let Some(ref mut props) = guard.properties {
                    props.local_name = Some(name);
                }
            }
            Ok(())
        })?;
        // Auto-negotiate maximum MTU (517) after connection
        let mtu_future = self.with_obj(|env, obj| {
            JSendFuture::try_from(JFuture::from_env(env, obj.request_mtu(517)?)?)
        })?;
        let mtu_result_ref = mtu_future.await?;
        self.with_obj(|env, _obj| -> Result<()> {
            let mtu_result = JPollResult::from_env(env, mtu_result_ref.as_obj())?;
            let mtu_obj = get_poll_result(env, mtu_result)?;
            let mtu_val = env.call_method(mtu_obj, "intValue", "()I", &[])?.i()?;
            self.mtu.store(mtu_val as u16, Ordering::Relaxed);
            Ok(())
        })?;
        Ok(())
    }

    async fn disconnect(&self) -> Result<()> {
        let future = self.with_obj(|env, obj| JSendFuture::try_from(obj.disconnect()?))?;
        let result_ref = future.await?;
        self.with_obj(|env, _obj| {
            let result = JPollResult::from_env(env, result_ref.as_obj())?;
            get_poll_result(env, result).map(|_| {})
        })
    }

    /// The set of services we've discovered for this device. This will be empty until
    /// `discover_services` is called.
    fn services(&self) -> BTreeSet<Service> {
        let guard = self.shared.lock().unwrap();
        (&guard.services).clone()
    }

    async fn discover_services(&self) -> Result<()> {
        let future = self.with_obj(|env, obj| JSendFuture::try_from(obj.discover_services()?))?;
        let result_ref = future.await?;
        self.with_obj(|env, _obj| {
            use std::iter::FromIterator;

            let result = JPollResult::from_env(env, result_ref.as_obj())?;
            let obj = get_poll_result(env, result)?;
            let list = JList::from_env(env, obj)?;
            let mut peripheral_services = Vec::new();
            let mut peripheral_characteristics = Vec::new();

            for service in list.iter()? {
                let service = JBluetoothGattService::from_env(env, service)?;
                let mut characteristics = BTreeSet::<Characteristic>::new();
                for characteristic in service.get_characteristics()? {
                    let mut descriptors = BTreeSet::new();
                    for descriptor in characteristic.get_descriptors()? {
                        descriptors.insert(Descriptor {
                            uuid: descriptor.get_uuid()?,
                            service_uuid: service.get_uuid()?,
                            characteristic_uuid: characteristic.get_uuid()?,
                        });
                    }
                    let char = Characteristic {
                        service_uuid: service.get_uuid()?,
                        uuid: characteristic.get_uuid()?,
                        properties: characteristic.get_properties()?,
                        descriptors: descriptors.clone(),
                    };
                    // Only consider the first characteristic of each UUID
                    // This "should" be unique, but of course it's not enforced
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
                    uuid: service.get_uuid()?,
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
            JSendFuture::try_from(obj.write(uuid, data_obj.into(), write_type)?)
        })?;
        let result_ref = future.await?;
        self.with_obj(|env, _obj| {
            let result = JPollResult::from_env(env, result_ref.as_obj())?;
            get_poll_result(env, result).map(|_| {})
        })
    }

    async fn read(&self, characteristic: &Characteristic) -> Result<Vec<u8>> {
        let future = self.with_obj(|env, obj| {
            let uuid = JUuid::new(env, characteristic.uuid)?;
            JSendFuture::try_from(obj.read(uuid)?)
        })?;
        let result_ref = future.await?;
        self.with_obj(|env, _obj| {
            let result = JPollResult::from_env(env, result_ref.as_obj())?;
            let bytes = get_poll_result(env, result)?;
            Ok(byte_array_to_vec(env, bytes.into_raw())?)
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
        let stream = self.with_obj(|_env, obj| JSendStream::try_from(obj.get_notifications()?))?;
        let stream = stream
            .map(move |item| match item {
                Ok(item) => {
                    let env = global_jvm().get_env()?;
                    let item = item.as_obj();
                    let characteristic = JBluetoothGattCharacteristic::from_env(&env, item)?;
                    let uuid = characteristic.get_uuid()?;
                    let value = characteristic.get_value()?;
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
                }
                Err(err) => Err(err),
            })
            .filter_map(|item| async { item.ok() });
        Ok(Box::pin(stream))
    }

    async fn read_rssi(&self) -> Result<i16> {
        let future = self.with_obj(|env, obj| {
            JSendFuture::try_from(JFuture::from_env(env, obj.read_remote_rssi()?)?)
        })?;
        let result_ref = future.await?;
        self.with_obj(|env, _obj| {
            let result = JPollResult::from_env(env, result_ref.as_obj())?;
            let rssi_obj = get_poll_result(env, result)?;
            let rssi_val = env.call_method(rssi_obj, "intValue", "()I", &[])?.i()?;
            Ok(rssi_val as i16)
        })
    }

    async fn write_descriptor(&self, descriptor: &Descriptor, data: &[u8]) -> Result<()> {
        let future = self.with_obj(|env, obj| {
            let characteristic = JUuid::new(env, descriptor.characteristic_uuid)?;
            let uuid = JUuid::new(env, descriptor.uuid)?;
            let data_obj = super::jni_utils::arrays::slice_to_byte_array(env, data)?;
            JSendFuture::try_from(obj.write_descriptor(characteristic, uuid, data_obj.into())?)
        })?;
        let result_ref = future.await?;
        self.with_obj(|env, _obj| {
            let result = JPollResult::from_env(env, result_ref.as_obj())?;
            get_poll_result(env, result).map(|_| {})
        })
    }

    async fn read_descriptor(&self, descriptor: &Descriptor) -> Result<Vec<u8>> {
        let future = self.with_obj(|env, obj| {
            let characteristic = JUuid::new(env, descriptor.characteristic_uuid)?;
            let uuid = JUuid::new(env, descriptor.uuid)?;
            JSendFuture::try_from(obj.read_descriptor(characteristic, uuid)?)
        })?;
        let result_ref = future.await?;
        self.with_obj(|env, _obj| {
            let result = JPollResult::from_env(env, result_ref.as_obj())?;
            let bytes = get_poll_result(env, result)?;
            Ok(byte_array_to_vec(env, bytes.into_raw())?)
        })
    }

    async fn connection_parameters(&self) -> Result<Option<ConnectionParameters>> {
        self.with_obj(|_env, obj| {
            Ok(obj
                .get_connection_parameters()
                .map_err(|e| Error::Other(format!("{:?}", e).into()))?)
        })
    }

    async fn request_connection_parameters(&self, preset: ConnectionParameterPreset) -> Result<()> {
        let priority = match preset {
            ConnectionParameterPreset::Balanced => 0, // CONNECTION_PRIORITY_BALANCED
            ConnectionParameterPreset::ThroughputOptimized => 1, // CONNECTION_PRIORITY_HIGH
            ConnectionParameterPreset::PowerOptimized => 2, // CONNECTION_PRIORITY_LOW_POWER
        };
        self.with_obj(|_env, obj| {
            let success = obj
                .request_connection_priority(priority)
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
