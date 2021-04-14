// btleplug Source Code File
//
// Copyright 2020 Nonpolynomial Labs LLC. All rights reserved.
//
// Licensed under the BSD 3-Clause license. See LICENSE file in the project root
// for full license information.
//
// Some portions of this file are taken and/or modified from blurmac
// (https://github.com/servo/devices), using a BSD 3-Clause license under the
// following copyright:
//
// Copyright (c) 2017 Akos Kiss.
//
// Licensed under the BSD 3-Clause License
// <LICENSE.md or https://opensource.org/licenses/BSD-3-Clause>.
// This file may not be copied, modified, or distributed except
// according to those terms.

use objc::runtime::{Class, Object, BOOL};
use objc::{msg_send, sel, sel_impl};
use std::os::raw::{c_char, c_int, c_uint};

#[allow(non_upper_case_globals)]
pub const nil: *mut Object = 0 as *mut Object;

pub mod ns {
    use super::*;

    // NSObject

    pub fn object_copy(nsobject: *mut Object) -> *mut Object {
        unsafe { msg_send![nsobject, copy] }
    }

    // NSNumber

    pub fn number_withbool(value: BOOL) -> *mut Object {
        unsafe { msg_send![Class::get("NSNumber").unwrap(), numberWithBool: value] }
    }

    pub fn number_withunsignedlonglong(value: u64) -> *mut Object {
        unsafe {
            msg_send![
                Class::get("NSNumber").unwrap(),
                numberWithUnsignedLongLong: value
            ]
        }
    }

    pub fn number_unsignedlonglongvalue(nsnumber: *mut Object) -> u64 {
        unsafe { msg_send![nsnumber, unsignedLongLongValue] }
    }

    // NSString

    pub fn string(cstring: *const c_char) -> *mut Object /* NSString* */ {
        unsafe {
            msg_send![
                Class::get("NSString").unwrap(),
                stringWithUTF8String: cstring
            ]
        }
    }

    pub fn string_utf8string(nsstring: *mut Object) -> *const c_char {
        unsafe { msg_send![nsstring, UTF8String] }
    }

    // NSArray

    pub fn array_count(nsarray: *mut Object) -> c_uint {
        unsafe { msg_send![nsarray, count] }
    }

    pub fn array_objectatindex(nsarray: *mut Object, index: c_uint) -> *mut Object {
        unsafe { msg_send![nsarray, objectAtIndex: index] }
    }

    // NSDictionary

    pub fn dictionary_allkeys(nsdict: *mut Object) -> *mut Object /* NSArray* */ {
        unsafe { msg_send![nsdict, allKeys] }
    }

    pub fn dictionary_objectforkey(nsdict: *mut Object, key: *mut Object) -> *mut Object {
        unsafe { msg_send![nsdict, objectForKey: key] }
    }

    // NSMutableDictionary : NSDictionary

    pub fn mutabledictionary() -> *mut Object {
        unsafe { msg_send![Class::get("NSMutableDictionary").unwrap(), dictionaryWithCapacity:0] }
    }

    pub fn mutabledictionary_removeobjectforkey(nsmutdict: *mut Object, key: *mut Object) {
        unsafe { msg_send![nsmutdict, removeObjectForKey: key] }
    }

    pub fn mutabledictionary_setobject_forkey(
        nsmutdict: *mut Object,
        object: *mut Object,
        key: *mut Object,
    ) {
        unsafe { msg_send![nsmutdict, setObject:object forKey:key] }
    }

    // NSData

    pub fn data(bytes: *const u8, length: c_uint) -> *mut Object /* NSData* */ {
        unsafe { msg_send![Class::get("NSData").unwrap(), dataWithBytes:bytes length:length] }
    }

    pub fn data_length(nsdata: *mut Object) -> c_uint {
        unsafe { msg_send![nsdata, length] }
    }

    pub fn data_bytes(nsdata: *mut Object) -> *const u8 {
        unsafe { msg_send![nsdata, bytes] }
    }

    // NSUUID

