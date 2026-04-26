use crate::droidplug::jni_utils::{future::JFuture, stream::JStream, uuid::JUuid};
use jni::{
    JNIEnv,
    errors::Result,
    objects::{JClass, JList, JMap, JMethodID, JObject, JString, JValue},
    signature::{Primitive, ReturnType},
    strings::JavaStr,
    sys::jint,
};
use std::{collections::HashMap, convert::TryFrom, iter::Iterator};
use uuid::Uuid;

use crate::api::{BDAddr, CharPropFlags, PeripheralProperties, ScanFilter};

pub struct JPeripheral<'a: 'b, 'b> {
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
    env: &'b JNIEnv<'a>,
}

impl<'a: 'b, 'b> ::std::ops::Deref for JPeripheral<'a, 'b> {
    type Target = JObject<'a>;

    fn deref(&self) -> &Self::Target {
        &self.internal
    }
}

impl<'a: 'b, 'b> From<JPeripheral<'a, 'b>> for JObject<'a> {
    fn from(other: JPeripheral<'a, 'b>) -> JObject<'a> {
        other.internal
    }
}

impl<'a: 'b, 'b> JPeripheral<'a, 'b> {
    pub fn from_env(env: &'b JNIEnv<'a>, obj: JObject<'a>) -> Result<Self> {
        //Self::from_env_impl(env, obj)
        //let class = env.find_class("com/nonpolynomial/btleplug/android/impl/Peripheral")?;
        //Self::from_env_impl(env, obj, class)
        Self::from_env_impl(env, obj)
    }

    fn from_env_impl(env: &'b JNIEnv<'a>, obj: JObject<'a>) -> Result<Self> {
        //let class = env.auto_local(class);
        let class_static = crate::droidplug::jni_utils::classcache::get_class(
            "com/nonpolynomial/btleplug/android/impl/Peripheral",
        )
        .unwrap();
        let class = JClass::from(class_static.as_obj());

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
            env,
        })
    }

    pub fn new(env: &'b JNIEnv<'a>, adapter: JObject<'a>, addr: BDAddr) -> Result<Self> {
        // let class = env.find_class("com/nonpolynomial/btleplug/android/impl/Peripheral")?;
        let addr_jstr = env.new_string(format!("{:X}", addr))?;
        let obj = env.new_object(
            JClass::from(
                crate::droidplug::jni_utils::classcache::get_class(
                    "com/nonpolynomial/btleplug/android/impl/Peripheral",
                )
                .unwrap()
                .as_obj(),
            ),
            //class.as_obj(),
            "(Lcom/nonpolynomial/btleplug/android/impl/Adapter;Ljava/lang/String;)V",
            &[adapter.into(), addr_jstr.into()],
        )?;
        //Self::from_env_impl(env, obj, class)
        Self::from_env_impl(env, obj)
    }

    pub fn connect(&self) -> Result<JFuture<'a, 'b>> {
        let future_obj = self
            .env
            .call_method_unchecked(self.internal, self.connect, ReturnType::Object, &[])?
            .l()?;
        JFuture::from_env(self.env, future_obj)
    }

    pub fn disconnect(&self) -> Result<JFuture<'a, 'b>> {
        let future_obj = self
            .env
            .call_method_unchecked(self.internal, self.disconnect, ReturnType::Object, &[])?
            .l()?;
        JFuture::from_env(self.env, future_obj)
    }

    pub fn is_connected(&self) -> Result<bool> {
        self.env
            .call_method_unchecked(
                self.internal,
                self.is_connected,
                ReturnType::Primitive(Primitive::Boolean),
                &[],
            )?
            .z()
    }

    pub fn discover_services(&self) -> Result<JFuture<'a, 'b>> {
        let future_obj = self
            .env
            .call_method_unchecked(
                self.internal,
                self.discover_services,
                ReturnType::Object,
                &[],
            )?
            .l()?;
        JFuture::from_env(self.env, future_obj)
    }

    pub fn read(&self, uuid: JUuid<'a, 'b>) -> Result<JFuture<'a, 'b>> {
        let future_obj = self
            .env
            .call_method_unchecked(
                self.internal,
                self.read,
                ReturnType::Object,
                &[JValue::from(uuid).to_jni()],
            )?
            .l()?;
        JFuture::from_env(self.env, future_obj)
    }

    pub fn write(
        &self,
        uuid: JUuid<'a, 'b>,
        data: JObject<'a>,
        write_type: jint,
    ) -> Result<JFuture<'a, 'b>> {
        let future_obj = self
            .env
            .call_method_unchecked(
                self.internal,
                self.write,
                ReturnType::Object,
                &[
                    JValue::from(uuid).to_jni(),
                    JValue::from(data).to_jni(),
                    JValue::from(write_type).to_jni(),
                ],
            )?
            .l()?;
        JFuture::from_env(self.env, future_obj)
    }

    pub fn set_characteristic_notification(
        &self,
        uuid: JUuid<'a, 'b>,
        enable: bool,
    ) -> Result<JFuture<'a, 'b>> {
        let future_obj = self
            .env
            .call_method_unchecked(
                self.internal,
                self.set_characteristic_notification,
                ReturnType::Object,
                &[JValue::from(uuid).to_jni(), JValue::from(enable).to_jni()],
            )?
            .l()?;
        JFuture::from_env(self.env, future_obj)
    }

    pub fn get_notifications(&self) -> Result<JStream<'a, 'b>> {
        let stream_obj = self
            .env
            .call_method_unchecked(
                self.internal,
                self.get_notifications,
                ReturnType::Object,
                &[],
            )?
            .l()?;
        JStream::from_env(self.env, stream_obj)
    }

    pub fn read_descriptor(
        &self,
        characteristic: JUuid<'a, 'b>,
        uuid: JUuid<'a, 'b>,
    ) -> Result<JFuture<'a, 'b>> {
        let future_obj = self
            .env
            .call_method_unchecked(
                self.internal,
                self.read_descriptor,
                ReturnType::Object,
                &[
                    JValue::from(characteristic).to_jni(),
                    JValue::from(uuid).to_jni(),
                ],
            )?
            .l()?;
        JFuture::from_env(self.env, future_obj)
    }

    pub fn get_device_name(&self) -> Result<Option<String>> {
        let obj = self
            .env
            .call_method_unchecked(self.internal, self.get_device_name, ReturnType::Object, &[])?
            .l()?;
        if obj.is_null() {
            Ok(None)
        } else {
            let name_str = self.env.get_string(obj.into())?;
            Ok(Some(name_str.into()))
        }
    }

    pub fn request_mtu(&self, mtu: jint) -> Result<JObject<'a>> {
        self.env
            .call_method_unchecked(
                self.internal,
                self.request_mtu,
                ReturnType::Object,
                &[JValue::from(mtu).to_jni()],
            )?
            .l()
    }

    pub fn get_connection_parameters(&self) -> Result<Option<crate::api::ConnectionParameters>> {
        let obj = self
            .env
            .call_method_unchecked(
                self.internal,
                self.get_connection_parameters,
                ReturnType::Array,
                &[],
            )?
            .l()?;
        if obj.is_null() {
            return Ok(None);
        }
        let arr = obj.into_raw();
        let len = self.env.get_array_length(arr)?;
        if len < 3 {
            return Ok(None);
        }
        let mut buf = [0i32; 3];
        self.env.get_int_array_region(arr, 0, &mut buf)?;
        // interval is in 1.25ms units → microseconds: × 1250
        // timeout is in 10ms units → microseconds: × 10000
        Ok(Some(crate::api::ConnectionParameters {
            interval_us: (buf[0] as u32) * 1250,
            latency: buf[1] as u16,
            supervision_timeout_us: (buf[2] as u32) * 10_000,
        }))
    }

    pub fn read_remote_rssi(&self) -> Result<JObject<'a>> {
        self.env
            .call_method_unchecked(
                self.internal,
                self.read_remote_rssi,
                ReturnType::Object,
                &[],
            )?
            .l()
    }

    pub fn request_connection_priority(&self, priority: jint) -> Result<bool> {
        self.env
            .call_method_unchecked(
                self.internal,
                self.request_connection_priority,
                ReturnType::Primitive(Primitive::Boolean),
                &[JValue::from(priority).to_jni()],
            )?
            .z()
    }

    pub fn write_descriptor(
        &self,
        characteristic: JUuid<'a, 'b>,
        uuid: JUuid<'a, 'b>,
        data: JObject<'a>,
    ) -> Result<JFuture<'a, 'b>> {
        let future_obj = self
            .env
            .call_method_unchecked(
                self.internal,
                self.write_descriptor,
                ReturnType::Object,
                &[
                    JValue::from(characteristic).to_jni(),
                    JValue::from(uuid).to_jni(),
                    JValue::from(data).to_jni(),
                ],
            )?
            .l()?;
        JFuture::from_env(self.env, future_obj)
    }
}

