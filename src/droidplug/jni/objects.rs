use crate::droidplug::jni_utils::{future::JFuture, stream::JStream, uuid::JUuid};
use jni::{
    Env,
    bind_java_type,
    errors::Result,
    jni_sig,
    objects::{JObject, JString},
    sys::jint,
};
use std::{collections::HashMap, iter::Iterator};
use uuid::Uuid;

use crate::api::{BDAddr, CharPropFlags, PeripheralProperties, ScanFilter};

bind_java_type! {
    pub JNotConnectedException => com.nonpolynomial.btleplug.android.impl.NotConnectedException,
}

bind_java_type! {
    pub JPermissionDeniedException => com.nonpolynomial.btleplug.android.impl.PermissionDeniedException,
}

bind_java_type! {
    pub JUnexpectedCallbackException => com.nonpolynomial.btleplug.android.impl.UnexpectedCallbackException,
}

bind_java_type! {
    pub JUnexpectedCharacteristicException => com.nonpolynomial.btleplug.android.impl.UnexpectedCharacteristicException,
}

bind_java_type! {
    pub JNoSuchCharacteristicException => com.nonpolynomial.btleplug.android.impl.NoSuchCharacteristicException,
}

bind_java_type! {
    pub JNoBluetoothAdapterException => com.nonpolynomial.btleplug.android.impl.NoBluetoothAdapterException,
}

bind_java_type! {
    pub JScanFilterClass => com.nonpolynomial.btleplug.android.impl.ScanFilter,
}

bind_java_type! {
    pub JPeripheral => com.nonpolynomial.btleplug.android.impl.Peripheral,
    constructors {
        fn with_adapter(adapter: JObject, address: JString),
    },
    methods {
        priv fn connect_raw() -> JObject { name = "connect" },
        priv fn disconnect_raw() -> JObject { name = "disconnect" },
        fn is_connected() -> jboolean,
        priv fn discover_services_raw() -> JObject { name = "discoverServices" },
        priv fn read_raw(uuid: JObject) -> JObject { name = "read" },
        priv fn write_raw(uuid: JObject, data: JObject, write_type: jint) -> JObject { name = "write" },
        priv fn set_characteristic_notification_raw(uuid: JObject, enable: jboolean) -> JObject {
            name = "setCharacteristicNotification",
        },
        priv fn get_notifications_raw() -> JObject { name = "getNotifications" },
        priv fn read_descriptor_raw(characteristic: JObject, uuid: JObject) -> JObject { name = "readDescriptor" },
        priv fn write_descriptor_raw(characteristic: JObject, uuid: JObject, data: JObject) -> JObject {
            name = "writeDescriptor",
        },
        priv fn get_device_name_raw() -> JObject { name = "getDeviceName" },
        fn request_mtu(mtu: jint) -> JObject,
        priv fn get_connection_parameters_raw() -> JObject { name = "getConnectionParameters" },
        fn request_connection_priority(priority: jint) -> jboolean,
        fn read_remote_rssi() -> JObject,
    },
}

impl JPeripheral<'_> {
    pub fn create<'local>(env: &mut Env<'local>, adapter: JObject<'local>, addr: BDAddr) -> Result<JPeripheral<'local>> {
        let addr_jstr = env.new_string(format!("{:X}", addr))?;
        JPeripheral::with_adapter(env, &adapter, &addr_jstr)
    }
}

impl<'local> JPeripheral<'local> {
    pub fn connect(&self, env: &mut Env<'local>) -> Result<JFuture<'local>> {
        env.cast_local::<JFuture>(self.connect_raw(env)?)
    }

    pub fn disconnect(&self, env: &mut Env<'local>) -> Result<JFuture<'local>> {
        env.cast_local::<JFuture>(self.disconnect_raw(env)?)
    }

    pub fn discover_services(&self, env: &mut Env<'local>) -> Result<JFuture<'local>> {
        env.cast_local::<JFuture>(self.discover_services_raw(env)?)
    }

    pub fn read(&self, env: &mut Env<'local>, uuid: &JUuid<'local>) -> Result<JFuture<'local>> {
        env.cast_local::<JFuture>(self.read_raw(env, uuid)?)
    }

    pub fn write(
        &self,
        env: &mut Env<'local>,
        uuid: &JUuid<'local>,
        data: &JObject<'local>,
        write_type: jint,
    ) -> Result<JFuture<'local>> {
        env.cast_local::<JFuture>(self.write_raw(env, uuid, data, write_type)?)
    }

    pub fn set_characteristic_notification(
        &self,
        env: &mut Env<'local>,
        uuid: &JUuid<'local>,
        enable: bool,
    ) -> Result<JFuture<'local>> {
        env.cast_local::<JFuture>(self.set_characteristic_notification_raw(env, uuid, enable)?)
    }

