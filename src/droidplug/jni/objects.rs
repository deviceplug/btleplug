use crate::droidplug::jni_utils::{future::JFuture, stream::JStream, uuid::JUuid};
use jni::{
    JNIEnv,
    errors::Result,
    objects::{JClass, JMethodID, JObject, JString},
    signature::{Primitive, ReturnType},
    sys::{jint, jvalue},
};
use std::{collections::HashMap, iter::Iterator};
use uuid::Uuid;

use crate::api::{BDAddr, CharPropFlags, PeripheralProperties, ScanFilter};

pub struct JPeripheral<'a> {
    internal: JObject<'a>,
    connect: JMethodID,
    disconnect: JMethodID,
    is_connected: JMethodID,
    discover_services: JMethodID,
    read: JMethodID,
    write: JMethodID,
    set_characteristic_notification: JMethodID,
    get_notifications: JMethodID,
    read_descriptor: JMethodID,
    write_descriptor: JMethodID,
    get_device_name: JMethodID,
    request_mtu: JMethodID,
    get_connection_parameters: JMethodID,
    request_connection_priority: JMethodID,
    read_remote_rssi: JMethodID,
}

impl<'a> ::std::ops::Deref for JPeripheral<'a> {
    type Target = JObject<'a>;

    fn deref(&self) -> &Self::Target {
        &self.internal
    }
}

impl<'a> From<JPeripheral<'a>> for JObject<'a> {
    fn from(other: JPeripheral<'a>) -> JObject<'a> {
        other.internal
    }
}

impl<'a> JPeripheral<'a> {
    pub fn from_env(env: &mut JNIEnv<'a>, obj: JObject<'a>) -> Result<Self> {
        let class_static = crate::droidplug::jni_utils::classcache::get_class(
            "com/nonpolynomial/btleplug/android/impl/Peripheral",
        )
        .unwrap();
        let class = <&JClass>::from(class_static.as_obj());

        let connect = env.get_method_id(
            class,
            "connect",
            "()Lio/github/gedgygedgy/rust/future/Future;",
        )?;
        let disconnect = env.get_method_id(
            class,
            "disconnect",
            "()Lio/github/gedgygedgy/rust/future/Future;",
        )?;
        let is_connected = env.get_method_id(class, "isConnected", "()Z")?;
        let discover_services = env.get_method_id(
            class,
            "discoverServices",
            "()Lio/github/gedgygedgy/rust/future/Future;",
        )?;
        let read = env.get_method_id(
            class,
            "read",
            "(Ljava/util/UUID;)Lio/github/gedgygedgy/rust/future/Future;",
        )?;
        let write = env.get_method_id(
            class,
            "write",
            "(Ljava/util/UUID;[BI)Lio/github/gedgygedgy/rust/future/Future;",
        )?;
        let set_characteristic_notification = env.get_method_id(
            class,
            "setCharacteristicNotification",
            "(Ljava/util/UUID;Z)Lio/github/gedgygedgy/rust/future/Future;",
        )?;
        let get_notifications = env.get_method_id(
            class,
            "getNotifications",
            "()Lio/github/gedgygedgy/rust/stream/Stream;",
        )?;
        let read_descriptor = env.get_method_id(
            class,
            "readDescriptor",
            "(Ljava/util/UUID;Ljava/util/UUID;)Lio/github/gedgygedgy/rust/future/Future;",
        )?;
        let write_descriptor = env.get_method_id(
            class,
            "writeDescriptor",
            "(Ljava/util/UUID;Ljava/util/UUID;[B)Lio/github/gedgygedgy/rust/future/Future;",
        )?;
        let get_device_name = env.get_method_id(class, "getDeviceName", "()Ljava/lang/String;")?;
        let request_mtu = env.get_method_id(
            class,
            "requestMtu",
            "(I)Lio/github/gedgygedgy/rust/future/Future;",
        )?;
        let get_connection_parameters =
            env.get_method_id(class, "getConnectionParameters", "()[I")?;
        let request_connection_priority =
            env.get_method_id(class, "requestConnectionPriority", "(I)Z")?;
        let read_remote_rssi = env.get_method_id(
            class,
            "readRemoteRssi",
            "()Lio/github/gedgygedgy/rust/future/Future;",
        )?;
        Ok(Self {
            internal: obj,
            connect,
            disconnect,
            is_connected,
            discover_services,
            read,
            write,
            set_characteristic_notification,
            get_notifications,
            read_descriptor,
            write_descriptor,
            get_device_name,
            request_mtu,
            get_connection_parameters,
            request_connection_priority,
            read_remote_rssi,
        })
    }

