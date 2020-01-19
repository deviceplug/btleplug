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

use std::{
    sync::Once,
    collections::HashMap,
    str::FromStr
};
use crossbeam::crossbeam_channel::{bounded, Receiver, Sender};

use objc::{
    declare::ClassDecl,
    runtime::{Class, Object, Protocol, Sel},
    rc::StrongPtr
};

use super::{
    framework::{nil, cb, ns},
    utils::{CoreBluetoothUtils, NSStringUtils},
};

use uuid::Uuid;

use libc::c_void;

pub enum CentralDelegateEvent {
    DidUpdateState,
    DiscoveredPeripheral(StrongPtr),
    // Peripheral UUID, HashMap Service Uuid to StrongPtr
    DiscoveredServices(Uuid, HashMap<Uuid, StrongPtr>),
    DiscoveredIncludedServices(Uuid, HashMap<Uuid, StrongPtr>),
    // Peripheral UUID, HashMap Characteristic Uuid to StrongPtr
    DiscoveredCharacteristics(Uuid, HashMap<Uuid, StrongPtr>),
    // TODO Deal with descriptors at some point, but not a huge worry at the moment.
    // DiscoveredDescriptors(String, )
}

pub mod CentralDelegate {
    use super::*;

    pub fn delegate() -> *mut Object {
        unsafe {
            let mut delegate: *mut Object = msg_send![delegate_class(), alloc];
            delegate = msg_send![delegate, init];
            delegate
        }
    }

    pub fn delegate_receiver_clone(delegate: *mut Object) -> Receiver<CentralDelegateEvent> {
        unsafe {
            // Just clone here and return, so we don't have to worry about
            // accidentally screwing up ownership by passing the bare pointer
            // outside.
            (*(*(&mut *delegate).get_ivar::<*mut c_void>(DELEGATE_RECEIVER_IVAR) as *mut Receiver<CentralDelegateEvent>)).clone()
        }
    }

    pub fn delegate_drop_channel(delegate: *mut Object) {
        unsafe {
            let _ = Box::from_raw(*(&mut *delegate).get_ivar::<*mut c_void>(DELEGATE_RECEIVER_IVAR) as *mut Receiver<CentralDelegateEvent>);
            let _ = Box::from_raw(*(&mut *delegate).get_ivar::<*mut c_void>(DELEGATE_SENDER_IVAR) as *mut Sender<CentralDelegateEvent>);
        }
    }

    const DELEGATE_SENDER_IVAR: &'static str = "_sender";
    const DELEGATE_RECEIVER_IVAR: &'static str = "_receiver";

    fn delegate_class() -> &'static Class {
        trace!("delegate_class");
        static REGISTER_DELEGATE_CLASS: Once = Once::new();
        let mut decl = ClassDecl::new("BtlePlugCentralManagerDelegate", Class::get("NSObject").unwrap()).unwrap();

