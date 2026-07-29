use crate::droidplug::jni_utils::{future::JFuture, stream::JStream, uuid::JUuid};
use jni::{
    Env, bind_java_type,
    errors::Result,
    jni_sig, jni_str,
    objects::{JObject, JString, Reference},
    sys::jint,
};
use std::{collections::HashMap, iter::Iterator};
use uuid::Uuid;

use crate::api::{BDAddr, CharPropFlags, PeripheralProperties, ScanFilter};

bind_java_type! {
    pub JNotConnectedException => "com.nonpolynomial.btleplug.android.impl.NotConnectedException",
}

bind_java_type! {
    pub JPermissionDeniedException => "com.nonpolynomial.btleplug.android.impl.PermissionDeniedException",
}

bind_java_type! {
    pub JUnexpectedCallbackException => "com.nonpolynomial.btleplug.android.impl.UnexpectedCallbackException",
}

bind_java_type! {
    pub JUnexpectedCharacteristicException => "com.nonpolynomial.btleplug.android.impl.UnexpectedCharacteristicException",
}

bind_java_type! {
    pub JNoSuchCharacteristicException => "com.nonpolynomial.btleplug.android.impl.NoSuchCharacteristicException",
}

bind_java_type! {
    pub JNoBluetoothAdapterException => "com.nonpolynomial.btleplug.android.impl.NoBluetoothAdapterException",
}

bind_java_type! {
    pub JScanFilterClass => "com.nonpolynomial.btleplug.android.impl.ScanFilter",
}

// JPeripheral: bind_java_type! for class definition only. Methods use domain-specific
// Java types (UUID, Future, Stream, byte[]) whose JNI signatures can't be expressed
// through the macro's Rust-to-JNI type mapping (JObject → Ljava/lang/Object; is wrong).
bind_java_type! {
    pub JPeripheral => "com.nonpolynomial.btleplug.android.impl.Peripheral",
    methods {
        fn is_connected() -> jboolean,
        fn request_connection_priority(priority: jint) -> jboolean,
    },
}

impl JPeripheral<'_> {
    pub fn create<'local>(
        env: &mut Env<'local>,
        adapter: JObject<'local>,
        addr: BDAddr,
    ) -> Result<JPeripheral<'local>> {
        let addr_jstr = env.new_string(format!("{:X}", addr))?;
        let class = JPeripheral::lookup_class(env, &Default::default())?;
        let obj = env.new_object(
            &*class,
            jni_sig!("(Lcom/nonpolynomial/btleplug/android/impl/Adapter;Ljava/lang/String;)V"),
            &[(&adapter).into(), (&addr_jstr).into()],
        )?;
        env.cast_local::<JPeripheral>(obj)
    }
}

impl<'local> JPeripheral<'local> {
    pub fn connect(&self, env: &mut Env<'local>) -> Result<JFuture<'local>> {
        let raw = env
            .call_method(
                self,
                jni_str!("connect"),
                jni_sig!("()Lio/github/gedgygedgy/rust/future/Future;"),
                &[],
            )?
            .l()?;
        env.cast_local::<JFuture>(raw)
    }

    pub fn disconnect(&self, env: &mut Env<'local>) -> Result<JFuture<'local>> {
        let raw = env
            .call_method(
                self,
                jni_str!("disconnect"),
                jni_sig!("()Lio/github/gedgygedgy/rust/future/Future;"),
                &[],
            )?
            .l()?;
        env.cast_local::<JFuture>(raw)
    }

    pub fn discover_services(&self, env: &mut Env<'local>) -> Result<JFuture<'local>> {
        let raw = env
            .call_method(
                self,
                jni_str!("discoverServices"),
                jni_sig!("()Lio/github/gedgygedgy/rust/future/Future;"),
                &[],
            )?
            .l()?;
        env.cast_local::<JFuture>(raw)
    }

    pub fn read(&self, env: &mut Env<'local>, uuid: &JUuid<'local>) -> Result<JFuture<'local>> {
        let raw = env
            .call_method(
                self,
                jni_str!("read"),
                jni_sig!("(Ljava/util/UUID;)Lio/github/gedgygedgy/rust/future/Future;"),
                &[uuid.into()],
            )?
            .l()?;
        env.cast_local::<JFuture>(raw)
    }

    pub fn write(
        &self,
        env: &mut Env<'local>,
        uuid: &JUuid<'local>,
        data: &JObject<'local>,
        write_type: jint,
    ) -> Result<JFuture<'local>> {
        let raw = env
            .call_method(
                self,
                jni_str!("write"),
                jni_sig!("(Ljava/util/UUID;[BI)Lio/github/gedgygedgy/rust/future/Future;"),
                &[uuid.into(), data.into(), write_type.into()],
            )?
            .l()?;
        env.cast_local::<JFuture>(raw)
    }

    pub fn set_characteristic_notification(
        &self,
        env: &mut Env<'local>,
        uuid: &JUuid<'local>,
        enable: bool,
    ) -> Result<JFuture<'local>> {
        let raw = env
            .call_method(
                self,
                jni_str!("setCharacteristicNotification"),
                jni_sig!("(Ljava/util/UUID;Z)Lio/github/gedgygedgy/rust/future/Future;"),
                &[uuid.into(), enable.into()],
            )?
            .l()?;
        env.cast_local::<JFuture>(raw)
    }