    pub fn new(env: &mut JNIEnv<'a>, adapter: JObject<'a>, addr: BDAddr) -> Result<Self> {
        let addr_jstr = env.new_string(format!("{:X}", addr))?;
        let class_static = crate::droidplug::jni_utils::classcache::get_class(
            "com/nonpolynomial/btleplug/android/impl/Peripheral",
        )
        .unwrap();
        let obj = env.new_object(
            <&JClass>::from(class_static.as_obj()),
            "(Lcom/nonpolynomial/btleplug/android/impl/Adapter;Ljava/lang/String;)V",
            &[(&adapter).into(), (&addr_jstr).into()],
        )?;
        Self::from_env(env, obj)
    }

    pub fn connect(&self, env: &mut JNIEnv<'a>) -> Result<JFuture<'a>> {
        let future_obj = unsafe {
            env.call_method_unchecked(&self.internal, self.connect, ReturnType::Object, &[])
        }?
        .l()?;
        JFuture::from_env(env, future_obj)
    }

    pub fn disconnect(&self, env: &mut JNIEnv<'a>) -> Result<JFuture<'a>> {
        let future_obj = unsafe {
            env.call_method_unchecked(&self.internal, self.disconnect, ReturnType::Object, &[])
        }?
        .l()?;
        JFuture::from_env(env, future_obj)
    }

    pub fn is_connected(&self, env: &mut JNIEnv<'a>) -> Result<bool> {
        unsafe {
            env.call_method_unchecked(
                &self.internal,
                self.is_connected,
                ReturnType::Primitive(Primitive::Boolean),
                &[],
            )
        }?
        .z()
    }

    pub fn discover_services(&self, env: &mut JNIEnv<'a>) -> Result<JFuture<'a>> {
        let future_obj = unsafe {
            env.call_method_unchecked(
                &self.internal,
                self.discover_services,
                ReturnType::Object,
                &[],
            )
        }?
        .l()?;
        JFuture::from_env(env, future_obj)
    }

    pub fn read(&self, env: &mut JNIEnv<'a>, uuid: &JUuid<'a>) -> Result<JFuture<'a>> {
        let future_obj = unsafe {
            env.call_method_unchecked(
                &self.internal,
                self.read,
                ReturnType::Object,
                &[jvalue {
                    l: uuid.as_obj().as_raw(),
                }],
            )
        }?
        .l()?;
        JFuture::from_env(env, future_obj)
    }

    pub fn write(
        &self,
        env: &mut JNIEnv<'a>,
        uuid: &JUuid<'a>,
        data: &JObject<'a>,
        write_type: jint,
    ) -> Result<JFuture<'a>> {
        let future_obj = unsafe {
            env.call_method_unchecked(
                &self.internal,
                self.write,
                ReturnType::Object,
                &[
                    jvalue {
                        l: uuid.as_obj().as_raw(),
                    },
                    jvalue {
                        l: data.as_raw(),
                    },
                    jvalue { i: write_type },
                ],
            )
        }?
        .l()?;
        JFuture::from_env(env, future_obj)
    }

    pub fn set_characteristic_notification(
        &self,
        env: &mut JNIEnv<'a>,
        uuid: &JUuid<'a>,
        enable: bool,
    ) -> Result<JFuture<'a>> {
        let future_obj = unsafe {
            env.call_method_unchecked(
                &self.internal,
                self.set_characteristic_notification,
                ReturnType::Object,
                &[
                    jvalue {
                        l: uuid.as_obj().as_raw(),
                    },
                    jvalue {
                        z: enable as u8,
                    },
                ],
            )
        }?
        .l()?;
        JFuture::from_env(env, future_obj)
    }

