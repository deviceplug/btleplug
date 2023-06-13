use jni::{
    errors::Result,
    objects::{JClass, JList, JMap, JMethodID, JObject, JString},
    signature::{JavaType, Primitive},
    strings::JavaStr,
    sys::jint,
    JNIEnv,
};
use jni_utils::{future::JFuture, stream::JStream, uuid::JUuid};
use std::{collections::HashMap, convert::TryFrom, iter::Iterator};
use uuid::Uuid;

use crate::api::{BDAddr, CharPropFlags, PeripheralProperties, ScanFilter};

pub struct JPeripheral<'a: 'b, 'b> {
    internal: JObject<'a>,
    connect: JMethodID<'a>,
    disconnect: JMethodID<'a>,
    is_connected: JMethodID<'a>,
    discover_services: JMethodID<'a>,
    read: JMethodID<'a>,
    write: JMethodID<'a>,
    set_characteristic_notification: JMethodID<'a>,
    get_notifications: JMethodID<'a>,
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
        let class_static =
            jni_utils::classcache::get_class("com/nonpolynomial/btleplug/android/impl/Peripheral")
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
            env,
        })
    }

    pub fn new(env: &'b JNIEnv<'a>, adapter: JObject<'a>, addr: BDAddr) -> Result<Self> {
        // let class = env.find_class("com/nonpolynomial/btleplug/android/impl/Peripheral")?;
        let addr_jstr = env.new_string(format!("{:X}", addr))?;
        let obj = env.new_object(
            JClass::from(
                jni_utils::classcache::get_class(
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
            .call_method_unchecked(
                self.internal,
                self.connect,
                JavaType::Object("Lio/github/gedgygedgy/rust/future/Future;".to_string()),
                &[],
            )?
            .l()?;
        JFuture::from_env(self.env, future_obj)
    }

    pub fn disconnect(&self) -> Result<JFuture<'a, 'b>> {
        let future_obj = self
            .env
            .call_method_unchecked(
                self.internal,
                self.disconnect,
                JavaType::Object("Lio/github/gedgygedgy/rust/future/Future;".to_string()),
                &[],
            )?
            .l()?;
        JFuture::from_env(self.env, future_obj)
    }

    pub fn is_connected(&self) -> Result<bool> {
        self.env
            .call_method_unchecked(
                self.internal,
                self.is_connected,
                JavaType::Primitive(Primitive::Boolean),
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
                JavaType::Object("Lio/github/gedgygedgy/rust/future/Future;".to_string()),
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
                JavaType::Object("Lio/github/gedgygedgy/rust/future/Future;".to_string()),
                &[uuid.into()],
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
                JavaType::Object("Lio/github/gedgygedgy/rust/future/Future;".to_string()),
                &[uuid.into(), data.into(), write_type.into()],
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
                JavaType::Object("Lio/github/gedgygedgy/rust/future/Future;".to_string()),
                &[uuid.into(), enable.into()],
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
                JavaType::Object("Lio/github/gedgygedgy/rust/stream/Stream;".to_string()),
                &[],
            )?
            .l()?;
        JStream::from_env(self.env, stream_obj)
    }
}

pub struct JBluetoothGattService<'a: 'b, 'b> {
    internal: JObject<'a>,
    get_uuid: JMethodID<'a>,
    //is_primary: JMethodID<'a>,
    get_characteristics: JMethodID<'a>,
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
            JavaType::Primitive(Primitive::Boolean),
            &[],
        )?
        .z()
        */
        Ok(true)
    }

    pub fn get_uuid(&self) -> Result<Uuid> {
        let obj = self
            .env
            .call_method_unchecked(
                self.internal,
                self.get_uuid,
                JavaType::Object("Ljava/util/UUID;".to_string()),
                &[],
            )?
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
                JavaType::Object("Ljava/util/List;".to_string()),
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
    get_uuid: JMethodID<'a>,
    get_properties: JMethodID<'a>,
    get_value: JMethodID<'a>,
    env: &'b JNIEnv<'a>,
}

impl<'a: 'b, 'b> JBluetoothGattCharacteristic<'a, 'b> {
    pub fn from_env(env: &'b JNIEnv<'a>, obj: JObject<'a>) -> Result<Self> {
        let class =
            env.auto_local(env.find_class("android/bluetooth/BluetoothGattCharacteristic")?);

        let get_uuid = env.get_method_id(&class, "getUuid", "()Ljava/util/UUID;")?;
        let get_properties = env.get_method_id(&class, "getProperties", "()I")?;
        let get_value = env.get_method_id(&class, "getValue", "()[B")?;
        Ok(Self {
            internal: obj,
            get_uuid,
            get_properties,
            get_value,
            env,
        })
    }

    pub fn get_uuid(&self) -> Result<Uuid> {
        let obj = self
            .env
            .call_method_unchecked(
                self.internal,
                self.get_uuid,
                JavaType::Object("Ljava/util/UUID;".to_string()),
                &[],
            )?
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
                JavaType::Primitive(Primitive::Int),
                &[],
            )?
            .i()?;
        Ok(CharPropFlags::from_bits_truncate(flags as u8))
    }

    pub fn get_value(&self) -> Result<Vec<u8>> {
        let value = self
            .env
            .call_method_unchecked(
                self.internal,
                self.get_value,
                JavaType::Array(JavaType::Primitive(Primitive::Byte).into()),
                &[],
            )?
            .l()?;
        jni_utils::arrays::byte_array_to_vec(self.env, value.into_inner())
    }
}