pub struct JBluetoothGattService<'a: 'b, 'b> {
    internal: JObject<'a>,
    get_uuid: JMethodID,
    //is_primary: JMethodID,
    get_characteristics: JMethodID,
    env: &'b JNIEnv<'a>,
}

impl<'a: 'b, 'b> JBluetoothGattService<'a, 'b> {
    pub fn from_env(env: &'b JNIEnv<'a>, obj: JObject<'a>) -> Result<Self> {
        let class = env.auto_local(env.find_class("android/bluetooth/BluetoothGattService")?);

        let get_uuid = env.get_method_id(&class, "getUuid", "()Ljava/util/UUID;")?;
        //let is_primary = env.get_method_id(&class, "isPrimary", "()Z;")?;
        let get_characteristics =
            env.get_method_id(&class, "getCharacteristics", "()Ljava/util/List;")?;
        Ok(Self {
            internal: obj,
            get_uuid,
            //is_primary,
            get_characteristics,
            env,
        })
    }

    pub fn is_primary(&self) -> Result<bool> {
        /*
        self.env
        .call_method_unchecked(
            self.internal,
            self.is_primary,
            ReturnType::Primitive(Primitive::Boolean),
            &[],
        )?
        .z()
        */
        Ok(true)
    }

    pub fn get_uuid(&self) -> Result<Uuid> {
        let obj = self
            .env
            .call_method_unchecked(self.internal, self.get_uuid, ReturnType::Object, &[])?
            .l()?;
        let uuid_obj = JUuid::from_env(self.env, obj)?;
        Ok(uuid_obj.as_uuid()?)
    }

    pub fn get_characteristics(&self) -> Result<Vec<JBluetoothGattCharacteristic>> {
        let obj = self
            .env
            .call_method_unchecked(
                self.internal,
                self.get_characteristics,
                ReturnType::Object,
                &[],
            )?
            .l()?;
        let chr_list = JList::from_env(self.env, obj)?;
        let mut chr_vec = vec![];
        for chr in chr_list.iter()? {
            chr_vec.push(JBluetoothGattCharacteristic::from_env(self.env, chr)?);
        }
        Ok(chr_vec)
    }
}