    pub fn get_notifications(&self, env: &mut JNIEnv<'a>) -> Result<JStream<'a>> {
        let stream_obj = unsafe {
            env.call_method_unchecked(
                &self.internal,
                self.get_notifications,
                ReturnType::Object,
                &[],
            )
        }?
        .l()?;
        JStream::from_env(env, stream_obj)
    }

    pub fn read_descriptor(
        &self,
        env: &mut JNIEnv<'a>,
        characteristic: &JUuid<'a>,
        uuid: &JUuid<'a>,
    ) -> Result<JFuture<'a>> {
        let future_obj = unsafe {
            env.call_method_unchecked(
                &self.internal,
                self.read_descriptor,
                ReturnType::Object,
                &[
                    jvalue {
                        l: characteristic.as_obj().as_raw(),
                    },
                    jvalue {
                        l: uuid.as_obj().as_raw(),
                    },
                ],
            )
        }?
        .l()?;
        JFuture::from_env(env, future_obj)
    }

    pub fn get_device_name(&self, env: &mut JNIEnv<'a>) -> Result<Option<String>> {
        let obj = unsafe {
            env.call_method_unchecked(
                &self.internal,
                self.get_device_name,
                ReturnType::Object,
                &[],
            )
        }?
        .l()?;
        if obj.is_null() {
            Ok(None)
        } else {
            let jstr: JString = obj.into();
            let name_str = env.get_string(&jstr)?;
            Ok(Some(name_str.into()))
        }
    }

    pub fn request_mtu(&self, env: &mut JNIEnv<'a>, mtu: jint) -> Result<JObject<'a>> {
        unsafe {
            env.call_method_unchecked(
                &self.internal,
                self.request_mtu,
                ReturnType::Object,
                &[jvalue { i: mtu }],
            )
        }?
        .l()
    }

    pub fn get_connection_parameters(
        &self,
        env: &mut JNIEnv<'a>,
    ) -> Result<Option<crate::api::ConnectionParameters>> {
        let obj = unsafe {
            env.call_method_unchecked(
                &self.internal,
                self.get_connection_parameters,
                ReturnType::Array,
                &[],
            )
        }?
        .l()?;
        if obj.is_null() {
            return Ok(None);
        }
        let arr = unsafe { jni::objects::JIntArray::from_raw(obj.into_raw()) };
        let len = env.get_array_length(&arr)?;
        if len < 3 {
            return Ok(None);
        }
        let mut buf = [0i32; 3];
        env.get_int_array_region(&arr, 0, &mut buf)?;
        Ok(Some(crate::api::ConnectionParameters {
            interval_us: (buf[0] as u32) * 1250,
            latency: buf[1] as u16,
            supervision_timeout_us: (buf[2] as u32) * 10_000,
        }))
    }

    pub fn read_remote_rssi(&self, env: &mut JNIEnv<'a>) -> Result<JObject<'a>> {
        unsafe {
            env.call_method_unchecked(
                &self.internal,
                self.read_remote_rssi,
                ReturnType::Object,
                &[],
            )
        }?
        .l()
    }

    pub fn request_connection_priority(
        &self,
        env: &mut JNIEnv<'a>,
        priority: jint,
    ) -> Result<bool> {
        unsafe {
            env.call_method_unchecked(
                &self.internal,
                self.request_connection_priority,
                ReturnType::Primitive(Primitive::Boolean),
                &[jvalue { i: priority }],
            )
        }?
        .z()
    }

    pub fn write_descriptor(
        &self,
        env: &mut JNIEnv<'a>,
        characteristic: &JUuid<'a>,
        uuid: &JUuid<'a>,
        data: &JObject<'a>,
    ) -> Result<JFuture<'a>> {
        let future_obj = unsafe {
            env.call_method_unchecked(
                &self.internal,
                self.write_descriptor,
                ReturnType::Object,
                &[
                    jvalue {
                        l: characteristic.as_obj().as_raw(),
                    },
                    jvalue {
                        l: uuid.as_obj().as_raw(),
                    },
                    jvalue {
                        l: data.as_raw(),
                    },
                ],
            )
        }?
        .l()?;
        JFuture::from_env(env, future_obj)
    }
}

pub struct JBluetoothGattService<'a> {
    internal: JObject<'a>,
    get_uuid: JMethodID,
    get_characteristics: JMethodID,
}

