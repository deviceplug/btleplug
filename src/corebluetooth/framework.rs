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

use super::utils::{id, nil};
use objc2::encode::{Encode, Encoding};
use objc2::rc::Id;
use objc2::{class, msg_send};
use objc2_foundation::{NSArray, NSDictionary, NSInteger, NSString, NSUInteger};
use std::os::raw::{c_char, c_void};
use std::ptr;

pub mod ns {
    use objc2::runtime::AnyObject;

    use super::*;

    // NSNumber

    pub fn number_withbool(value: bool) -> id {
        unsafe { msg_send![class!(NSNumber), numberWithBool: value] }
    }

    pub fn number_as_i64(value: id) -> i64 {
        unsafe {
            let i: NSInteger = msg_send![&*value, integerValue];
            i as i64
        }
    }

    // NSArray

    pub fn array_count(nsarray: id /* NSArray* */) -> NSUInteger {
        let nsarray: &NSArray = unsafe { &*nsarray.cast() };
        nsarray.count()
    }

    pub fn array_objectatindex(
        nsarray: id, /* NSArray* */
        index: NSUInteger,
    ) -> *mut AnyObject {
        let nsarray: &NSArray = unsafe { &*nsarray.cast() };
        unsafe { Id::into_raw(nsarray.objectAtIndex(index)).cast() }
    }

    // NSDictionary

    pub fn dictionary_allkeys(nsdict: id /* NSDictionary* */) -> id /* NSArray* */ {
        let nsdict: &NSDictionary = unsafe { &*nsdict.cast() };
        unsafe { Id::into_raw(nsdict.allKeys()).cast() }
    }

    pub fn dictionary_objectforkey(nsdict: id /* NSDictionary* */, key: id) -> id {
        let nsdict: &NSDictionary = unsafe { &*nsdict.cast() };
        match unsafe { nsdict.objectForKey(&*key) } {
            Some(obj) => Id::into_raw(obj).cast(),
            None => ptr::null_mut(),
        }
    }

    // NSMutableDictionary : NSDictionary

    pub fn mutabledictionary() -> id {
        unsafe { msg_send![class!(NSMutableDictionary), dictionaryWithCapacity: 0usize] }
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

    pub fn data_length(nsdata: id /* NSData* */) -> NSUInteger {
        unsafe { msg_send![nsdata, length] }
    }

    pub fn data_bytes(nsdata: id) -> *const u8 {
        let bytes: *const c_void = unsafe { msg_send![nsdata, bytes] };
        bytes.cast()
    }

    // NSUUID

    pub fn uuid_uuidstring(nsuuid: id) -> id /* NSString* */ {
        unsafe {
            let uuidstring: id = msg_send![nsuuid, UUIDString];
            uuidstring
        }
    }

    // NSError

    pub fn error_localizeddescription(nserror: id) -> id /* NSString* */ {
        unsafe {
            let description: id = msg_send![nserror, localizedDescription];
            description
        }
    }
}

pub mod cb {
    use objc2::msg_send_id;
    use objc2::runtime::AnyObject;

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
            pub static CBAdvertisementDataManufacturerDataKey: &'static NSString;
            pub static CBAdvertisementDataServiceDataKey: &'static NSString;
            pub static CBAdvertisementDataServiceUUIDsKey: &'static NSString;

            pub static CBCentralManagerScanOptionAllowDuplicatesKey: &'static NSString;
        }
    }

    // CBCentralManager

    pub fn centralmanager(delegate: id /*CBCentralManagerDelegate* */) -> id /*CBCentralManager* */
    {
        let label = CString::new("CBqueue").unwrap();
        unsafe {
            let cbcentralmanager: id = msg_send![class!(CBCentralManager), alloc];
            let queue = dispatch_queue_create(label.as_ptr(), DISPATCH_QUEUE_SERIAL);
            let queue: id = queue.cast();

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

    unsafe impl Encode for CBManagerAuthorization {
        const ENCODING: Encoding = i64::ENCODING;
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

    unsafe impl Encode for CBPeripheralState {
        const ENCODING: Encoding = i64::ENCODING;
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
        value: bool,
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

    pub const PERIPHERALSTATE_CONNECTED: isize = 2; // CBPeripheralStateConnected

    // CBAttribute

    pub fn attribute_uuid(cbattribute: id) -> id /* CBUUID* */ {
        unsafe { msg_send![cbattribute, UUID] }
    }

    // CBService : CBAttribute

    pub fn service_isprimary(cbservice: id) -> bool {
        unsafe { msg_send![cbservice, isPrimary] }
    }

    pub fn service_includedservices(cbservice: id) -> id /* NSArray<CBService*>* */ {
        unsafe { msg_send![cbservice, includedServices] }
    }

    pub fn service_characteristics(cbservice: id) -> id /* NSArray<CBCharacteristic*>* */ {
        unsafe { msg_send![cbservice, characteristics] }
    }

    // CBCharacteristic : CBAttribute

    pub fn characteristic_isnotifying(cbcharacteristic: id) -> bool {
        unsafe { msg_send![cbcharacteristic, isNotifying] }
    }

    pub fn characteristic_value(cbcharacteristic: id) -> id /* NSData* */ {
        unsafe { msg_send![cbcharacteristic, value] }
    }

    pub fn characteristic_properties(cbcharacteristic: id) -> NSUInteger {
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

    pub const CHARACTERISTICPROPERTY_BROADCAST: usize = 0x01; // CBCharacteristicPropertyBroadcast
    pub const CHARACTERISTICPROPERTY_READ: usize = 0x02; // CBCharacteristicPropertyRead
    pub const CHARACTERISTICPROPERTY_WRITEWITHOUTRESPONSE: usize = 0x04; // CBCharacteristicPropertyWriteWithoutResponse
    pub const CHARACTERISTICPROPERTY_WRITE: usize = 0x08; // CBCharacteristicPropertyWrite
    pub const CHARACTERISTICPROPERTY_NOTIFY: usize = 0x10; // CBCharacteristicPropertyNotify
    pub const CHARACTERISTICPROPERTY_INDICATE: usize = 0x20; // CBCharacteristicPropertyIndicate
    pub const CHARACTERISTICPROPERTY_AUTHENTICATEDSIGNEDWRITES: usize = 0x40; // CBCharacteristicPropertyAuthenticatedSignedWrites

    // CBUUID

    pub fn uuid_uuidstring(cbuuid: id) -> id /* NSString* */ {
        unsafe { msg_send![cbuuid, UUIDString] }
    }

    pub fn uuid_uuidwithstring(s: &NSString) -> Id<AnyObject> /* CBUUID */ {
        unsafe { msg_send_id![class!(CBUUID), UUIDWithString: s] }
    }

    // CBCentralManagerScanOption...Key

    pub use self::link::CBCentralManagerScanOptionAllowDuplicatesKey as CENTRALMANAGERSCANOPTIONALLOWDUPLICATESKEY;

    // CBAdvertisementData...Key

    pub use self::link::CBAdvertisementDataManufacturerDataKey as ADVERTISEMENT_DATA_MANUFACTURER_DATA_KEY;
    pub use self::link::CBAdvertisementDataServiceDataKey as ADVERTISEMENT_DATA_SERVICE_DATA_KEY;
    pub use self::link::CBAdvertisementDataServiceUUIDsKey as ADVERTISEMENT_DATA_SERVICE_UUIDS_KEY;
}