pub struct JBluetoothGattCharacteristic<'a: 'b, 'b> {
    internal: JObject<'a>,
    get_uuid: JMethodID,
    get_properties: JMethodID,
    get_value: JMethodID,
    get_descriptors: JMethodID,
    env: &'b JNIEnv<'a>,
}

impl<'a: 'b, 'b> JBluetoothGattCharacteristic<'a, 'b> {
    pub fn from_env(env: &'b JNIEnv<'a>, obj: JObject<'a>) -> Result<Self> {
        let class =
            env.auto_local(env.find_class("android/bluetooth/BluetoothGattCharacteristic")?);

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
            env,
        })
    }

    pub fn get_uuid(&self) -> Result<Uuid> {
        let obj = self
            .env
            .call_method_unchecked(self.internal, self.get_uuid, ReturnType::Object, &[])?
            .l()?;
        let uuid_obj = JUuid::from_env(self.env, obj)?;
        Ok(uuid_obj.as_uuid()?)
    }

    pub fn get_properties(&self) -> Result<CharPropFlags> {
        let flags = self
            .env
            .call_method_unchecked(
                self.internal,
                self.get_properties,
                ReturnType::Primitive(Primitive::Int),
                &[],
            )?
            .i()?;
        Ok(CharPropFlags::from_bits_truncate(flags as u8))
    }

    pub fn get_value(&self) -> Result<Vec<u8>> {
        let value = self
            .env
            .call_method_unchecked(self.internal, self.get_value, ReturnType::Array, &[])?
            .l()?;
        crate::droidplug::jni_utils::arrays::byte_array_to_vec(self.env, value.into_raw())
    }

    pub fn get_descriptors(&self) -> Result<Vec<JBluetoothGattDescriptor>> {
        let obj = self
            .env
            .call_method_unchecked(self.internal, self.get_descriptors, ReturnType::Object, &[])?
            .l()?;
        let desc_list = JList::from_env(self.env, obj)?;
        let mut desc_vec = vec![];
        for desc in desc_list.iter()? {
            desc_vec.push(JBluetoothGattDescriptor::from_env(self.env, desc)?);
        }
        Ok(desc_vec)
    }
}