impl<'a> JBluetoothGattService<'a> {
    pub fn from_env(env: &mut JNIEnv<'a>, obj: JObject<'a>) -> Result<Self> {
        let class = env.find_class("android/bluetooth/BluetoothGattService")?;

        let get_uuid = env.get_method_id(&class, "getUuid", "()Ljava/util/UUID;")?;
        let get_characteristics =
            env.get_method_id(&class, "getCharacteristics", "()Ljava/util/List;")?;
        Ok(Self {
            internal: obj,
            get_uuid,
            get_characteristics,
        })
    }

    pub fn is_primary(&self) -> Result<bool> {
        Ok(true)
    }

    pub fn get_uuid(&self, env: &mut JNIEnv<'a>) -> Result<Uuid> {
        let obj = unsafe {
            env.call_method_unchecked(&self.internal, self.get_uuid, ReturnType::Object, &[])
        }?
        .l()?;
        let uuid_obj = JUuid::from_env(env, obj)?;
        uuid_obj.as_uuid(env)
    }

    pub fn get_characteristics(
        &self,
        env: &mut JNIEnv<'a>,
    ) -> Result<Vec<JBluetoothGattCharacteristic<'a>>> {
        let obj = unsafe {
            env.call_method_unchecked(
                &self.internal,
                self.get_characteristics,
                ReturnType::Object,
                &[],
            )
        }?
        .l()?;
        let size = env.call_method(&obj, "size", "()I", &[])?.i()?;
        let mut chr_vec = Vec::with_capacity(size as usize);
        for i in 0..size {
            let chr = env
                .call_method(&obj, "get", "(I)Ljava/lang/Object;", &[jni::objects::JValue::from(i)])?
                .l()?;
            chr_vec.push(JBluetoothGattCharacteristic::from_env(env, chr)?);
        }
        Ok(chr_vec)
    }
}

pub struct JBluetoothGattCharacteristic<'a> {
    internal: JObject<'a>,
    get_uuid: JMethodID,
    get_properties: JMethodID,
    get_value: JMethodID,
    get_descriptors: JMethodID,
}

impl<'a> JBluetoothGattCharacteristic<'a> {
    pub fn from_env(env: &mut JNIEnv<'a>, obj: JObject<'a>) -> Result<Self> {
        let class = env.find_class("android/bluetooth/BluetoothGattCharacteristic")?;

        let get_uuid = env.get_method_id(&class, "getUuid", "()Ljava/util/UUID;")?;
        let get_properties = env.get_method_id(&class, "getProperties", "()I")?;
        let get_descriptors = env.get_method_id(&class, "getDescriptors", "()Ljava/util/List;")?;
        let get_value = env.get_method_id(&class, "getValue", "()[B")?;
        Ok(Self {
            internal: obj,
            get_uuid,
            get_properties,
            get_value,
            get_descriptors,
        })
    }

    pub fn get_uuid(&self, env: &mut JNIEnv<'a>) -> Result<Uuid> {
        let obj = unsafe {
            env.call_method_unchecked(&self.internal, self.get_uuid, ReturnType::Object, &[])
        }?
        .l()?;
        let uuid_obj = JUuid::from_env(env, obj)?;
        uuid_obj.as_uuid(env)
    }

    pub fn get_properties(&self, env: &mut JNIEnv<'a>) -> Result<CharPropFlags> {
        let flags = unsafe {
            env.call_method_unchecked(
                &self.internal,
                self.get_properties,
                ReturnType::Primitive(Primitive::Int),
                &[],
            )
        }?
        .i()?;
        Ok(CharPropFlags::from_bits_truncate(flags as u8))
    }

    pub fn get_value(&self, env: &mut JNIEnv<'a>) -> Result<Vec<u8>> {
        let value = unsafe {
            env.call_method_unchecked(&self.internal, self.get_value, ReturnType::Array, &[])
        }?
        .l()?;
        let value_arr = unsafe { jni::objects::JByteArray::from_raw(value.into_raw()) };
        crate::droidplug::jni_utils::arrays::byte_array_to_vec(env, &value_arr)
    }