        REGISTER_DELEGATE_CLASS.call_once(|| {
            decl.add_protocol(Protocol::get("CBCentralManagerDelegate").unwrap());

            decl.add_ivar::<*mut c_void>(DELEGATE_SENDER_IVAR); /* crossbeam_channel::Sender<DelegateMessage>* */
            decl.add_ivar::<*mut c_void>(DELEGATE_RECEIVER_IVAR); /* crossbeam_channel::Receiver<DelegateMessage>* */
            unsafe {
                // Initialization
                decl.add_method(sel!(init),
                                delegate_init as extern fn(&mut Object, Sel) -> *mut Object);

                // CentralManager Events
                decl.add_method(sel!(centralManagerDidUpdateState:),
                                delegate_centralmanagerdidupdatestate as extern fn(&mut Object, Sel, *mut Object));
                // decl.add_method(sel!(centralManager:willRestoreState:),
                //                 delegate_centralmanager_willrestorestate as extern fn(&mut Object, Sel, *mut Object, *mut Object));
                decl.add_method(sel!(centralManager:didConnectPeripheral:),
                                delegate_centralmanager_didconnectperipheral as extern fn(&mut Object, Sel, *mut Object, *mut Object));
                decl.add_method(sel!(centralManager:didDisconnectPeripheral:error:),
                                delegate_centralmanager_diddisconnectperipheral_error as extern fn(&mut Object, Sel, *mut Object, *mut Object, *mut Object));
                // decl.add_method(sel!(centralManager:didFailToConnectPeripheral:error:),
                //                 delegate_centralmanager_didfailtoconnectperipheral_error as extern fn(&mut Object, Sel, *mut Object, *mut Object, *mut Object));
                decl.add_method(sel!(centralManager:didDiscoverPeripheral:advertisementData:RSSI:),
                                delegate_centralmanager_diddiscoverperipheral_advertisementdata_rssi as extern fn(&mut Object, Sel, *mut Object, *mut Object, *mut Object, *mut Object));

                // Peripheral events
                decl.add_method(sel!(peripheral:didDiscoverServices:),
                                delegate_peripheral_diddiscoverservices as extern fn(&mut Object, Sel, *mut Object, *mut Object));
                decl.add_method(sel!(peripheral:didDiscoverIncludedServicesForService:error:),
                                delegate_peripheral_diddiscoverincludedservicesforservice_error as extern fn(&mut Object, Sel, *mut Object, *mut Object, *mut Object));
                decl.add_method(sel!(peripheral:didDiscoverCharacteristicsForService:error:),
                                delegate_peripheral_diddiscovercharacteristicsforservice_error as extern fn(&mut Object, Sel, *mut Object, *mut Object, *mut Object));
                // TODO Finish implementing this.
                // decl.add_method(sel!(peripheral:didDiscoverDescriptorsForCharacteristic:error:),
                //                 delegate_peripheral_diddiscoverdescriptorsforcharacteristic_error as extern fn(&mut Object, Sel, *mut Object, *mut Object, *mut Object));
                decl.add_method(sel!(peripheral:didUpdateValueForCharacteristic:error:),
                                delegate_peripheral_didupdatevalueforcharacteristic_error as extern fn(&mut Object, Sel, *mut Object, *mut Object, *mut Object));
                decl.add_method(sel!(peripheral:didWriteValueForCharacteristic:error:),
                                delegate_peripheral_didwritevalueforcharacteristic_error as extern fn(&mut Object, Sel, *mut Object, *mut Object, *mut Object));
                decl.add_method(sel!(peripheral:didReadRSSI:error:),
                                delegate_peripheral_didreadrssi_error as extern fn(&mut Object, Sel, *mut Object, *mut Object, *mut Object));
            }

            decl.register();
        });