    pub fn get_notifications(&self, env: &mut Env<'local>) -> Result<JStream<'local>> {
        let raw = env
            .call_method(
                self,
                jni_str!("getNotifications"),
                jni_sig!("()Lio/github/gedgygedgy/rust/stream/Stream;"),
                &[],
            )?
            .l()?;
        env.cast_local::<JStream>(raw)
    }

    pub fn read_descriptor(
        &self,
        env: &mut Env<'local>,
        characteristic: &JUuid<'local>,
        uuid: &JUuid<'local>,
    ) -> Result<JFuture<'local>> {
        let raw = env
            .call_method(
                self,
                jni_str!("readDescriptor"),
                jni_sig!(
                    "(Ljava/util/UUID;Ljava/util/UUID;)Lio/github/gedgygedgy/rust/future/Future;"
                ),
                &[characteristic.into(), uuid.into()],
            )?
            .l()?;
        env.cast_local::<JFuture>(raw)
    }

    pub fn write_descriptor(
        &self,
        env: &mut Env<'local>,
        characteristic: &JUuid<'local>,
        uuid: &JUuid<'local>,
        data: &JObject<'local>,
    ) -> Result<JFuture<'local>> {
        let raw = env
            .call_method(
                self,
                jni_str!("writeDescriptor"),
                jni_sig!(
                    "(Ljava/util/UUID;Ljava/util/UUID;[B)Lio/github/gedgygedgy/rust/future/Future;"
                ),
                &[characteristic.into(), uuid.into(), data.into()],
            )?
            .l()?;
        env.cast_local::<JFuture>(raw)
    }

    pub fn get_device_name(&self, env: &mut Env<'local>) -> Result<Option<String>> {
        let obj = env
            .call_method(
                self,
                jni_str!("getDeviceName"),
                jni_sig!("()Ljava/lang/String;"),
                &[],
            )?
            .l()?;
        if obj.is_null() {
            Ok(None)
        } else {
            let jstr = env.cast_local::<JString>(obj)?;
            let name_str = jstr.mutf8_chars(env)?;
            Ok(Some(String::from(name_str)))
        }
    }

    pub fn request_mtu(&self, env: &mut Env<'local>, mtu: jint) -> Result<JFuture<'local>> {
        let raw = env
            .call_method(
                self,
                jni_str!("requestMtu"),
                jni_sig!("(I)Lio/github/gedgygedgy/rust/future/Future;"),
                &[mtu.into()],
            )?
            .l()?;
        env.cast_local::<JFuture>(raw)
    }

    pub fn get_connection_parameters(
        &self,
        env: &mut Env<'local>,
    ) -> Result<Option<crate::api::ConnectionParameters>> {
        let obj = env
            .call_method(
                self,
                jni_str!("getConnectionParameters"),
                jni_sig!("()[I"),
                &[],
            )?
            .l()?;
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

    pub fn read_remote_rssi(&self, env: &mut Env<'local>) -> Result<JFuture<'local>> {
        let raw = env
            .call_method(
                self,
                jni_str!("readRemoteRssi"),
                jni_sig!("()Lio/github/gedgygedgy/rust/future/Future;"),
                &[],
            )?
            .l()?;
        env.cast_local::<JFuture>(raw)
    }
}

// Android SDK types: class definition only, methods use manual JNI signatures
// because return types (UUID, List, byte[]) don't map to JObject.

bind_java_type! {
    pub JBluetoothGattService => android.bluetooth.BluetoothGattService,
}

impl<'local> JBluetoothGattService<'local> {
    pub fn is_primary(&self) -> Result<bool> {
        Ok(true)
    }

    pub fn get_uuid(&self, env: &mut Env<'local>) -> Result<Uuid> {
        let obj = env
            .call_method(
                self,
                jni_str!("getUuid"),
                jni_sig!("()Ljava/util/UUID;"),
                &[],
            )?
            .l()?;
        let uuid_obj = env.cast_local::<JUuid>(obj)?;
        uuid_obj.as_uuid(env)
    }

    pub fn get_characteristics(
        &self,
        env: &mut Env<'local>,
    ) -> Result<Vec<JBluetoothGattCharacteristic<'local>>> {
        let obj = env
            .call_method(
                self,
                jni_str!("getCharacteristics"),
                jni_sig!("()Ljava/util/List;"),
                &[],
            )?
            .l()?;
        let size = env
            .call_method(&obj, jni_str!("size"), jni_sig!("()I"), &[])?
            .i()?;
        let mut chr_vec = Vec::with_capacity(size as usize);
        for i in 0..size {
            let chr = env
                .call_method(
                    &obj,
                    jni_str!("get"),
                    jni_sig!("(I)Ljava/lang/Object;"),
                    &[jni::objects::JValue::from(i)],
                )?
                .l()?;
            chr_vec.push(env.cast_local::<JBluetoothGattCharacteristic>(chr)?);
        }
        Ok(chr_vec)
    }
}

bind_java_type! {
    pub JBluetoothGattCharacteristic => android.bluetooth.BluetoothGattCharacteristic,
    methods {
        fn get_properties_raw { name = "getProperties", sig = () -> jint },
    },
}

impl<'local> JBluetoothGattCharacteristic<'local> {
    pub fn get_uuid(&self, env: &mut Env<'local>) -> Result<Uuid> {
        let obj = env
            .call_method(
                self,
                jni_str!("getUuid"),
                jni_sig!("()Ljava/util/UUID;"),
                &[],
            )?
            .l()?;
        let uuid_obj = env.cast_local::<JUuid>(obj)?;
        uuid_obj.as_uuid(env)
    }

    pub fn get_properties(&self, env: &mut Env<'local>) -> Result<CharPropFlags> {
        let flags = self.get_properties_raw(env)?;
        Ok(CharPropFlags::from_bits_truncate(flags as u8))
    }

    pub fn get_value(&self, env: &mut Env<'local>) -> Result<Vec<u8>> {
        let value = env
            .call_method(self, jni_str!("getValue"), jni_sig!("()[B"), &[])?
            .l()?;
        let value_arr = unsafe { jni::objects::JByteArray::from_raw(env, value.into_raw()) };
        crate::droidplug::jni_utils::arrays::byte_array_to_vec(env, &value_arr)
    }

    pub fn get_descriptors(
        &self,
        env: &mut Env<'local>,
    ) -> Result<Vec<JBluetoothGattDescriptor<'local>>> {
        let obj = env
            .call_method(
                self,
                jni_str!("getDescriptors"),
                jni_sig!("()Ljava/util/List;"),
                &[],
            )?
            .l()?;
        let size = env
            .call_method(&obj, jni_str!("size"), jni_sig!("()I"), &[])?
            .i()?;
        let mut desc_vec = Vec::with_capacity(size as usize);
        for i in 0..size {
            let desc = env
                .call_method(
                    &obj,
                    jni_str!("get"),
                    jni_sig!("(I)Ljava/lang/Object;"),
                    &[jni::objects::JValue::from(i)],
                )?
                .l()?;
            desc_vec.push(env.cast_local::<JBluetoothGattDescriptor>(desc)?);
        }
        Ok(desc_vec)
    }
}