pub struct JBluetoothGattDescriptor<'a: 'b, 'b> {
    internal: JObject<'a>,
    get_uuid: JMethodID,
    env: &'b JNIEnv<'a>,
}

impl<'a: 'b, 'b> JBluetoothGattDescriptor<'a, 'b> {
    pub fn from_env(env: &'b JNIEnv<'a>, obj: JObject<'a>) -> Result<Self> {
        let class = env.auto_local(env.find_class("android/bluetooth/BluetoothGattDescriptor")?);

        let get_uuid = env.get_method_id(&class, "getUuid", "()Ljava/util/UUID;")?;
        Ok(Self {
            internal: obj,
            get_uuid,
            env,
        })
    }

    pub fn get_uuid(&self) -> Result<Uuid> {
        let obj = self
            .env
            .call_method_unchecked(self.internal, self.get_uuid, ReturnType::Object, &[])?
            .l()?;
        let uuid_obj = JUuid::from_env(self.env, obj)?;
        Ok(uuid_obj.as_uuid()?)
    }
}

pub struct JBluetoothDevice<'a: 'b, 'b> {
    internal: JObject<'a>,
    get_address: JMethodID,
    env: &'b JNIEnv<'a>,
}

impl<'a: 'b, 'b> JBluetoothDevice<'a, 'b> {
    pub fn from_env(env: &'b JNIEnv<'a>, obj: JObject<'a>) -> Result<Self> {
        let class = env.auto_local(env.find_class("android/bluetooth/BluetoothDevice")?);

        let get_address = env.get_method_id(&class, "getAddress", "()Ljava/lang/String;")?;
        Ok(Self {
            internal: obj,
            get_address,
            env,
        })
    }

    pub fn get_address(&self) -> Result<JString<'a>> {
        let obj = self
            .env
            .call_method_unchecked(self.internal, self.get_address, ReturnType::Object, &[])?
            .l()?;
        Ok(obj.into())
    }
}

pub struct JScanFilter<'a> {
    internal: JObject<'a>,
}

impl<'a> JScanFilter<'a> {
    pub fn new(env: &'a JNIEnv<'a>, filter: ScanFilter) -> Result<Self> {
        let uuids = env.new_object_array(
            filter.services.len() as i32,
            env.find_class("java/lang/String")?,
            JObject::null(),
        )?;
        for (idx, uuid) in filter.services.into_iter().enumerate() {
            let uuid_str = env.new_string(uuid.to_string())?;
            env.set_object_array_element(uuids, idx as i32, uuid_str)?;
        }
        let obj = env.new_object(
            JClass::from(
                crate::droidplug::jni_utils::classcache::get_class(
                    "com/nonpolynomial/btleplug/android/impl/ScanFilter",
                )
                .unwrap()
                .as_obj(),
            ),
            //class.as_obj(),
            "([Ljava/lang/String;)V",
            &[uuids.into()],
        )?;
        Ok(Self { internal: obj })
    }
}