    pub fn get_descriptors(
        &self,
        env: &mut JNIEnv<'a>,
    ) -> Result<Vec<JBluetoothGattDescriptor<'a>>> {
        let obj = unsafe {
            env.call_method_unchecked(
                &self.internal,
                self.get_descriptors,
                ReturnType::Object,
                &[],
            )
        }?
        .l()?;
        let size = env.call_method(&obj, "size", "()I", &[])?.i()?;
        let mut desc_vec = Vec::with_capacity(size as usize);
        for i in 0..size {
            let desc = env
                .call_method(&obj, "get", "(I)Ljava/lang/Object;", &[jni::objects::JValue::from(i)])?
                .l()?;
            desc_vec.push(JBluetoothGattDescriptor::from_env(env, desc)?);
        }
        Ok(desc_vec)
    }
}

pub struct JBluetoothGattDescriptor<'a> {
    internal: JObject<'a>,
    get_uuid: JMethodID,
}

impl<'a> JBluetoothGattDescriptor<'a> {
    pub fn from_env(env: &mut JNIEnv<'a>, obj: JObject<'a>) -> Result<Self> {
        let class = env.find_class("android/bluetooth/BluetoothGattDescriptor")?;

        let get_uuid = env.get_method_id(&class, "getUuid", "()Ljava/util/UUID;")?;
        Ok(Self {
            internal: obj,
            get_uuid,
        })
    }

    pub fn get_uuid(&self, env: &mut JNIEnv<'a>) -> Result<Uuid> {
        let obj = unsafe {
            env.call_method_unchecked(&self.internal, self.get_uuid, ReturnType::Object, &[])
        }?
        .l()?;
        let uuid_obj = JUuid::from_env(env, obj)?;
        uuid_obj.as_uuid(env)
    }
}

pub struct JBluetoothDevice<'a> {
    internal: JObject<'a>,
    get_address: JMethodID,
}

impl<'a> JBluetoothDevice<'a> {
    pub fn from_env(env: &mut JNIEnv<'a>, obj: JObject<'a>) -> Result<Self> {
        let class = env.find_class("android/bluetooth/BluetoothDevice")?;

        let get_address = env.get_method_id(&class, "getAddress", "()Ljava/lang/String;")?;
        Ok(Self {
            internal: obj,
            get_address,
        })
    }

    pub fn get_address(&self, env: &mut JNIEnv<'a>) -> Result<JString<'a>> {
        let obj = unsafe {
            env.call_method_unchecked(&self.internal, self.get_address, ReturnType::Object, &[])
        }?
        .l()?;
        Ok(obj.into())
    }
}

pub struct JScanFilter<'a> {
    internal: JObject<'a>,
}

impl<'a> JScanFilter<'a> {
    pub fn new(env: &mut JNIEnv<'a>, filter: ScanFilter) -> Result<Self> {
        let string_class = env.find_class("java/lang/String")?;
        let uuids = env.new_object_array(
            filter.services.len() as i32,
            &string_class,
            &JObject::null(),
        )?;
        for (idx, uuid) in filter.services.into_iter().enumerate() {
            let uuid_str = env.new_string(uuid.to_string())?;
            env.set_object_array_element(&uuids, idx as i32, &uuid_str)?;
        }
        let class_static = crate::droidplug::jni_utils::classcache::get_class(
            "com/nonpolynomial/btleplug/android/impl/ScanFilter",
        )
        .unwrap();
        let obj = env.new_object(
            <&JClass>::from(class_static.as_obj()),
            "([Ljava/lang/String;)V",
            &[(&uuids).into()],
        )?;
        Ok(Self { internal: obj })
    }
}

impl<'a> From<JScanFilter<'a>> for JObject<'a> {
    fn from(value: JScanFilter<'a>) -> Self {
        value.internal
    }
}

pub struct JScanResult<'a> {
    internal: JObject<'a>,
    get_device: JMethodID,
    get_scan_record: JMethodID,
    get_tx_power: JMethodID,
    get_rssi: JMethodID,
}

