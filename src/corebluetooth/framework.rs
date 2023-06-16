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

use cocoa::{
    base::{id, nil},
    foundation::{NSArray, NSData, NSDictionary, NSString, NSUInteger},
};
use objc::runtime::BOOL;
use objc::{class, msg_send, sel, sel_impl};
use std::os::raw::{c_char, c_int, c_uint};

pub mod ns {
    use super::*;

    // NSNumber

    pub fn number_withbool(value: BOOL) -> id {
        unsafe { msg_send![class!(NSNumber), numberWithBool: value] }
    }

    pub fn number_as_i64(value: id) -> i64 {
        unsafe {
            let i: cocoa::foundation::NSInteger = msg_send![&*value, integerValue];
            i as i64
        }
    }

    // NSArray

    pub fn array_count(nsarray: impl NSArray) -> NSUInteger {
        unsafe { nsarray.count() }
    }

    pub fn array_objectatindex(nsarray: impl NSArray, index: NSUInteger) -> id {
        unsafe { nsarray.objectAtIndex(index) }
    }

    // NSDictionary

    pub fn dictionary_allkeys(nsdict: impl NSDictionary) -> id /* NSArray* */ {
        unsafe { nsdict.allKeys() }
    }

    pub fn dictionary_objectforkey(nsdict: impl NSDictionary, key: id) -> id {
        unsafe { nsdict.objectForKey_(key) }
    }

    // NSMutableDictionary : NSDictionary

    pub fn mutabledictionary() -> id {
        unsafe { msg_send![class!(NSMutableDictionary), dictionaryWithCapacity:0] }
    }

    pub fn mutabledictionary_setobject_forkey(nsmutdict: id, object: id, key: id) {
        unsafe { msg_send![nsmutdict, setObject:object forKey:key] }
    }

    // NSData

    pub fn data(bytes: &[u8]) -> id /* NSData* */ {
        unsafe {
            msg_send![class!(NSData), dataWithBytes:bytes.as_ptr() length:bytes.len() as NSUInteger]
        }
    }

    pub fn data_length(nsdata: impl NSData) -> NSUInteger {
        unsafe { nsdata.length() }
    }

    pub fn data_bytes(nsdata: id) -> *const u8 {
        unsafe { msg_send![nsdata, bytes] }
    }

    // NSUUID

    pub fn uuid_uuidstring(nsuuid: id) -> id /* NSString* */ {
        unsafe {
            let uuidstring: id = msg_send![nsuuid, UUIDString];
            uuidstring
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

    #[cfg_attr(target_os = "macos", link(name = "AppKit", kind = "framework"))]
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
            pub static CBAdvertisementDataManufacturerDataKey: id;
            pub static CBAdvertisementDataServiceDataKey: id;
            pub static CBAdvertisementDataServiceUUIDsKey: id;

            pub static CBCentralManagerScanOptionAllowDuplicatesKey: id;
        }
    }

    // CBCentralManager

    pub fn centralmanager(delegate: id /*CBCentralManagerDelegate* */) -> id /*CBCentralManager* */
    {
        let label = CString::new("CBqueue").unwrap();
        unsafe {
            let cbcentralmanager: id = msg_send![class!(CBCentralManager), alloc];
            let queue = dispatch_queue_create(label.as_ptr(), DISPATCH_QUEUE_SERIAL);

            msg_send![cbcentralmanager, initWithDelegate:delegate queue:queue]
        }
    }

    pub fn centralmanager_scanforperipheralswithservices_options(
        cbcentralmanager: id,
        service_uuids: id, /* NSArray<CBUUID *> */
        options: id,       /* NSDictionary<NSString*,id> */
    ) {
        unsafe {
            msg_send![cbcentralmanager, scanForPeripheralsWithServices:service_uuids options:options]
        }
    }

    pub fn centralmanager_stopscan(cbcentralmanager: id) {
        unsafe { msg_send![cbcentralmanager, stopScan] }
    }

    pub fn centralmanager_connectperipheral(
        cbcentralmanager: id,
        peripheral: id, /* CBPeripheral* */
    ) {
        unsafe { msg_send![cbcentralmanager, connectPeripheral:peripheral options:nil] }
    }

    pub fn centralmanager_cancelperipheralconnection(
        cbcentralmanager: id,
        peripheral: id, /* CBPeripheral* */
    ) {
        unsafe { msg_send![cbcentralmanager, cancelPeripheralConnection: peripheral] }
    }