    pub fn get_notifications(&self, env: &mut Env<'local>) -> Result<JStream<'local>> {
        env.cast_local::<JStream>(self.get_notifications_raw(env)?)
    }

    pub fn read_descriptor(
        &self,
        env: &mut Env<'local>,
        characteristic: &JUuid<'local>,
        uuid: &JUuid<'local>,
    ) -> Result<JFuture<'local>> {
        env.cast_local::<JFuture>(self.read_descriptor_raw(env, characteristic, uuid)?)
    }

    pub fn write_descriptor(
        &self,
        env: &mut Env<'local>,
        characteristic: &JUuid<'local>,
        uuid: &JUuid<'local>,
        data: &JObject<'local>,
    ) -> Result<JFuture<'local>> {
        env.cast_local::<JFuture>(self.write_descriptor_raw(env, characteristic, uuid, data)?)
    }

    pub fn get_device_name(&self, env: &mut Env<'local>) -> Result<Option<String>> {
        let obj = self.get_device_name_raw(env)?;
        if obj.is_null() {
            Ok(None)
        } else {
            let jstr = env.cast_local::<JString>(obj)?;
            let name_str = jstr.mutf8_chars(env)?;
            Ok(Some(String::from(name_str)))
        }
    }

    pub fn get_connection_parameters(
        &self,
        env: &mut Env<'local>,
    ) -> Result<Option<crate::api::ConnectionParameters>> {
        let obj = self.get_connection_parameters_raw(env)?;
        if obj.is_null() {
            return Ok(None);
        }
        let arr = unsafe { jni::objects::JIntArray::from_raw(env, obj.into_raw()) };
        let len = arr.len(env)?;
        if len < 3 {
            return Ok(None);
        }
        let mut buf = [0i32; 3];
        arr.get_region(env, 0, &mut buf)?;
        Ok(Some(crate::api::ConnectionParameters {
            interval_us: (buf[0] as u32) * 1250,
            latency: buf[1] as u16,
            supervision_timeout_us: (buf[2] as u32) * 10_000,
        }))
    }
}

bind_java_type! {
    pub JBluetoothGattService => android.bluetooth.BluetoothGattService,
    methods {
        fn get_uuid_obj() -> JObject {
            name = "getUuid",
        },
        fn get_characteristics_obj() -> JObject {
            name = "getCharacteristics",
        },
    },
}

impl<'local> JBluetoothGattService<'local> {
    pub fn is_primary(&self) -> Result<bool> {
        Ok(true)
    }

    pub fn get_uuid(&self, env: &mut Env<'local>) -> Result<Uuid> {
        let obj = self.get_uuid_obj(env)?;
        let uuid_obj = env.cast_local::<JUuid>(obj)?;
        uuid_obj.as_uuid(env)
    }

    pub fn get_characteristics(
        &self,
        env: &mut Env<'local>,
    ) -> Result<Vec<JBluetoothGattCharacteristic<'local>>> {
        let obj = self.get_characteristics_obj(env)?;
        let size = env.call_method(&obj, jni_str!("size"), jni_sig!("()I"), &[])?.i()?;
        let mut chr_vec = Vec::with_capacity(size as usize);
        for i in 0..size {
            let chr = env
                .call_method(&obj, jni_str!("get"), jni_sig!("(I)Ljava/lang/Object;"), &[jni::objects::JValue::from(i)])?
                .l()?;
            chr_vec.push(env.cast_local::<JBluetoothGattCharacteristic>(chr)?);
        }
        Ok(chr_vec)
    }
}

bind_java_type! {
    pub JBluetoothGattCharacteristic => android.bluetooth.BluetoothGattCharacteristic,
    methods {
        fn get_uuid_obj() -> JObject {
            name = "getUuid",
        },
        fn get_properties_raw() -> jint {
            name = "getProperties",
        },
        fn get_value_obj() -> JObject {
            name = "getValue",
        },
        fn get_descriptors_obj() -> JObject {
            name = "getDescriptors",
        },
    },
}