impl<'a> JScanResult<'a> {
    pub fn from_env(env: &mut JNIEnv<'a>, obj: JObject<'a>) -> Result<Self> {
        let class = env.find_class("android/bluetooth/le/ScanResult")?;

        let get_device =
            env.get_method_id(&class, "getDevice", "()Landroid/bluetooth/BluetoothDevice;")?;
        let get_scan_record = env.get_method_id(
            &class,
            "getScanRecord",
            "()Landroid/bluetooth/le/ScanRecord;",
        )?;
        let get_tx_power = env.get_method_id(&class, "getTxPower", "()I")?;
        let get_rssi = env.get_method_id(&class, "getRssi", "()I")?;
        Ok(Self {
            internal: obj,
            get_device,
            get_scan_record,
            get_tx_power,
            get_rssi,
        })
    }

    pub fn get_device(&self, env: &mut JNIEnv<'a>) -> Result<JBluetoothDevice<'a>> {
        let obj = unsafe {
            env.call_method_unchecked(&self.internal, self.get_device, ReturnType::Object, &[])
        }?
        .l()?;
        JBluetoothDevice::from_env(env, obj)
    }

    pub fn get_scan_record(&self, env: &mut JNIEnv<'a>) -> Result<JScanRecord<'a>> {
        let obj = unsafe {
            env.call_method_unchecked(
                &self.internal,
                self.get_scan_record,
                ReturnType::Object,
                &[],
            )
        }?
        .l()?;
        JScanRecord::from_env(env, obj)
    }

    pub fn get_tx_power(&self, env: &mut JNIEnv<'a>) -> Result<jint> {
        unsafe {
            env.call_method_unchecked(
                &self.internal,
                self.get_tx_power,
                ReturnType::Primitive(Primitive::Int),
                &[],
            )
        }?
        .i()
    }

    pub fn get_rssi(&self, env: &mut JNIEnv<'a>) -> Result<jint> {
        unsafe {
            env.call_method_unchecked(
                &self.internal,
                self.get_rssi,
                ReturnType::Primitive(Primitive::Int),
                &[],
            )
        }?
        .i()
    }

    pub fn to_peripheral_properties(
        &self,
        env: &mut JNIEnv<'a>,
    ) -> std::result::Result<(BDAddr, Option<PeripheralProperties>), crate::Error> {
        use std::str::FromStr;

        let device = self.get_device(env)?;
        let addr_jstr = device.get_address(env)?;
        let addr_str = env.get_string(&addr_jstr)?;
        let addr = BDAddr::from_str(
            addr_str
                .to_str()
                .map_err(|e| crate::Error::Other(e.into()))?,
        )?;

        let record = self.get_scan_record(env)?;
        let record_is_null = env.is_same_object(&*record, JObject::null())?;
        let properties = if record_is_null {
            None
        } else {
            let device_name_obj = record.get_device_name(env)?;
            let device_name = if env.is_same_object(&device_name_obj, JObject::null())? {
                None
            } else {
                let device_name_jstr: JString = device_name_obj.into();
                let device_name_str = env.get_string(&device_name_jstr)?;
                Some(
                    String::from_utf8_lossy(device_name_str.to_bytes())
                        .chars()
                        .filter(|&c| c != '\u{fffd}')
                        .collect(),
                )
            };

            let tx_power_level = self.get_tx_power(env)?;
            const TX_POWER_NOT_PRESENT: jint = 127;
            let tx_power_level = if tx_power_level == TX_POWER_NOT_PRESENT {
                None
            } else {
                Some(tx_power_level as i16)
            };

            let rssi = Some(self.get_rssi(env)? as i16);

            let manufacturer_specific_data_obj = record.get_manufacturer_specific_data(env)?;
            let mut manufacturer_data = HashMap::new();
            if !env.is_same_object(&*manufacturer_specific_data_obj, JObject::null())? {
                let size = manufacturer_specific_data_obj.size(env)?;
                for i in 0..size {
                    let key = manufacturer_specific_data_obj.key_at(env, i)?;
                    let value = manufacturer_specific_data_obj.value_at(env, i)?;
                    let value_arr =
                        unsafe { jni::objects::JByteArray::from_raw(value.into_raw()) };
                    let data =
                        crate::droidplug::jni_utils::arrays::byte_array_to_vec(env, &value_arr)?;
                    manufacturer_data.insert(key as u16, data);
                }
            }

            let service_data_obj = record.get_service_data(env)?;
            let mut service_data = HashMap::new();
            if !env.is_same_object(&service_data_obj, JObject::null())? {
                let entry_set = env
                    .call_method(&service_data_obj, "entrySet", "()Ljava/util/Set;", &[])?
                    .l()?;
                let iter_obj = env
                    .call_method(
                        &entry_set,
                        "iterator",
                        "()Ljava/util/Iterator;",
                        &[],
                    )?
                    .l()?;
                while env
                    .call_method(&iter_obj, "hasNext", "()Z", &[])?
                    .z()?
                {
                    let entry = env
                        .call_method(&iter_obj, "next", "()Ljava/lang/Object;", &[])?
                        .l()?;
                    let key = env
                        .call_method(&entry, "getKey", "()Ljava/lang/Object;", &[])?
                        .l()?;
                    let value = env
                        .call_method(&entry, "getValue", "()Ljava/lang/Object;", &[])?
                        .l()?;
                    let parcel_uuid = JParcelUuid::from_env(env, key)?;
                    let juuid = parcel_uuid.get_uuid(env)?;
                    let uuid = juuid.as_uuid(env)?;
                    let value_arr =
                        unsafe { jni::objects::JByteArray::from_raw(value.into_raw()) };
                    let data =
                        crate::droidplug::jni_utils::arrays::byte_array_to_vec(env, &value_arr)?;
                    service_data.insert(uuid, data);
                }
            }

            let services_obj = record.get_service_uuids(env)?;
            let mut services = Vec::new();
            if !env.is_same_object(&services_obj, JObject::null())? {
                let size = env
                    .call_method(&services_obj, "size", "()I", &[])?
                    .i()?;
                for i in 0..size {
                    let obj = env
                        .call_method(
                            &services_obj,
                            "get",
                            "(I)Ljava/lang/Object;",
                            &[jni::objects::JValue::from(i)],
                        )?
                        .l()?;
                    let parcel_uuid = JParcelUuid::from_env(env, obj)?;
                    let juuid = parcel_uuid.get_uuid(env)?;
                    let uuid = juuid.as_uuid(env)?;
                    services.push(uuid);
                }
            }

            Some(PeripheralProperties {
                address: addr,
                address_type: None,
                local_name: device_name.clone(),
                advertisement_name: device_name,
                tx_power_level,
                manufacturer_data,
                service_data,
                services,
                rssi,
                class: None,
            })
        };
        Ok((addr, properties))
    }
}

pub struct JScanRecord<'a> {
    internal: JObject<'a>,
    get_device_name: JMethodID,
    get_tx_power_level: JMethodID,
    get_manufacturer_specific_data: JMethodID,
    get_service_data: JMethodID,
    get_service_uuids: JMethodID,
}

impl<'a> From<JScanRecord<'a>> for JObject<'a> {
    fn from(scan_record: JScanRecord<'a>) -> Self {
        scan_record.internal
    }
}