impl<'a> From<JScanFilter<'a>> for JObject<'a> {
    fn from(value: JScanFilter<'a>) -> Self {
        value.internal
    }
}

pub struct JScanResult<'a: 'b, 'b> {
    internal: JObject<'a>,
    get_device: JMethodID,
    get_scan_record: JMethodID,
    get_tx_power: JMethodID,
    get_rssi: JMethodID,
    env: &'b JNIEnv<'a>,
}

impl<'a: 'b, 'b> JScanResult<'a, 'b> {
    pub fn from_env(env: &'b JNIEnv<'a>, obj: JObject<'a>) -> Result<Self> {
        let class = env.auto_local(env.find_class("android/bluetooth/le/ScanResult")?);

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
            env,
        })
    }

    pub fn get_device(&self) -> Result<JBluetoothDevice<'a, 'b>> {
        let obj = self
            .env
            .call_method_unchecked(self.internal, self.get_device, ReturnType::Object, &[])?
            .l()?;
        JBluetoothDevice::from_env(self.env, obj)
    }

    pub fn get_scan_record(&self) -> Result<JScanRecord<'a, 'b>> {
        let obj = self
            .env
            .call_method_unchecked(self.internal, self.get_scan_record, ReturnType::Object, &[])?
            .l()?;
        JScanRecord::from_env(self.env, obj)
    }

    pub fn get_tx_power(&self) -> Result<jint> {
        self.env
            .call_method_unchecked(
                self.internal,
                self.get_tx_power,
                ReturnType::Primitive(Primitive::Int),
                &[],
            )?
            .i()
    }

    pub fn get_rssi(&self) -> Result<jint> {
        self.env
            .call_method_unchecked(
                self.internal,
                self.get_rssi,
                ReturnType::Primitive(Primitive::Int),
                &[],
            )?
            .i()
    }
}