    pub fn uuid_uuidstring(nsuuid: *mut Object) -> *mut Object /* NSString* */ {
        unsafe {
            let uuidstring: *mut Object = msg_send![nsuuid, UUIDString];
            uuidstring
        }
    }
}

pub mod io {
    use super::*;

    #[link(name = "IOBluetooth", kind = "framework")]
    extern "C" {
        pub fn IOBluetoothPreferenceGetControllerPowerState() -> c_int;
        pub fn IOBluetoothPreferenceSetControllerPowerState(state: c_int);

        pub fn IOBluetoothPreferenceGetDiscoverableState() -> c_int;
        pub fn IOBluetoothPreferenceSetDiscoverableState(state: c_int);
    }

    // IOBluetoothHostController

    pub fn bluetoothhostcontroller_defaultcontroller() -> *mut Object /* IOBluetoothHostController* */
    {
        unsafe {
            msg_send![
                Class::get("IOBluetoothHostController").unwrap(),
                defaultController
            ]
        }
    }

    pub fn bluetoothhostcontroller_nameasstring(iobthc: *mut Object) -> *mut Object /* NSString* */
    {
        unsafe { msg_send![iobthc, nameAsString] }
    }

    pub fn bluetoothhostcontroller_addressasstring(iobthc: *mut Object) -> *mut Object /* NSString* */
    {
        unsafe { msg_send![iobthc, addressAsString] }
    }

    pub fn bluetoothhostcontroller_classofdevice(iobthc: *mut Object) -> u32 {
        unsafe { msg_send![iobthc, classOfDevice] }
    }

    // IOBluetoothPreference...

    pub fn bluetoothpreferencegetcontrollerpowerstate() -> c_int {
        unsafe { IOBluetoothPreferenceGetControllerPowerState() }
    }

    pub fn bluetoothpreferencesetcontrollerpowerstate(state: c_int) {
        unsafe {
            IOBluetoothPreferenceSetControllerPowerState(state);
        }
    }

    pub fn bluetoothpreferencegetdiscoverablestate() -> c_int {
        unsafe { IOBluetoothPreferenceGetDiscoverableState() }
    }

    pub fn bluetoothpreferencesetdiscoverablestate(state: c_int) {
        unsafe {
            IOBluetoothPreferenceSetDiscoverableState(state);
        }
    }
}

pub mod cb {
    use super::*;
    use std::ffi::CString;

    #[allow(non_camel_case_types)]
    pub enum dispatch_object_s {}
    #[allow(non_camel_case_types)]
    pub type dispatch_queue_t = *mut dispatch_object_s;
    #[allow(non_camel_case_types)]
    pub type dispatch_queue_attr_t = *const dispatch_object_s;
    pub const DISPATCH_QUEUE_SERIAL: dispatch_queue_attr_t = 0 as dispatch_queue_attr_t;

    #[link(name = "AppKit", kind = "framework")]
    #[link(name = "Foundation", kind = "framework")]
    #[link(name = "CoreBluetooth", kind = "framework")]
    extern "C" {
        pub fn dispatch_queue_create(
            label: *const c_char,
            attr: dispatch_queue_attr_t,
        ) -> dispatch_queue_t;
    }

    mod link {
        use super::*;

        #[link(name = "CoreBluetooth", kind = "framework")]
        extern "C" {
            pub static CBAdvertisementDataManufacturerDataKey: *mut Object;
            pub static CBAdvertisementDataServiceDataKey: *mut Object;
            pub static CBAdvertisementDataServiceUUIDsKey: *mut Object;

            pub static CBCentralManagerScanOptionAllowDuplicatesKey: *mut Object;
        }
    }

    // CBCentralManager

    pub fn centralmanager(delegate: *mut Object, /*CBCentralManagerDelegate* */) -> *mut Object /*CBCentralManager* */
    {
        let label = CString::new("CBqueue").unwrap();
        unsafe {
            let cbcentralmanager: *mut Object =
                msg_send![Class::get("CBCentralManager").unwrap(), alloc];
            let queue = dispatch_queue_create(label.as_ptr(), DISPATCH_QUEUE_SERIAL);

            msg_send![cbcentralmanager, initWithDelegate:delegate queue:queue]
        }
    }

