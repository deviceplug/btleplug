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

use std::error::Error;
use std::sync::{Once};
use crossbeam::crossbeam_channel::{bounded, Receiver, Sender};

use objc::{
    declare::ClassDecl,
    runtime::{Class, Object, Protocol, Sel}
};

use super::{
    framework::{nil, cb, ns},
    utils::{NO_PERIPHERAL_FOUND, CoreBluetoothUtils, NSStringUtils},
};

use libc::c_void;

pub enum CentralDelegateEvent {
    DidUpdateState,
    DiscoveredPeripheral(String, String),
}

pub mod CentralDelegate {
    use super::*;

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
                decl.add_method(sel!(init),
                                delegate_init as extern fn(&mut Object, Sel) -> *mut Object);
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

    extern fn delegate_centralmanagerdidupdatestate(delegate: &mut Object, _cmd: Sel, _central: *mut Object) {
        trace!("delegate_centralmanagerdidupdatestate");
        unsafe {
            let sender = delegate_get_sender_clone(delegate);
            sender.send(CentralDelegateEvent::DidUpdateState);
            trace!("actually sent!");
        }
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
        // let peripherals = delegate_peripherals(delegate);
        let uuid_nsstring = ns::uuid_uuidstring(cb::peer_identifier(peripheral));
        // let mut data = ns::dictionary_objectforkey(peripherals, uuid_nsstring);
        // if data == nil {
        //     data = ns::mutabledictionary();
        //     ns::mutabledictionary_setobject_forkey(peripherals, data, uuid_nsstring);
        // }

        // ns::mutabledictionary_setobject_forkey(data, ns::object_copy(peripheral), nsx::string_from_str(PERIPHERALDATA_PERIPHERALKEY));

        // ns::mutabledictionary_setobject_forkey(data, rssi, nsx::string_from_str(PERIPHERALDATA_RSSIKEY));

        // let cbuuids_nsarray = ns::dictionary_objectforkey(adv_data, unsafe { cb::ADVERTISEMENTDATASERVICEUUIDSKEY });
        // if cbuuids_nsarray != nil {
        //     ns::mutabledictionary_setobject_forkey(data, cbuuids_nsarray, nsx::string_from_str(PERIPHERALDATA_UUIDSKEY));
        // }

        // if ns::dictionary_objectforkey(data, nsx::string_from_str(PERIPHERALDATA_EVENTSKEY)) == nil {
        //     ns::mutabledictionary_setobject_forkey(data, ns::mutabledictionary(), nsx::string_from_str(PERIPHERALDATA_EVENTSKEY));
        // }
        let uuid_str = NSStringUtils::string_to_string(uuid_nsstring);
        let name = NSStringUtils::string_to_string(cb::peripheral_name(peripheral));
        info!("Discovered device: {}", name);
        unsafe {
            let sender = delegate_get_sender_clone(delegate);

            sender.send(CentralDelegateEvent::DiscoveredPeripheral(uuid_str, name));
            trace!("actually sent!");
        }
    }

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
}