impl<'a: 'b, 'b> TryFrom<JScanResult<'a, 'b>> for (BDAddr, Option<PeripheralProperties>) {
    type Error = crate::Error;

    fn try_from(result: JScanResult<'a, 'b>) -> std::result::Result<Self, Self::Error> {
        use std::str::FromStr;

        let device = result.get_device()?;

        let addr_obj = device.get_address()?;
        let addr_str = JavaStr::from_env(result.env, addr_obj)?;
        let addr = BDAddr::from_str(
            addr_str
                .to_str()
                .map_err(|e| Self::Error::Other(e.into()))?,
        )?;

        let record = result.get_scan_record()?;
        let record_obj: &JObject = &record;
        let properties = if result
            .env
            .is_same_object(record_obj.clone(), JObject::null())?
        {
            None
        } else {
            let device_name_obj = record.get_device_name()?;
            let device_name = if result
                .env
                .is_same_object(device_name_obj, JObject::null())?
            {
                None
            } else {
                let device_name_str = JavaStr::from_env(result.env, device_name_obj)?;
                // On Android, there is a chance that a device name may not actually be valid UTF-8.
                // We're given the full buffer, regardless of if it's just UTF-8 characters,
                // possibly c str with null characters, or whatever. We should try UTF-8 first, if
                // that doesn't work out, see if there's a null termination character in it and try
                // parsing that.
                Some(
                    String::from_utf8_lossy(device_name_str.to_bytes())
                        .chars()
                        .filter(|&c| c != '\u{fffd}')
                        .collect(),
                )
            };

            let tx_power_level = result.get_tx_power()?;
            const TX_POWER_NOT_PRESENT: jint = 127; // from ScanResult documentation
            let tx_power_level = if tx_power_level == TX_POWER_NOT_PRESENT {
                None
            } else {
                Some(tx_power_level as i16)
            };

            let rssi = Some(result.get_rssi()? as i16);

            let manufacturer_specific_data_array = record.get_manufacturer_specific_data()?;
            let manufacturer_specific_data_obj: &JObject = &manufacturer_specific_data_array;
            let mut manufacturer_data = HashMap::new();
            if !result
                .env
                .is_same_object(manufacturer_specific_data_obj.clone(), JObject::null())?
            {
                for item in manufacturer_specific_data_array.iter() {
                    let (index, data) = item?;

                    let index = index as u16;
                    let data = crate::droidplug::jni_utils::arrays::byte_array_to_vec(
                        result.env,
                        data.into_raw(),
                    )?;
                    manufacturer_data.insert(index, data);
                }
            }

            let service_data_map = record.get_service_data()?;
            let service_data_obj: &JObject = &service_data_map;
            let mut service_data = HashMap::new();
            if !result
                .env
                .is_same_object(service_data_obj.clone(), JObject::null())?
            {
                for (key, value) in service_data_map.iter()? {
                    let uuid = JParcelUuid::from_env(result.env, key)?
                        .get_uuid()?
                        .as_uuid()?;
                    let data = crate::droidplug::jni_utils::arrays::byte_array_to_vec(
                        result.env,
                        value.into_raw(),
                    )?;
                    service_data.insert(uuid, data);
                }
            }

            let services_list = record.get_service_uuids()?;
            let services_obj: &JObject = &services_list;
            let mut services = Vec::new();
            if !result
                .env
                .is_same_object(services_obj.clone(), JObject::null())?
            {
                for obj in services_list.iter()? {
                    let uuid = JParcelUuid::from_env(result.env, obj)?
                        .get_uuid()?
                        .as_uuid()?;
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

pub struct JScanRecord<'a: 'b, 'b> {
    internal: JObject<'a>,
    get_device_name: JMethodID,
    get_tx_power_level: JMethodID,
    get_manufacturer_specific_data: JMethodID,
    get_service_data: JMethodID,
    get_service_uuids: JMethodID,
    env: &'b JNIEnv<'a>,
}

impl<'a: 'b, 'b> From<JScanRecord<'a, 'b>> for JObject<'a> {
    fn from(scan_record: JScanRecord<'a, 'b>) -> Self {
        scan_record.internal
    }
}

impl<'a: 'b, 'b> ::std::ops::Deref for JScanRecord<'a, 'b> {
    type Target = JObject<'a>;

    fn deref(&self) -> &Self::Target {
        &self.internal
    }
}

impl<'a: 'b, 'b> JScanRecord<'a, 'b> {
    pub fn from_env(env: &'b JNIEnv<'a>, obj: JObject<'a>) -> Result<Self> {
        let class = env.auto_local(env.find_class("android/bluetooth/le/ScanRecord")?);

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
            env,
        })
    }

    pub fn get_device_name(&self) -> Result<JString<'a>> {
        let obj = self
            .env
            .call_method_unchecked(self.internal, self.get_device_name, ReturnType::Object, &[])?
            .l()?;
        Ok(obj.into())
    }

    pub fn get_tx_power_level(&self) -> Result<jint> {
        self.env
            .call_method_unchecked(
                self.internal,
                self.get_tx_power_level,
                ReturnType::Primitive(Primitive::Int),
                &[],
            )?
            .i()
    }

    pub fn get_manufacturer_specific_data(&self) -> Result<JSparseArray<'a, 'b>> {
        let obj = self
            .env
            .call_method_unchecked(
                self.internal,
                self.get_manufacturer_specific_data,
                ReturnType::Object,
                &[],
            )?
            .l()?;
        JSparseArray::from_env(self.env, obj)
    }

    pub fn get_service_data(&self) -> Result<JMap<'a, 'b>> {
        let obj = self
            .env
            .call_method_unchecked(
                self.internal,
                self.get_service_data,
                ReturnType::Object,
                &[],
            )?
            .l()?;
        JMap::from_env(self.env, obj)
    }

    pub fn get_service_uuids(&self) -> Result<JList<'a, 'b>> {
        let obj = self
            .env
            .call_method_unchecked(
                self.internal,
                self.get_service_uuids,
                ReturnType::Object,
                &[],
            )?
            .l()?;
        JList::from_env(self.env, obj)
    }
}