impl<'a> ::std::ops::Deref for JScanRecord<'a> {
    type Target = JObject<'a>;

    fn deref(&self) -> &Self::Target {
        &self.internal
    }
}

impl<'a> JScanRecord<'a> {
    pub fn from_env(env: &mut JNIEnv<'a>, obj: JObject<'a>) -> Result<Self> {
        let class = env.find_class("android/bluetooth/le/ScanRecord")?;

        let get_device_name = env.get_method_id(&class, "getDeviceName", "()Ljava/lang/String;")?;
        let get_tx_power_level = env.get_method_id(&class, "getTxPowerLevel", "()I")?;
        let get_manufacturer_specific_data = env.get_method_id(
            &class,
            "getManufacturerSpecificData",
            "()Landroid/util/SparseArray;",
        )?;
        let get_service_data = env.get_method_id(&class, "getServiceData", "()Ljava/util/Map;")?;
        let get_service_uuids =
            env.get_method_id(&class, "getServiceUuids", "()Ljava/util/List;")?;
        Ok(Self {
            internal: obj,
            get_device_name,
            get_tx_power_level,
            get_manufacturer_specific_data,
            get_service_data,
            get_service_uuids,
        })
    }

    pub fn get_device_name(&self, env: &mut JNIEnv<'a>) -> Result<JObject<'a>> {
        unsafe {
            env.call_method_unchecked(
                &self.internal,
                self.get_device_name,
                ReturnType::Object,
                &[],
            )
        }?
        .l()
    }

    pub fn get_tx_power_level(&self, env: &mut JNIEnv<'a>) -> Result<jint> {
        unsafe {
            env.call_method_unchecked(
                &self.internal,
                self.get_tx_power_level,
                ReturnType::Primitive(Primitive::Int),
                &[],
            )
        }?
        .i()
    }

    pub fn get_manufacturer_specific_data(
        &self,
        env: &mut JNIEnv<'a>,
    ) -> Result<JSparseArray<'a>> {
        let obj = unsafe {
            env.call_method_unchecked(
                &self.internal,
                self.get_manufacturer_specific_data,
                ReturnType::Object,
                &[],
            )
        }?
        .l()?;
        JSparseArray::from_env(env, obj)
    }