pub struct JBluetoothDevice<'a: 'b, 'b> {
    internal: JObject<'a>,
    get_address: JMethodID<'a>,
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
            .call_method_unchecked(
                self.internal,
                self.get_address,
                JavaType::Object("Ljava/lang/String;".to_string()),
                &[],
            )?
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
                jni_utils::classcache::get_class(
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
    get_device: JMethodID<'a>,
    get_scan_record: JMethodID<'a>,
    get_tx_power: JMethodID<'a>,
    get_rssi: JMethodID<'a>,
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
            .call_method_unchecked(
                self.internal,
                self.get_device,
                JavaType::Object("Landroid/bluetooth/BluetoothDevice;".to_string()),
                &[],
            )?
            .l()?;
        JBluetoothDevice::from_env(self.env, obj)
    }

    pub fn get_scan_record(&self) -> Result<JScanRecord<'a, 'b>> {
        let obj = self
            .env
            .call_method_unchecked(
                self.internal,
                self.get_scan_record,
                JavaType::Object("Landroid/bluetooth/le/ScanRecord;".to_string()),
                &[],
            )?
            .l()?;
        JScanRecord::from_env(self.env, obj)
    }

    pub fn get_tx_power(&self) -> Result<jint> {
        self.env
            .call_method_unchecked(
                self.internal,
                self.get_tx_power,
                JavaType::Primitive(Primitive::Int),
                &[],
            )?
            .i()
    }

    pub fn get_rssi(&self) -> Result<jint> {
        self.env
            .call_method_unchecked(
                self.internal,
                self.get_rssi,
                JavaType::Primitive(Primitive::Int),
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
                Some(
                    device_name_str
                        .to_str()
                        .map_err(|e| Self::Error::Other(e.into()))?
                        .to_string(),
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
                    let data = jni_utils::arrays::byte_array_to_vec(result.env, data.into_inner())?;
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
                    let data =
                        jni_utils::arrays::byte_array_to_vec(result.env, value.into_inner())?;
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
                local_name: device_name,
                tx_power_level,
                manufacturer_data,
                service_data,
                services,
                rssi,
            })
        };
        Ok((addr, properties))
    }
}

pub struct JScanRecord<'a: 'b, 'b> {
    internal: JObject<'a>,
    get_device_name: JMethodID<'a>,
    get_tx_power_level: JMethodID<'a>,
    get_manufacturer_specific_data: JMethodID<'a>,
    get_service_data: JMethodID<'a>,
    get_service_uuids: JMethodID<'a>,
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
            .call_method_unchecked(
                self.internal,
                self.get_device_name,
                JavaType::Object("Ljava/lang/String;".to_string()),
                &[],
            )?
            .l()?;
        Ok(obj.into())
    }

    pub fn get_tx_power_level(&self) -> Result<jint> {
        self.env
            .call_method_unchecked(
                self.internal,
                self.get_tx_power_level,
                JavaType::Primitive(Primitive::Int),
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
                JavaType::Object("Landroid/util/SparseArray;".to_string()),
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
                JavaType::Object("Ljava/util/Map;".to_string()),
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
                JavaType::Object("Ljava/util/List;".to_string()),
                &[],
            )?
            .l()?;
        JList::from_env(self.env, obj)
    }
}

#[derive(Clone)]
pub struct JSparseArray<'a: 'b, 'b> {
    internal: JObject<'a>,
    size: JMethodID<'a>,
    key_at: JMethodID<'a>,
    value_at: JMethodID<'a>,
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
                JavaType::Primitive(Primitive::Int),
                &[],
            )?
            .i()
    }

    pub fn key_at(&self, index: jint) -> Result<jint> {
        self.env
            .call_method_unchecked(
                self.internal,
                self.key_at,
                JavaType::Primitive(Primitive::Int),
                &[index.into()],
            )?
            .i()
    }

    pub fn value_at(&self, index: jint) -> Result<JObject<'a>> {
        self.env
            .call_method_unchecked(
                self.internal,
                self.value_at,
                JavaType::Object("Ljava/lang/Object;".to_string()),
                &[index.into()],
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
    get_uuid: JMethodID<'a>,
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
            .call_method_unchecked(
                self.internal,
                self.get_uuid,
                JavaType::Object("Ljava/util/UUID;".to_string()),
                &[],
            )?
            .l()?;
        JUuid::from_env(self.env, obj)
    }
}