impl<'local> JBluetoothGattCharacteristic<'local> {
    pub fn get_uuid(&self, env: &mut Env<'local>) -> Result<Uuid> {
        let obj = self.get_uuid_obj(env)?;
        let uuid_obj = env.cast_local::<JUuid>(obj)?;
        uuid_obj.as_uuid(env)
    }

    pub fn get_properties(&self, env: &mut Env<'local>) -> Result<CharPropFlags> {
        let flags = self.get_properties_raw(env)?;
        Ok(CharPropFlags::from_bits_truncate(flags as u8))
    }

    pub fn get_value(&self, env: &mut Env<'local>) -> Result<Vec<u8>> {
        let value = self.get_value_obj(env)?;
        let value_arr = unsafe { jni::objects::JByteArray::from_raw(env, value.into_raw()) };
        crate::droidplug::jni_utils::arrays::byte_array_to_vec(env, &value_arr)
    }

    pub fn get_descriptors(
        &self,
        env: &mut Env<'local>,
    ) -> Result<Vec<JBluetoothGattDescriptor<'local>>> {
        let obj = self.get_descriptors_obj(env)?;
        let size = env.call_method(&obj, jni_str!("size"), jni_sig!("()I"), &[])?.i()?;
        let mut desc_vec = Vec::with_capacity(size as usize);
        for i in 0..size {
            let desc = env
                .call_method(&obj, jni_str!("get"), jni_sig!("(I)Ljava/lang/Object;"), &[jni::objects::JValue::from(i)])?
                .l()?;
            desc_vec.push(env.cast_local::<JBluetoothGattDescriptor>(desc)?);
        }
        Ok(desc_vec)
    }
}

bind_java_type! {
    pub JBluetoothGattDescriptor => android.bluetooth.BluetoothGattDescriptor,
    methods {
        fn get_uuid_obj() -> JObject {
            name = "getUuid",
        },
    },
}

impl<'local> JBluetoothGattDescriptor<'local> {
    pub fn get_uuid(&self, env: &mut Env<'local>) -> Result<Uuid> {
        let obj = self.get_uuid_obj(env)?;
        let uuid_obj = env.cast_local::<JUuid>(obj)?;
        uuid_obj.as_uuid(env)
    }
}

bind_java_type! {
    pub JBluetoothDevice => android.bluetooth.BluetoothDevice,
    methods {
        fn get_address() -> JString,
    },
}

pub struct JScanFilter<'a> {
    internal: JObject<'a>,
}