    // CBManager
    pub fn manager_authorization() -> CBManagerAuthorization {
        unsafe { msg_send![class!(CBManager), authorization] }
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

    pub fn peer_identifier(cbpeer: id) -> id /* NSUUID* */ {
        unsafe { msg_send![cbpeer, identifier] }
    }

    // CBPeripheral : CBPeer

    pub fn peripheral_name(cbperipheral: id) -> id /* NSString* */ {
        unsafe { msg_send![cbperipheral, name] }
    }

    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    #[repr(i64)]
    pub enum CBPeripheralState {
        Disonnected = 0,
        Connecting = 1,
        Connected = 2,
        Disconnecting = 3,
    }

    pub fn peripheral_state(cbperipheral: id) -> CBPeripheralState {
        unsafe { msg_send![cbperipheral, state] }
    }

    pub fn peripheral_setdelegate(cbperipheral: id, delegate: id /* CBPeripheralDelegate* */) {
        unsafe { msg_send![cbperipheral, setDelegate: delegate] }
    }

    pub fn peripheral_discoverservices(cbperipheral: id) {
        unsafe { msg_send![cbperipheral, discoverServices: nil] }
    }

    pub fn peripheral_discoverincludedservicesforservice(
        cbperipheral: id,
        service: id, /* CBService* */
    ) {
        unsafe { msg_send![cbperipheral, discoverIncludedServices:nil forService:service] }
    }

    pub fn peripheral_services(cbperipheral: id) -> id /* NSArray<CBService*>* */ {
        unsafe { msg_send![cbperipheral, services] }
    }

    pub fn peripheral_discovercharacteristicsforservice(
        cbperipheral: id,
        service: id, /* CBService* */
    ) {
        unsafe { msg_send![cbperipheral, discoverCharacteristics:nil forService:service] }
    }

    pub fn peripheral_readvalue_forcharacteristic(
        cbperipheral: id,
        characteristic: id, /* CBCharacteristic* */
    ) {
        unsafe { msg_send![cbperipheral, readValueForCharacteristic: characteristic] }
    }

    pub fn peripheral_writevalue_forcharacteristic(
        cbperipheral: id,
        value: id,          /* NSData* */
        characteristic: id, /* CBCharacteristic* */
        write_type: usize,
    ) {
        unsafe {
            msg_send![cbperipheral, writeValue:value forCharacteristic:characteristic type:write_type]
            // CBCharacteristicWriteWithResponse from CBPeripheral.h
        }
    }

    pub fn peripheral_setnotifyvalue_forcharacteristic(
        cbperipheral: id,
        value: BOOL,
        characteristic: id, /* CBCharacteristic* */
    ) {
        unsafe { msg_send![cbperipheral, setNotifyValue:value forCharacteristic:characteristic] }
    }

    pub fn peripheral_discoverdescriptorsforcharacteristic(
        cbperipheral: id,
        characteristic: id, /* CBCharacteristic* */
    ) {
        unsafe {
            msg_send![
                cbperipheral,
                discoverDescriptorsForCharacteristic: characteristic
            ]
        }
    }

    pub fn peripheral_readvalue_fordescriptor(
        cbperipheral: id,
        descriptor: id, /* CBDescriptor * */
    ) {
        unsafe { msg_send![cbperipheral, readValueForDescriptor: descriptor] }
    }

    pub fn peripheral_writevalue_fordescriptor(
        cbperipheral: id,
        value: id,      /* NSData* */
        descriptor: id, /* CBCharacteristic* */
    ) {
        unsafe { msg_send![cbperipheral, writeValue:value forDescriptor:descriptor] }
    }

    // CBPeripheralState = NSInteger from CBPeripheral.h

    pub const PERIPHERALSTATE_CONNECTED: c_int = 2; // CBPeripheralStateConnected

    // CBAttribute

    pub fn attribute_uuid(cbattribute: id) -> id /* CBUUID* */ {
        unsafe { msg_send![cbattribute, UUID] }
    }

    // CBService : CBAttribute

    pub fn service_isprimary(cbservice: id) -> BOOL {
        unsafe { msg_send![cbservice, isPrimary] }
    }

    pub fn service_includedservices(cbservice: id) -> id /* NSArray<CBService*>* */ {
        unsafe { msg_send![cbservice, includedServices] }
    }

    pub fn service_characteristics(cbservice: id) -> id /* NSArray<CBCharacteristic*>* */ {
        unsafe { msg_send![cbservice, characteristics] }
    }

    // CBCharacteristic : CBAttribute

    pub fn characteristic_isnotifying(cbcharacteristic: id) -> BOOL {
        unsafe { msg_send![cbcharacteristic, isNotifying] }
    }

    pub fn characteristic_value(cbcharacteristic: id) -> id /* NSData* */ {
        unsafe { msg_send![cbcharacteristic, value] }
    }

    pub fn characteristic_properties(cbcharacteristic: id) -> c_uint {
        unsafe { msg_send![cbcharacteristic, properties] }
    }

    pub fn characteristic_service(cbcharacteristic: id) -> id /* CBService* */ {
        unsafe { msg_send![cbcharacteristic, service] }
    }

    pub fn characteristic_descriptors(cbcharacteristic: id) -> id /* NSArray<CBDescriptor*>* */ {
        unsafe { msg_send![cbcharacteristic, descriptors] }
    }

    // CBDescriptor : CBAttribute

    pub fn descriptor_characteristic(cbdescriptor: id) -> id /* CBCharacteristic* */ {
        unsafe { msg_send![cbdescriptor, characteristic] }
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

    pub fn uuid_uuidstring(cbuuid: id) -> id /* NSString* */ {
        unsafe { msg_send![cbuuid, UUIDString] }
    }

    pub fn uuid_uuidwithstring(s: impl NSString) -> id /* CBUUID */ {
        unsafe { msg_send![class!(CBUUID), UUIDWithString: s] }
    }

    // CBCentralManagerScanOption...Key

    pub use self::link::CBCentralManagerScanOptionAllowDuplicatesKey as CENTRALMANAGERSCANOPTIONALLOWDUPLICATESKEY;

    // CBAdvertisementData...Key

    pub use self::link::CBAdvertisementDataManufacturerDataKey as ADVERTISEMENT_DATA_MANUFACTURER_DATA_KEY;
    pub use self::link::CBAdvertisementDataServiceDataKey as ADVERTISEMENT_DATA_SERVICE_DATA_KEY;
    pub use self::link::CBAdvertisementDataServiceUUIDsKey as ADVERTISEMENT_DATA_SERVICE_UUIDS_KEY;
}