    pub fn centralmanager_scanforperipherals_options(
        cbcentralmanager: *mut Object,
        options: *mut Object, /* NSDictionary<NSString*,id> */
    ) {
        unsafe { msg_send![cbcentralmanager, scanForPeripheralsWithServices:nil options:options] }
    }

    pub fn centralmanager_stopscan(cbcentralmanager: *mut Object) {
        unsafe { msg_send![cbcentralmanager, stopScan] }
    }

    pub fn centralmanager_connectperipheral(
        cbcentralmanager: *mut Object,
        peripheral: *mut Object, /* CBPeripheral* */
    ) {
        unsafe { msg_send![cbcentralmanager, connectPeripheral:peripheral options:nil] }
    }

    pub fn centralmanager_cancelperipheralconnection(
        cbcentralmanager: *mut Object,
        peripheral: *mut Object, /* CBPeripheral* */
    ) {
        unsafe { msg_send![cbcentralmanager, cancelPeripheralConnection: peripheral] }
    }

    // CBManager
    pub fn manager_authorization() -> CBManagerAuthorization {
        unsafe { msg_send![Class::get("CBManager").unwrap(), authorization] }
    }

    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    #[repr(i64)]
    pub enum CBManagerAuthorization {
        NotDetermined = 0,
        Restricted = 1,
        Denied = 2,
        AllowedAlways = 3,
    }

    // CBPeer

    pub fn peer_identifier(cbpeer: *mut Object) -> *mut Object /* NSUUID* */ {
        unsafe { msg_send![cbpeer, identifier] }
    }

    // CBPeripheral : CBPeer

    pub fn peripheral_name(cbperipheral: *mut Object) -> *mut Object /* NSString* */ {
        unsafe { msg_send![cbperipheral, name] }
    }

    pub fn peripheral_state(cbperipheral: *mut Object) -> c_int {
        unsafe { msg_send![cbperipheral, state] }
    }

    pub fn peripheral_setdelegate(
        cbperipheral: *mut Object,
        delegate: *mut Object, /* CBPeripheralDelegate* */
    ) {
        unsafe { msg_send![cbperipheral, setDelegate: delegate] }
    }

    pub fn peripheral_discoverservices(cbperipheral: *mut Object) {
        unsafe { msg_send![cbperipheral, discoverServices: nil] }
    }

    pub fn peripheral_discoverincludedservicesforservice(
        cbperipheral: *mut Object,
        service: *mut Object, /* CBService* */
    ) {
        unsafe { msg_send![cbperipheral, discoverIncludedServices:nil forService:service] }
    }

    pub fn peripheral_services(cbperipheral: *mut Object) -> *mut Object /* NSArray<CBService*>* */
    {
        unsafe { msg_send![cbperipheral, services] }
    }

    pub fn peripheral_discovercharacteristicsforservice(
        cbperipheral: *mut Object,
        service: *mut Object, /* CBService* */
    ) {
        unsafe { msg_send![cbperipheral, discoverCharacteristics:nil forService:service] }
    }

    pub fn peripheral_readvalue_forcharacteristic(
        cbperipheral: *mut Object,
        characteristic: *mut Object, /* CBCharacteristic* */
    ) {
        unsafe { msg_send![cbperipheral, readValueForCharacteristic: characteristic] }
    }

    pub fn peripheral_writevalue_forcharacteristic(
        cbperipheral: *mut Object,
        value: *mut Object,          /* NSData* */
        characteristic: *mut Object, /* CBCharacteristic* */
        write_type: usize,
    ) {
        unsafe {
            msg_send![cbperipheral, writeValue:value forCharacteristic:characteristic type:write_type]
            // CBCharacteristicWriteWithResponse from CBPeripheral.h
        }
    }

    pub fn peripheral_setnotifyvalue_forcharacteristic(
        cbperipheral: *mut Object,
        value: BOOL,
        characteristic: *mut Object, /* CBCharacteristic* */
    ) {
        unsafe { msg_send![cbperipheral, setNotifyValue:value forCharacteristic:characteristic] }
    }