impl<'a> JScanFilter<'a> {
    pub fn new(env: &mut Env<'a>, filter: ScanFilter) -> Result<Self> {
        let uuids = jni::objects::JObjectArray::<JString>::new(
            env,
            filter.services.len(),
            &JString::default(),
        )?;
        for (idx, uuid) in filter.services.into_iter().enumerate() {
            let uuid_str = env.new_string(uuid.to_string())?;
            uuids.set_element(env, idx, &uuid_str)?;
        }
        let class = <JScanFilterClass as jni::objects::Reference>::lookup_class(
            env,
            &Default::default(),
        )?;
        let obj = env.new_object(
            &*class,
            jni_sig!("([Ljava/lang/String;)V"),
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

bind_java_type! {
    pub JScanResult => android.bluetooth.le.ScanResult,
    methods {
        fn get_device_obj() -> JObject {
            name = "getDevice",
        },
        fn get_scan_record_obj() -> JObject {
            name = "getScanRecord",
        },
        fn get_tx_power() -> jint,
        fn get_rssi() -> jint,
    },
}

impl<'local> JScanResult<'local> {
    pub fn get_device(&self, env: &mut Env<'local>) -> Result<JBluetoothDevice<'local>> {
        let obj = self.get_device_obj(env)?;
        env.cast_local::<JBluetoothDevice>(obj)
    }

    pub fn get_scan_record(&self, env: &mut Env<'local>) -> Result<JObject<'local>> {
        self.get_scan_record_obj(env)
    }

    pub fn to_peripheral_properties(
        &self,
        env: &mut Env<'local>,
    ) -> std::result::Result<(BDAddr, Option<PeripheralProperties>), crate::Error> {
        use std::str::FromStr;

        let device = self.get_device(env)?;
        let addr_jstr = device.get_address(env)?;
        let addr_str = String::from(addr_jstr.mutf8_chars(env)?);
        let addr = BDAddr::from_str(&addr_str)?;

        let record_obj = self.get_scan_record(env)?;
        let properties = if record_obj.is_null() {
            None
        } else {
            let record = env.cast_local::<JScanRecord>(record_obj)?;
            let device_name_obj = record.get_device_name(env)?;
            let device_name = if env.is_same_object(&device_name_obj, JObject::null())? {
                None
            } else {
                let device_name_jstr = env.cast_local::<JString>(device_name_obj)?;
                let device_name_str = String::from(device_name_jstr.mutf8_chars(env)?);
                Some(
                    device_name_str
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

            let mfr_data_obj = record.get_manufacturer_specific_data(env)?;
            let mut manufacturer_data = HashMap::new();
            if !mfr_data_obj.is_null() {
                let sparse_arr = env.cast_local::<JSparseArray>(mfr_data_obj)?;
                let size = sparse_arr.size(env)?;
                for i in 0..size {
                    let key = sparse_arr.key_at(env, i)?;
                    let value = sparse_arr.value_at(env, i)?;
                    let value_arr =
                        unsafe { jni::objects::JByteArray::from_raw(env, value.into_raw()) };
                    let data =
                        crate::droidplug::jni_utils::arrays::byte_array_to_vec(env, &value_arr)?;
                    manufacturer_data.insert(key as u16, data);
                }
            }

            let service_data_obj = record.get_service_data(env)?;
            let mut service_data = HashMap::new();
            if !env.is_same_object(&service_data_obj, JObject::null())? {
                let entry_set = env
                    .call_method(&service_data_obj, jni_str!("entrySet"), jni_sig!("()Ljava/util/Set;"), &[])?
                    .l()?;
                let iter_obj = env
                    .call_method(
                        &entry_set,
                        jni_str!("iterator"),
                        jni_sig!("()Ljava/util/Iterator;"),
                        &[],
                    )?
                    .l()?;
                while env
                    .call_method(&iter_obj, jni_str!("hasNext"), jni_sig!("()Z"), &[])?
                    .z()?
                {
                    let entry = env
                        .call_method(&iter_obj, jni_str!("next"), jni_sig!("()Ljava/lang/Object;"), &[])?
                        .l()?;
                    let key = env
                        .call_method(&entry, jni_str!("getKey"), jni_sig!("()Ljava/lang/Object;"), &[])?
                        .l()?;
                    let value = env
                        .call_method(&entry, jni_str!("getValue"), jni_sig!("()Ljava/lang/Object;"), &[])?
                        .l()?;
                    let parcel_uuid = env.cast_local::<JParcelUuid>(key)?;
                    let juuid = parcel_uuid.get_uuid(env)?;
                    let uuid = juuid.as_uuid(env)?;
                    let value_arr =
                        unsafe { jni::objects::JByteArray::from_raw(env, value.into_raw()) };
                    let data =
                        crate::droidplug::jni_utils::arrays::byte_array_to_vec(env, &value_arr)?;
                    service_data.insert(uuid, data);
                }
            }

            let services_obj = record.get_service_uuids(env)?;
            let mut services = Vec::new();
            if !env.is_same_object(&services_obj, JObject::null())? {
                let size = env
                    .call_method(&services_obj, jni_str!("size"), jni_sig!("()I"), &[])?
                    .i()?;
                for i in 0..size {
                    let obj = env
                        .call_method(
                            &services_obj,
                            jni_str!("get"),
                            jni_sig!("(I)Ljava/lang/Object;"),
                            &[jni::objects::JValue::from(i)],
                        )?
                        .l()?;
                    let parcel_uuid = env.cast_local::<JParcelUuid>(obj)?;
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

bind_java_type! {
    pub JScanRecord => android.bluetooth.le.ScanRecord,
    methods {
        fn get_device_name() -> JObject,
        fn get_tx_power_level() -> jint,
        fn get_manufacturer_specific_data() -> JObject,
        fn get_service_data() -> JObject,
        fn get_service_uuids() -> JObject,
    },
}

bind_java_type! {
    pub JSparseArray => android.util.SparseArray,
    methods {
        fn size() -> jint,
        fn key_at(index: jint) -> jint,
        fn value_at(index: jint) -> JObject,
    },
}

bind_java_type! {
    pub JParcelUuid => android.os.ParcelUuid,
    methods {
        fn get_uuid_obj() -> JObject {
            name = "getUuid",
        },
    },
}

impl<'local> JParcelUuid<'local> {
    pub fn get_uuid(&self, env: &mut Env<'local>) -> Result<JUuid<'local>> {
        let obj = self.get_uuid_obj(env)?;
        env.cast_local::<JUuid>(obj)
    }
}
