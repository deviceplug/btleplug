use crate::{
    api::{
        self, BDAddr, Characteristic, Descriptor, PeripheralProperties, Service, ValueNotification,
        WriteType,
    },
    Error, Result,
};
use async_trait::async_trait;
use futures::stream::Stream;
use jni::{
    descriptors,
    objects::{GlobalRef, JList, JObject},
    JNIEnv,
};
use jni_utils::{
    arrays::byte_array_to_vec, exceptions::try_block, future::JSendFuture, stream::JSendStream,
    task::JPollResult, uuid::JUuid,
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
    sync::{Arc, Mutex},
};

use super::jni::{
    global_jvm,
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

fn get_poll_result<'a: 'b, 'b>(
    env: &'b JNIEnv<'a>,
    result: JPollResult<'a, 'b>,
) -> Result<JObject<'a>> {
    let future_exc =
        jni_utils::classcache::get_class("io/github/gedgygedgy/rust/future/FutureException")
            .unwrap();
    try_block(env, || Ok(Ok(result.get()?)))
        .catch(future_exc.as_obj(), |ex| {
            let cause = env
                .call_method(ex, "getCause", "()Ljava/lang/Throwable;", &[])?
                .l()?;
            if env.is_instance_of(
                cause,
                "com/nonpolynomial/btleplug/android/impl/NotConnectedException",
            )? {
                Ok(Err(Error::NotConnected))
            } else if env.is_instance_of(
                cause,
                "com/nonpolynomial/btleplug/android/impl/PermissionDeniedException",
            )? {
                Ok(Err(Error::PermissionDenied))
            } else {
                env.throw(ex)?;
                Err(jni::errors::Error::JavaException)
            }
        })
        .result()?
}

struct PeripheralShared {
    services: BTreeSet<Service>,
    characteristics: BTreeSet<Characteristic>,
    properties: Option<PeripheralProperties>,
}

#[derive(Clone)]
pub struct Peripheral {
    addr: BDAddr,
    internal: GlobalRef,
    shared: Arc<Mutex<PeripheralShared>>,
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
            })),
        })
    }

    pub(crate) fn report_properties(&self, mut properties: PeripheralProperties) {
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

    async fn properties(&self) -> Result<Option<PeripheralProperties>> {
        let guard = self.shared.lock().unwrap();
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
        let future = self.with_obj(|_env, obj| JSendFuture::try_from(obj.connect()?))?;
        let result_ref = future.await?;
        self.with_obj(|env, _obj| {
            let result = JPollResult::from_env(env, result_ref.as_obj())?;
            get_poll_result(env, result).map(|_| {})
        })
    }

    async fn disconnect(&self) -> Result<()> {
        let future = self.with_obj(|_env, obj| JSendFuture::try_from(obj.disconnect()?))?;
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
        let future = self.with_obj(|_env, obj| JSendFuture::try_from(obj.discover_services()?))?;
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
                let mut characteristics = BTreeSet::new();
                for characteristic in service.get_characteristics()? {
                    let mut descriptors = BTreeSet::new();
                    for descriptor in characteristic.get_descriptors()? {
                        descriptors.insert(Descriptor {
                            uuid: descriptor.get_uuid()?,
                            service_uuid: service.get_uuid()?,
                            characteristic_uuid: characteristic.get_uuid()?,
                        });
                    }
                    characteristics.insert(Characteristic {
                        service_uuid: service.get_uuid()?,
                        uuid: characteristic.get_uuid()?,
                        properties: characteristic.get_properties()?,
                        descriptors: descriptors.clone(),
                    });
                    peripheral_characteristics.push(Characteristic {
                        service_uuid: service.get_uuid()?,
                        uuid: characteristic.get_uuid()?,
                        properties: characteristic.get_properties()?,
                        descriptors: descriptors,
                    });
                }
                peripheral_services.push(Service {
                    uuid: service.get_uuid()?,
                    primary: service.is_primary()?,
                    characteristics,
                })
            }
            let mut guard = self.shared.lock().unwrap();
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
            let data_obj = jni_utils::arrays::slice_to_byte_array(env, data)?;
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
            Ok(byte_array_to_vec(env, bytes.into_inner())?)
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
        let stream = self.with_obj(|_env, obj| JSendStream::try_from(obj.get_notifications()?))?;
        let stream = stream
            .map(|item| match item {
                Ok(item) => {
                    let env = global_jvm().get_env()?;
                    let item = item.as_obj();
                    let characteristic = JBluetoothGattCharacteristic::from_env(&env, item)?;
                    let uuid = characteristic.get_uuid()?;
                    let value = characteristic.get_value()?;
                    Ok(ValueNotification { uuid, value })
                }
                Err(err) => Err(err),
            })
            .filter_map(|item| async { item.ok() });
        Ok(Box::pin(stream))
    }

    async fn write_descriptor(&self, descriptor: &Descriptor, data: &[u8]) -> Result<()> {
        let future = self.with_obj(|env, obj| {
            let characteristic = JUuid::new(env, descriptor.characteristic_uuid)?;
            let uuid = JUuid::new(env, descriptor.uuid)?;
            let data_obj = jni_utils::arrays::slice_to_byte_array(env, data)?;
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
            Ok(byte_array_to_vec(env, bytes.into_inner())?)
        })
    }
}