    pub fn peripheral_discoverdescriptorsforcharacteristic(
        cbperipheral: *mut Object,
        characteristic: *mut Object, /* CBCharacteristic* */
    ) {
        unsafe {
            msg_send![
                cbperipheral,
                discoverDescriptorsForCharacteristic: characteristic
            ]
        }
    }

    // CBPeripheralState = NSInteger from CBPeripheral.h

    pub const PERIPHERALSTATE_CONNECTED: c_int = 2; // CBPeripheralStateConnected

    // CBAttribute

    pub fn attribute_uuid(cbattribute: *mut Object) -> *mut Object /* CBUUID* */ {
        unsafe { msg_send![cbattribute, UUID] }
    }

    // CBService : CBAttribute

    // pub fn service_isprimary(cbservice: *mut Object) -> BOOL {
    //     unsafe {
    //         let isprimary: BOOL = msg_send![cbservice, isPrimary];
    //         isprimary
    //     }
    // }

    pub fn service_includedservices(cbservice: *mut Object) -> *mut Object /* NSArray<CBService*>* */
    {
        unsafe { msg_send![cbservice, includedServices] }
    }

    pub fn service_characteristics(cbservice: *mut Object) -> *mut Object /* NSArray<CBCharacteristic*>* */
    {
        unsafe { msg_send![cbservice, characteristics] }
    }

    // CBCharacteristic : CBAttribute

    pub fn characteristic_isnotifying(cbcharacteristic: *mut Object) -> BOOL {
        unsafe { msg_send![cbcharacteristic, isNotifying] }
    }

    pub fn characteristic_value(cbcharacteristic: *mut Object) -> *mut Object /* NSData* */ {
        unsafe { msg_send![cbcharacteristic, value] }
    }

    pub fn characteristic_properties(cbcharacteristic: *mut Object) -> c_uint {
        unsafe { msg_send![cbcharacteristic, properties] }
    }

    // CBCharacteristicProperties = NSUInteger from CBCharacteristic.h

    pub const CHARACTERISTICPROPERTY_BROADCAST: c_uint = 0x01; // CBCharacteristicPropertyBroadcast
    pub const CHARACTERISTICPROPERTY_READ: c_uint = 0x02; // CBCharacteristicPropertyRead
    pub const CHARACTERISTICPROPERTY_WRITEWITHOUTRESPONSE: c_uint = 0x04; // CBCharacteristicPropertyWriteWithoutResponse
    pub const CHARACTERISTICPROPERTY_WRITE: c_uint = 0x08; // CBCharacteristicPropertyWrite
    pub const CHARACTERISTICPROPERTY_NOTIFY: c_uint = 0x10; // CBCharacteristicPropertyNotify
    pub const CHARACTERISTICPROPERTY_INDICATE: c_uint = 0x20; // CBCharacteristicPropertyIndicate
    pub const CHARACTERISTICPROPERTY_AUTHENTICATEDSIGNEDWRITES: c_uint = 0x40; // CBCharacteristicPropertyAuthenticatedSignedWrites

    // CBUUID

    pub fn uuid_uuidstring(cbuuid: *mut Object) -> *mut Object /* NSString* */ {
        unsafe { msg_send![cbuuid, UUIDString] }
    }

    // CBCentralManagerScanOption...Key

    pub use self::link::CBCentralManagerScanOptionAllowDuplicatesKey as CENTRALMANAGERSCANOPTIONALLOWDUPLICATESKEY;

    // CBAdvertisementData...Key

    pub use self::link::CBAdvertisementDataManufacturerDataKey as ADVERTISEMENT_DATA_MANUFACTURER_DATA_KEY;
    pub use self::link::CBAdvertisementDataServiceDataKey as ADVERTISEMENT_DATA_SERVICE_DATA_KEY;
    pub use self::link::CBAdvertisementDataServiceUUIDsKey as ADVERTISEMENT_DATA_SERVICE_UUIDS_KEY;
}