        Class::get("BtlePlugCentralManagerDelegate").unwrap()
    }

    extern fn delegate_get_sender_clone(delegate: &mut Object) -> Sender<CentralDelegateEvent> {
        unsafe {
            (*(*(&mut *delegate).get_ivar::<*mut c_void>(DELEGATE_SENDER_IVAR) as *mut Sender<CentralDelegateEvent>)).clone()
        }
    }

    extern fn delegate_init(delegate: &mut Object, _cmd: Sel) -> *mut Object {
        trace!("delegate_init");
        let (sender, recv) = bounded::<CentralDelegateEvent>(256);
        // TODO Should these maybe be Option<T>, so we can denote when we've
        // dropped? Not quite sure how delegate lifetime works here.
        let sendbox = Box::new(sender);
        let recvbox = Box::new(recv);
        unsafe {
            trace!("Storing off ivars!");
            delegate.set_ivar::<*mut c_void>(DELEGATE_SENDER_IVAR, Box::into_raw(sendbox) as *mut c_void);
            delegate.set_ivar::<*mut c_void>(DELEGATE_RECEIVER_IVAR, Box::into_raw(recvbox) as *mut c_void);
        }
        delegate
    }

    ////////////////////////////////////////////////////////////////
    //
    // CentralManager Handlers
    //
    ////////////////////////////////////////////////////////////////

    extern fn delegate_centralmanagerdidupdatestate(delegate: &mut Object, _cmd: Sel, _central: *mut Object) {
        trace!("delegate_centralmanagerdidupdatestate");
        let sender = delegate_get_sender_clone(delegate);
        sender.send(CentralDelegateEvent::DidUpdateState);
    }

    // extern fn delegate_centralmanager_willrestorestate(_delegate: &mut Object, _cmd: Sel, _central: *mut Object, _dict: *mut Object) {
    //     trace!("delegate_centralmanager_willrestorestate");
    // }

    extern fn delegate_centralmanager_didconnectperipheral(delegate: &mut Object, _cmd: Sel, _central: *mut Object, peripheral: *mut Object) {
        trace!("delegate_centralmanager_didconnectperipheral {}", CoreBluetoothUtils::peripheral_debug(peripheral));
        cb::peripheral_setdelegate(peripheral, delegate);
        cb::peripheral_discoverservices(peripheral);
    }

    extern fn delegate_centralmanager_diddisconnectperipheral_error(delegate: &mut Object, _cmd: Sel, _central: *mut Object, peripheral: *mut Object, _error: *mut Object) {
        trace!("delegate_centralmanager_diddisconnectperipheral_error {}", CoreBluetoothUtils::peripheral_debug(peripheral));
        // ns::mutabledictionary_removeobjectforkey(delegate_peripherals(delegate), ns::uuid_uuidstring(cb::peer_identifier(peripheral)));
    }

    // extern fn delegate_centralmanager_didfailtoconnectperipheral_error(_delegate: &mut Object, _cmd: Sel, _central: *mut Object, _peripheral: *mut Object, _error: *mut Object) {
    //     trace!("delegate_centralmanager_didfailtoconnectperipheral_error");
    // }

    extern fn delegate_centralmanager_diddiscoverperipheral_advertisementdata_rssi(delegate: &mut Object, _cmd: Sel, _central: *mut Object, peripheral: *mut Object, adv_data: *mut Object, rssi: *mut Object) {
        trace!("delegate_centralmanager_diddiscoverperipheral_advertisementdata_rssi {}", CoreBluetoothUtils::peripheral_debug(peripheral));
        let uuid_nsstring = ns::uuid_uuidstring(cb::peer_identifier(peripheral));
        let uuid_str = NSStringUtils::string_to_string(uuid_nsstring);
        let name = NSStringUtils::string_to_string(cb::peripheral_name(peripheral));
        let sender = delegate_get_sender_clone(delegate);
        let held_peripheral;
        unsafe {
            held_peripheral = StrongPtr::retain(peripheral);
        }
        sender.send(CentralDelegateEvent::DiscoveredPeripheral(held_peripheral));
    }

    ////////////////////////////////////////////////////////////////
    //
    // Peripheral Handlers
    //
    ////////////////////////////////////////////////////////////////

    extern fn delegate_peripheral_diddiscoverservices(delegate: &mut Object, _cmd: Sel, peripheral: *mut Object, error: *mut Object) {
        trace!("delegate_peripheral_diddiscoverservices {} {}", CoreBluetoothUtils::peripheral_debug(peripheral), if error != nil {"error"} else {""});
        if error == nil {
            let services = cb::peripheral_services(peripheral);
            let mut service_map = HashMap::new();
            for i in 0..ns::array_count(services) {
                // get the service out of the services array
                let s = ns::array_objectatindex(services, i);

                // go ahead and ask for characteristics and other services
                cb::peripheral_discovercharacteristicsforservice(peripheral, s);
                cb::peripheral_discoverincludedservicesforservice(peripheral, s);

                // Create the map entry we'll need to export.
                let uuid = cb::uuid_uuidstring(cb::attribute_uuid(s));
                let uuid_str = Uuid::from_str(&NSStringUtils::string_to_string(uuid)).unwrap();
                let held_service;
                unsafe {
                    held_service = StrongPtr::retain(s);
                }
                service_map.insert(uuid_str, held_service);
            }
            let puuid_nsstring = ns::uuid_uuidstring(cb::peer_identifier(peripheral));
            let puuid_str = NSStringUtils::string_to_string(puuid_nsstring);
            let sender = delegate_get_sender_clone(delegate);
            sender.send(CentralDelegateEvent::DiscoveredServices(Uuid::from_str(&puuid_str).unwrap(), service_map));
        }
    }

    extern fn delegate_peripheral_diddiscoverincludedservicesforservice_error(delegate: &mut Object, _cmd: Sel, peripheral: *mut Object, service: *mut Object, error: *mut Object) {
        trace!("delegate_peripheral_diddiscoverincludedservicesforservice_error {} {} {}", CoreBluetoothUtils::peripheral_debug(peripheral), CoreBluetoothUtils::service_debug(service), if error != nil {"error"} else {""});
        if error == nil {
            let includes = cb::service_includedservices(service);
            for i in 0..ns::array_count(includes) {
                let s = ns::array_objectatindex(includes, i);
                cb::peripheral_discovercharacteristicsforservice(peripheral, s);
            }
        }
    }

    extern fn delegate_peripheral_diddiscovercharacteristicsforservice_error(delegate: &mut Object, _cmd: Sel, peripheral: *mut Object, service: *mut Object, error: *mut Object) {
        trace!("delegate_peripheral_diddiscovercharacteristicsforservice_error {} {} {}", CoreBluetoothUtils::peripheral_debug(peripheral), CoreBluetoothUtils::service_debug(service), if error != nil {"error"} else {""});
        if error == nil {
            let mut char_map = HashMap::new();
            let chars = cb::service_characteristics(service);
            for i in 0..ns::array_count(chars) {
                let c = ns::array_objectatindex(chars, i);
                // TODO Actually implement characteristic descriptor enumeration
                // cb::peripheral_discoverdescriptorsforcharacteristic(peripheral, c);
                // Create the map entry we'll need to export.
                let uuid = cb::uuid_uuidstring(cb::attribute_uuid(c));
                let uuid_str = Uuid::from_str(&NSStringUtils::string_to_string(uuid)).unwrap();
                let held_char;
                unsafe {
                    held_char = StrongPtr::retain(c);
                }
                char_map.insert(uuid_str, held_char);
            }
            let puuid_nsstring = ns::uuid_uuidstring(cb::peer_identifier(peripheral));
            let puuid_str = Uuid::from_str(&NSStringUtils::string_to_string(puuid_nsstring)).unwrap();
            let sender = delegate_get_sender_clone(delegate);
            sender.send(CentralDelegateEvent::DiscoveredCharacteristics(puuid_str, char_map));
        }
    }

    extern fn delegate_peripheral_didupdatevalueforcharacteristic_error(delegate: &mut Object, _cmd: Sel, peripheral: *mut Object, characteristic: *mut Object, error: *mut Object) {
        trace!("delegate_peripheral_didupdatevalueforcharacteristic_error {} {} {}", CoreBluetoothUtils::peripheral_debug(peripheral), CoreBluetoothUtils::characteristic_debug(characteristic), if error != nil {"error"} else {""});
        if error == nil {
            // Notify BluetoothGATTCharacteristic::read_value that read was successful.
        }
    }

    extern fn delegate_peripheral_didwritevalueforcharacteristic_error(delegate: &mut Object, _cmd: Sel, peripheral: *mut Object, characteristic: *mut Object, error: *mut Object) {
        trace!("delegate_peripheral_didwritevalueforcharacteristic_error {} {} {}", CoreBluetoothUtils::peripheral_debug(peripheral), CoreBluetoothUtils::characteristic_debug(characteristic), if error != nil {"error"} else {""});
        if error == nil {
        }
    }

    // extern fn delegate_peripheral_didupdatenotificationstateforcharacteristic_error(_delegate: &mut Object, _cmd: Sel, _peripheral: *mut Object, _characteristic: *mut Object, _error: *mut Object) {
    //     trace!("delegate_peripheral_didupdatenotificationstateforcharacteristic_error");
    //     // TODO: this is where notifications should be handled...
    // }

    // extern fn delegate_peripheral_diddiscoverdescriptorsforcharacteristic_error(_delegate: &mut Object, _cmd: Sel, _peripheral: *mut Object, _characteristic: *mut Object, _error: *mut Object) {
    //     info!("delegate_peripheral_diddiscoverdescriptorsforcharacteristic_error");
    // }

    // extern fn delegate_peripheral_didupdatevaluefordescriptor(_delegate: &mut Object, _cmd: Sel, _peripheral: *mut Object, _descriptor: *mut Object, _error: *mut Object) {
    //     trace!("delegate_peripheral_didupdatevaluefordescriptor");
    // }

    // extern fn delegate_peripheral_didwritevaluefordescriptor_error(_delegate: &mut Object, _cmd: Sel, _peripheral: *mut Object, _descriptor: *mut Object, _error: *mut Object) {
    //     trace!("delegate_peripheral_didwritevaluefordescriptor_error");
    // }

    extern fn delegate_peripheral_didreadrssi_error(delegate: &mut Object, _cmd: Sel, peripheral: *mut Object, rssi: *mut Object, error: *mut Object) {
        trace!("delegate_peripheral_didreadrssi_error {}", CoreBluetoothUtils::peripheral_debug(peripheral));
        if error == nil {
        }
    }

}