#[derive(Clone)]
pub struct JSparseArray<'a: 'b, 'b> {
    internal: JObject<'a>,
    size: JMethodID,
    key_at: JMethodID,
    value_at: JMethodID,
    env: &'b JNIEnv<'a>,
}

impl<'a: 'b, 'b> From<JSparseArray<'a, 'b>> for JObject<'a> {
    fn from(sparse_array: JSparseArray<'a, 'b>) -> Self {
        sparse_array.internal
    }
}

impl<'a: 'b, 'b> ::std::ops::Deref for JSparseArray<'a, 'b> {
    type Target = JObject<'a>;

    fn deref(&self) -> &Self::Target {
        &self.internal
    }
}

impl<'a: 'b, 'b> JSparseArray<'a, 'b> {
    pub fn from_env(env: &'b JNIEnv<'a>, obj: JObject<'a>) -> Result<Self> {
        let class = env.auto_local(env.find_class("android/util/SparseArray")?);

        let size = env.get_method_id(&class, "size", "()I")?;
        let key_at = env.get_method_id(&class, "keyAt", "(I)I")?;
        let value_at = env.get_method_id(&class, "valueAt", "(I)Ljava/lang/Object;")?;
        Ok(Self {
            internal: obj,
            size,
            key_at,
            value_at,
            env,
        })
    }

    pub fn size(&self) -> Result<jint> {
        self.env
            .call_method_unchecked(
                self.internal,
                self.size,
                ReturnType::Primitive(Primitive::Int),
                &[],
            )?
            .i()
    }

    pub fn key_at(&self, index: jint) -> Result<jint> {
        self.env
            .call_method_unchecked(
                self.internal,
                self.key_at,
                ReturnType::Primitive(Primitive::Int),
                &[JValue::from(index).to_jni()],
            )?
            .i()
    }

    pub fn value_at(&self, index: jint) -> Result<JObject<'a>> {
        self.env
            .call_method_unchecked(
                self.internal,
                self.value_at,
                ReturnType::Object,
                &[JValue::from(index).to_jni()],
            )?
            .l()
    }

    pub fn iter(&self) -> JSparseArrayIter<'a, 'b> {
        JSparseArrayIter {
            internal: self.clone(),
            index: 0,
        }
    }
}

pub struct JSparseArrayIter<'a: 'b, 'b> {
    internal: JSparseArray<'a, 'b>,
    index: jint,
}

impl<'a: 'b, 'b> JSparseArrayIter<'a, 'b> {
    fn next_internal(&mut self) -> Result<Option<(jint, JObject<'a>)>> {
        let size = self.internal.size()?;
        Ok(if self.index >= size {
            None
        } else {
            let key = self.internal.key_at(self.index)?;
            let value = self.internal.value_at(self.index)?;
            self.index += 1;
            Some((key, value))
        })
    }
}

impl<'a: 'b, 'b> Iterator for JSparseArrayIter<'a, 'b> {
    type Item = Result<(jint, JObject<'a>)>;

    fn next(&mut self) -> Option<Self::Item> {
        self.next_internal().transpose()
    }
}
pub struct JParcelUuid<'a: 'b, 'b> {
    internal: JObject<'a>,
    get_uuid: JMethodID,
    env: &'b JNIEnv<'a>,
}

impl<'a: 'b, 'b> JParcelUuid<'a, 'b> {
    pub fn from_env(env: &'b JNIEnv<'a>, obj: JObject<'a>) -> Result<Self> {
        let class = env.auto_local(env.find_class("android/os/ParcelUuid")?);

        let get_uuid = env.get_method_id(&class, "getUuid", "()Ljava/util/UUID;")?;
        Ok(Self {
            internal: obj,
            get_uuid,
            env,
        })
    }

    pub fn get_uuid(&self) -> Result<JUuid<'a, 'b>> {
        let obj = self
            .env
            .call_method_unchecked(self.internal, self.get_uuid, ReturnType::Object, &[])?
            .l()?;
        JUuid::from_env(self.env, obj)
    }
}