    pub fn get_service_data(&self, env: &mut JNIEnv<'a>) -> Result<JObject<'a>> {
        unsafe {
            env.call_method_unchecked(
                &self.internal,
                self.get_service_data,
                ReturnType::Object,
                &[],
            )
        }?
        .l()
    }

    pub fn get_service_uuids(&self, env: &mut JNIEnv<'a>) -> Result<JObject<'a>> {
        unsafe {
            env.call_method_unchecked(
                &self.internal,
                self.get_service_uuids,
                ReturnType::Object,
                &[],
            )
        }?
        .l()
    }
}

pub struct JSparseArray<'a> {
    internal: JObject<'a>,
    size: JMethodID,
    key_at: JMethodID,
    value_at: JMethodID,
}

impl<'a> From<JSparseArray<'a>> for JObject<'a> {
    fn from(sparse_array: JSparseArray<'a>) -> Self {
        sparse_array.internal
    }
}

impl<'a> ::std::ops::Deref for JSparseArray<'a> {
    type Target = JObject<'a>;

    fn deref(&self) -> &Self::Target {
        &self.internal
    }
}

impl<'a> JSparseArray<'a> {
    pub fn from_env(env: &mut JNIEnv<'a>, obj: JObject<'a>) -> Result<Self> {
        let class = env.find_class("android/util/SparseArray")?;

        let size = env.get_method_id(&class, "size", "()I")?;
        let key_at = env.get_method_id(&class, "keyAt", "(I)I")?;
        let value_at = env.get_method_id(&class, "valueAt", "(I)Ljava/lang/Object;")?;
        Ok(Self {
            internal: obj,
            size,
            key_at,
            value_at,
        })
    }

    pub fn size(&self, env: &mut JNIEnv<'a>) -> Result<jint> {
        unsafe {
            env.call_method_unchecked(
                &self.internal,
                self.size,
                ReturnType::Primitive(Primitive::Int),
                &[],
            )
        }?
        .i()
    }

    pub fn key_at(&self, env: &mut JNIEnv<'a>, index: jint) -> Result<jint> {
        unsafe {
            env.call_method_unchecked(
                &self.internal,
                self.key_at,
                ReturnType::Primitive(Primitive::Int),
                &[jvalue { i: index }],
            )
        }?
        .i()
    }

    pub fn value_at(&self, env: &mut JNIEnv<'a>, index: jint) -> Result<JObject<'a>> {
        unsafe {
            env.call_method_unchecked(
                &self.internal,
                self.value_at,
                ReturnType::Object,
                &[jvalue { i: index }],
            )
        }?
        .l()
    }
}

pub struct JParcelUuid<'a> {
    internal: JObject<'a>,
    get_uuid: JMethodID,
}

impl<'a> JParcelUuid<'a> {
    pub fn from_env(env: &mut JNIEnv<'a>, obj: JObject<'a>) -> Result<Self> {
        let class = env.find_class("android/os/ParcelUuid")?;

        let get_uuid = env.get_method_id(&class, "getUuid", "()Ljava/util/UUID;")?;
        Ok(Self {
            internal: obj,
            get_uuid,
        })
    }

    pub fn get_uuid(&self, env: &mut JNIEnv<'a>) -> Result<JUuid<'a>> {
        let obj = unsafe {
            env.call_method_unchecked(&self.internal, self.get_uuid, ReturnType::Object, &[])
        }?
        .l()?;
        JUuid::from_env(env, obj)
    }
}