bind_java_type! {
    pub JBluetoothGattDescriptor => android.bluetooth.BluetoothGattDescriptor,
}

impl<'local> JBluetoothGattDescriptor<'local> {
    pub fn get_uuid(&self, env: &mut Env<'local>) -> Result<Uuid> {
        let obj = env
            .call_method(
                self,
                jni_str!("getUuid"),
                jni_sig!("()Ljava/util/UUID;"),
                &[],
            )?
            .l()?;
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
        let class = <JScanFilterClass as Reference>::lookup_class(env, &Default::default())?;
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
        fn get_tx_power() -> jint,
        fn get_rssi() -> jint,
    },
}

impl<'local> JScanResult<'local> {
    pub fn get_device(&self, env: &mut Env<'local>) -> Result<JBluetoothDevice<'local>> {
        let obj = env
            .call_method(
                self,
                jni_str!("getDevice"),
                jni_sig!("()Landroid/bluetooth/BluetoothDevice;"),
                &[],
            )?
            .l()?;
        env.cast_local::<JBluetoothDevice>(obj)
    }

    pub fn get_scan_record(&self, env: &mut Env<'local>) -> Result<JObject<'local>> {
        env.call_method(
            self,
            jni_str!("getScanRecord"),
            jni_sig!("()Landroid/bluetooth/le/ScanRecord;"),
            &[],
        )?
        .l()
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
            let appearance = record
                .get_bytes(env)?
                .as_deref()
                .and_then(crate::advertisement::parse_appearance_from_advertisement);

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
                    .call_method(
                        &service_data_obj,
                        jni_str!("entrySet"),
                        jni_sig!("()Ljava/util/Set;"),
                        &[],
                    )?
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
                        .call_method(
                            &iter_obj,
                            jni_str!("next"),
                            jni_sig!("()Ljava/lang/Object;"),
                            &[],
                        )?
                        .l()?;
                    let key = env
                        .call_method(
                            &entry,
                            jni_str!("getKey"),
                            jni_sig!("()Ljava/lang/Object;"),
                            &[],
                        )?
                        .l()?;
                    let value = env
                        .call_method(
                            &entry,
                            jni_str!("getValue"),
                            jni_sig!("()Ljava/lang/Object;"),
                            &[],
                        )?
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
                appearance,
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
        fn get_tx_power_level() -> jint,
    },
}

impl<'local> JScanRecord<'local> {
    pub fn get_bytes(&self, env: &mut Env<'local>) -> Result<Option<Vec<u8>>> {
        let value = env
            .call_method(self, jni_str!("getBytes"), jni_sig!("()[B"), &[])?
            .l()?;
        if value.is_null() {
            Ok(None)
        } else {
            let value = unsafe { jni::objects::JByteArray::from_raw(env, value.into_raw()) };
            crate::droidplug::jni_utils::arrays::byte_array_to_vec(env, &value).map(Some)
        }
    }

    pub fn get_device_name(&self, env: &mut Env<'local>) -> Result<JObject<'local>> {
        env.call_method(
            self,
            jni_str!("getDeviceName"),
            jni_sig!("()Ljava/lang/String;"),
            &[],
        )?
        .l()
    }

    pub fn get_manufacturer_specific_data(&self, env: &mut Env<'local>) -> Result<JObject<'local>> {
        env.call_method(
            self,
            jni_str!("getManufacturerSpecificData"),
            jni_sig!("()Landroid/util/SparseArray;"),
            &[],
        )?
        .l()
    }

    pub fn get_service_data(&self, env: &mut Env<'local>) -> Result<JObject<'local>> {
        env.call_method(
            self,
            jni_str!("getServiceData"),
            jni_sig!("()Ljava/util/Map;"),
            &[],
        )?
        .l()
    }

    pub fn get_service_uuids(&self, env: &mut Env<'local>) -> Result<JObject<'local>> {
        env.call_method(
            self,
            jni_str!("getServiceUuids"),
            jni_sig!("()Ljava/util/List;"),
            &[],
        )?
        .l()
    }
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
}

impl<'local> JParcelUuid<'local> {
    pub fn get_uuid(&self, env: &mut Env<'local>) -> Result<JUuid<'local>> {
        let obj = env
            .call_method(
                self,
                jni_str!("getUuid"),
                jni_sig!("()Ljava/util/UUID;"),
                &[],
            )?
            .l()?;
        env.cast_local::<JUuid>(obj)
    }
}
