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

use async_std::{
    sync::{channel, Receiver, Sender},
    task,
};
use std::{collections::HashMap, slice, str::FromStr, sync::Once};

use objc::{
    declare::ClassDecl,
    rc::StrongPtr,
    runtime::{Class, Object, Protocol, Sel},
};

use super::{
    framework::{cb, nil, ns},
    utils::{CoreBluetoothUtils, NSStringUtils},
};

use uuid::Uuid;

use libc::{c_char, c_void};
use std::ffi::CStr;

pub enum CentralDelegateEvent {
    DidUpdateState,
    DiscoveredPeripheral(StrongPtr),
    // Peripheral UUID, HashMap Service Uuid to StrongPtr
    DiscoveredServices(Uuid, HashMap<Uuid, StrongPtr>),
    // DiscoveredIncludedServices(Uuid, HashMap<Uuid, StrongPtr>),
    // Peripheral UUID, HashMap Characteristic Uuid to StrongPtr
    DiscoveredCharacteristics(Uuid, HashMap<Uuid, StrongPtr>),
    ConnectedDevice(Uuid),
    DisconnectedDevice(Uuid),
    CharacteristicSubscribed(Uuid, Uuid),
    CharacteristicUnsubscribed(Uuid, Uuid),
    CharacteristicNotified(Uuid, Uuid, Vec<u8>),
    CharacteristicWritten(Uuid, Uuid),
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
            (*(*(&mut *delegate).get_ivar::<*mut c_void>(DELEGATE_RECEIVER_IVAR)
                as *mut Receiver<CentralDelegateEvent>))
                .clone()
        }
    }

    pub fn delegate_drop_channel(delegate: *mut Object) {
        unsafe {
            let _ = Box::from_raw(
                *(&mut *delegate).get_ivar::<*mut c_void>(DELEGATE_RECEIVER_IVAR)
                    as *mut Receiver<CentralDelegateEvent>,
            );
            let _ = Box::from_raw(
                *(&mut *delegate).get_ivar::<*mut c_void>(DELEGATE_SENDER_IVAR)
                    as *mut Sender<CentralDelegateEvent>,
            );
        }
    }

    const DELEGATE_SENDER_IVAR: &'static str = "_sender";
    const DELEGATE_RECEIVER_IVAR: &'static str = "_receiver";

    fn delegate_class() -> &'static Class {
        trace!("delegate_class");
        static REGISTER_DELEGATE_CLASS: Once = Once::new();
        let mut decl = ClassDecl::new(
            "BtlePlugCentralManagerDelegate",
            Class::get("NSObject").unwrap(),
        )
        .unwrap();

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
                decl.add_method(sel!(peripheral:didUpdateNotificationStateForCharacteristic:error:),
                                delegate_peripheral_didupdatenotificationstateforcharacteristic_error as extern fn(&mut Object, Sel, *mut Object, *mut Object, *mut Object));
                decl.add_method(sel!(peripheral:didWriteValueForCharacteristic:error:),
                                delegate_peripheral_didwritevalueforcharacteristic_error as extern fn(&mut Object, Sel, *mut Object, *mut Object, *mut Object));
                decl.add_method(sel!(peripheral:didReadRSSI:error:),
                                delegate_peripheral_didreadrssi_error as extern fn(&mut Object, Sel, *mut Object, *mut Object, *mut Object));
            }

            decl.register();
        });

        Class::get("BtlePlugCentralManagerDelegate").unwrap()
    }

    fn localized_description(error: *mut Object) -> String {
        if error == nil {
            "".to_string()
        } else {
            unsafe {
                let nsstring: *mut Object = msg_send![error, localizedDescription];
                let c_string: *const c_char = msg_send![nsstring, UTF8String];
                let c_str: &CStr = CStr::from_ptr(c_string);
                let str_slice: &str = c_str.to_str().unwrap();
                str_slice.to_owned()
            }
        }
    }

    ////////////////////////////////////////////////////////////////
    //
    // Utility functions
    //
    ////////////////////////////////////////////////////////////////

    extern "C" fn delegate_get_sender_clone(delegate: &mut Object) -> Sender<CentralDelegateEvent> {
        unsafe {
            (*(*(&mut *delegate).get_ivar::<*mut c_void>(DELEGATE_SENDER_IVAR)
                as *mut Sender<CentralDelegateEvent>))
                .clone()
        }
    }

    extern "C" fn send_delegate_event(delegate: &mut Object, event: CentralDelegateEvent) {
        let sender = delegate_get_sender_clone(delegate);
        task::block_on(async {
            sender.send(event).await;
        });
    }

    extern "C" fn delegate_init(delegate: &mut Object, _cmd: Sel) -> *mut Object {
        trace!("delegate_init");
        let (sender, recv) = channel::<CentralDelegateEvent>(256);
        // TODO Should these maybe be Option<T>, so we can denote when we've
        // dropped? Not quite sure how delegate lifetime works here.
        let sendbox = Box::new(sender);
        let recvbox = Box::new(recv);
        unsafe {
            trace!("Storing off ivars!");
            delegate.set_ivar::<*mut c_void>(
                DELEGATE_SENDER_IVAR,
                Box::into_raw(sendbox) as *mut c_void,
            );
            delegate.set_ivar::<*mut c_void>(
                DELEGATE_RECEIVER_IVAR,
                Box::into_raw(recvbox) as *mut c_void,
            );
        }
        delegate
    }

    extern "C" fn get_characteristic_value(characteristic: *mut Object) -> Vec<u8> {
        info!("Getting data!");
        let value = cb::characteristic_value(characteristic);
        let length = ns::data_length(value);
        if length == 0 {
            info!("data is 0?");
            return vec![];
        }

        let bytes = ns::data_bytes(value);
        let v = unsafe { slice::from_raw_parts(bytes, length as usize).to_vec() };
        info!("BluetoothGATTCharacteristic::get_value -> {:?}", v);
        v
    }

    ////////////////////////////////////////////////////////////////
    //
    // CentralManager Handlers
    //
    ////////////////////////////////////////////////////////////////

    extern "C" fn delegate_centralmanagerdidupdatestate(
        delegate: &mut Object,
        _cmd: Sel,
        _central: *mut Object,
    ) {
        trace!("delegate_centralmanagerdidupdatestate");
        send_delegate_event(delegate, CentralDelegateEvent::DidUpdateState);
    }

    // extern fn delegate_centralmanager_willrestorestate(_delegate: &mut Object, _cmd: Sel, _central: *mut Object, _dict: *mut Object) {
    //     trace!("delegate_centralmanager_willrestorestate");
    // }

    extern "C" fn delegate_centralmanager_didconnectperipheral(
        delegate: &mut Object,
        _cmd: Sel,
        _central: *mut Object,
        peripheral: *mut Object,
    ) {
        trace!(
            "delegate_centralmanager_didconnectperipheral {}",
            CoreBluetoothUtils::peripheral_debug(peripheral)
        );
        cb::peripheral_setdelegate(peripheral, delegate);
        cb::peripheral_discoverservices(peripheral);
        let uuid_nsstring = ns::uuid_uuidstring(cb::peer_identifier(peripheral));
        let uuid = Uuid::from_str(&NSStringUtils::string_to_string(uuid_nsstring)).unwrap();
        send_delegate_event(delegate, CentralDelegateEvent::ConnectedDevice(uuid));
    }

    extern "C" fn delegate_centralmanager_diddisconnectperipheral_error(
        delegate: &mut Object,
        _cmd: Sel,
        _central: *mut Object,
        peripheral: *mut Object,
        _error: *mut Object,
    ) {
        trace!(
            "delegate_centralmanager_diddisconnectperipheral_error {}",
            CoreBluetoothUtils::peripheral_debug(peripheral)
        );
        let uuid_nsstring = ns::uuid_uuidstring(cb::peer_identifier(peripheral));
        let uuid = Uuid::from_str(&NSStringUtils::string_to_string(uuid_nsstring)).unwrap();
        send_delegate_event(delegate, CentralDelegateEvent::DisconnectedDevice(uuid));
    }

    // extern fn delegate_centralmanager_didfailtoconnectperipheral_error(_delegate: &mut Object, _cmd: Sel, _central: *mut Object, _peripheral: *mut Object, _error: *mut Object) {
    //     trace!("delegate_centralmanager_didfailtoconnectperipheral_error");
    // }

    extern "C" fn delegate_centralmanager_diddiscoverperipheral_advertisementdata_rssi(
        delegate: &mut Object,
        _cmd: Sel,
        _central: *mut Object,
        peripheral: *mut Object,
        _adv_data: *mut Object,
        _rssi: *mut Object,
    ) {
        trace!(
            "delegate_centralmanager_diddiscoverperipheral_advertisementdata_rssi {}",
            CoreBluetoothUtils::peripheral_debug(peripheral)
        );

        let held_peripheral;
        unsafe {
            held_peripheral = StrongPtr::retain(peripheral);
        }
        send_delegate_event(
            delegate,
            CentralDelegateEvent::DiscoveredPeripheral(held_peripheral),
        );
    }

    ////////////////////////////////////////////////////////////////
    //
    // Peripheral Handlers
    //
    ////////////////////////////////////////////////////////////////

    extern "C" fn delegate_peripheral_diddiscoverservices(
        delegate: &mut Object,
        _cmd: Sel,
        peripheral: *mut Object,
        error: *mut Object,
    ) {
        trace!(
            "delegate_peripheral_diddiscoverservices {} {}",
            CoreBluetoothUtils::peripheral_debug(peripheral),
            localized_description(error)
        );
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
                let uuid = CoreBluetoothUtils::uuid_to_canonical_uuid_string(cb::attribute_uuid(s));
                let uuid_str = Uuid::from_str(&uuid).unwrap();
                let held_service;
                unsafe {
                    held_service = StrongPtr::retain(s);
                }
                service_map.insert(uuid_str, held_service);
            }
            let puuid_nsstring = ns::uuid_uuidstring(cb::peer_identifier(peripheral));
            let puuid_str = NSStringUtils::string_to_string(puuid_nsstring);
            send_delegate_event(
                delegate,
                CentralDelegateEvent::DiscoveredServices(
                    Uuid::from_str(&puuid_str).unwrap(),
                    service_map,
                ),
            );
        }
    }

    extern "C" fn delegate_peripheral_diddiscoverincludedservicesforservice_error(
        _delegate: &mut Object,
        _cmd: Sel,
        peripheral: *mut Object,
        service: *mut Object,
        error: *mut Object,
    ) {
        trace!(
            "delegate_peripheral_diddiscoverincludedservicesforservice_error {} {} {}",
            CoreBluetoothUtils::peripheral_debug(peripheral),
            CoreBluetoothUtils::service_debug(service),
            localized_description(error)
        );
        if error == nil {
            let includes = cb::service_includedservices(service);
            for i in 0..ns::array_count(includes) {
                let s = ns::array_objectatindex(includes, i);
                cb::peripheral_discovercharacteristicsforservice(peripheral, s);
            }
        }
    }

    extern "C" fn delegate_peripheral_diddiscovercharacteristicsforservice_error(
        delegate: &mut Object,
        _cmd: Sel,
        peripheral: *mut Object,
        service: *mut Object,
        error: *mut Object,
    ) {
        trace!(
            "delegate_peripheral_diddiscovercharacteristicsforservice_error {} {} {}",
            CoreBluetoothUtils::peripheral_debug(peripheral),
            CoreBluetoothUtils::service_debug(service),
            localized_description(error)
        );
        if error == nil {
            let mut char_map = HashMap::new();
            let chars = cb::service_characteristics(service);
            for i in 0..ns::array_count(chars) {
                let c = ns::array_objectatindex(chars, i);
                // TODO Actually implement characteristic descriptor enumeration
                // cb::peripheral_discoverdescriptorsforcharacteristic(peripheral, c);
                // Create the map entry we'll need to export.
                let uuid = CoreBluetoothUtils::uuid_to_canonical_uuid_string(cb::attribute_uuid(c));
                let uuid = Uuid::from_str(&uuid).unwrap();
                let held_char;
                unsafe {
                    held_char = StrongPtr::retain(c);
                }
                char_map.insert(uuid, held_char);
            }
            let puuid_nsstring = ns::uuid_uuidstring(cb::peer_identifier(peripheral));
            let puuid = Uuid::from_str(&NSStringUtils::string_to_string(puuid_nsstring)).unwrap();
            send_delegate_event(
                delegate,
                CentralDelegateEvent::DiscoveredCharacteristics(puuid, char_map),
            );
        }
    }

    extern "C" fn delegate_peripheral_didupdatevalueforcharacteristic_error(
        delegate: &mut Object,
        _cmd: Sel,
        peripheral: *mut Object,
        characteristic: *mut Object,
        error: *mut Object,
    ) {
        trace!(
            "delegate_peripheral_didupdatevalueforcharacteristic_error {} {} {}",
            CoreBluetoothUtils::peripheral_debug(peripheral),
            CoreBluetoothUtils::characteristic_debug(characteristic),
            localized_description(error)
        );
        if error == nil {
            let v = get_characteristic_value(characteristic);
            let puuid_nsstring = ns::uuid_uuidstring(cb::peer_identifier(peripheral));
            let puuid = Uuid::from_str(&NSStringUtils::string_to_string(puuid_nsstring)).unwrap();
            let cuuid_nsstring = cb::uuid_uuidstring(cb::attribute_uuid(characteristic));
            let cuuid = Uuid::from_str(&NSStringUtils::string_to_string(cuuid_nsstring)).unwrap();
            send_delegate_event(
                delegate,
                CentralDelegateEvent::CharacteristicNotified(puuid, cuuid, v),
            );
            // Notify BluetoothGATTCharacteristic::read_value that read was successful.
        }
    }

    extern "C" fn delegate_peripheral_didwritevalueforcharacteristic_error(
        delegate: &mut Object,
        _cmd: Sel,
        peripheral: *mut Object,
        characteristic: *mut Object,
        error: *mut Object,
    ) {
        trace!(
            "delegate_peripheral_didwritevalueforcharacteristic_error {} {} {}",
            CoreBluetoothUtils::peripheral_debug(peripheral),
            CoreBluetoothUtils::characteristic_debug(characteristic),
            localized_description(error)
        );
        if error == nil {
            let puuid_nsstring = ns::uuid_uuidstring(cb::peer_identifier(peripheral));
            let puuid = Uuid::from_str(&NSStringUtils::string_to_string(puuid_nsstring)).unwrap();
            let cuuid_nsstring = cb::uuid_uuidstring(cb::attribute_uuid(characteristic));
            let cuuid = Uuid::from_str(&NSStringUtils::string_to_string(cuuid_nsstring)).unwrap();
            send_delegate_event(
                delegate,
                CentralDelegateEvent::CharacteristicWritten(puuid, cuuid),
            );
        }
    }

    extern "C" fn delegate_peripheral_didupdatenotificationstateforcharacteristic_error(
        delegate: &mut Object,
        _cmd: Sel,
        peripheral: *mut Object,
        characteristic: *mut Object,
        _error: *mut Object,
    ) {
        trace!("delegate_peripheral_didupdatenotificationstateforcharacteristic_error");
        // TODO check for error here
        let puuid_nsstring = ns::uuid_uuidstring(cb::peer_identifier(peripheral));
        let puuid = Uuid::from_str(&NSStringUtils::string_to_string(puuid_nsstring)).unwrap();
        let cuuid_nsstring = cb::uuid_uuidstring(cb::attribute_uuid(characteristic));
        let cuuid = Uuid::from_str(&NSStringUtils::string_to_string(cuuid_nsstring)).unwrap();
        if cb::characteristic_isnotifying(characteristic) == objc::runtime::YES {
            send_delegate_event(
                delegate,
                CentralDelegateEvent::CharacteristicSubscribed(puuid, cuuid),
            );
        } else {
            send_delegate_event(
                delegate,
                CentralDelegateEvent::CharacteristicUnsubscribed(puuid, cuuid),
            );
        }
    }

    // extern fn delegate_peripheral_diddiscoverdescriptorsforcharacteristic_error(_delegate: &mut Object, _cmd: Sel, _peripheral: *mut Object, _characteristic: *mut Object, _error: *mut Object) {
    //     info!("delegate_peripheral_diddiscoverdescriptorsforcharacteristic_error");
    // }

    // extern fn delegate_peripheral_didupdatevaluefordescriptor(_delegate: &mut Object, _cmd: Sel, _peripheral: *mut Object, _descriptor: *mut Object, _error: *mut Object) {
    //     trace!("delegate_peripheral_didupdatevaluefordescriptor");
    // }

    // extern fn delegate_peripheral_didwritevaluefordescriptor_error(_delegate: &mut Object, _cmd: Sel, _peripheral: *mut Object, _descriptor: *mut Object, _error: *mut Object) {
    //     trace!("delegate_peripheral_didwritevaluefordescriptor_error");
    // }

    extern "C" fn delegate_peripheral_didreadrssi_error(
        _delegate: &mut Object,
        _cmd: Sel,
        peripheral: *mut Object,
        _rssi: *mut Object,
        error: *mut Object,
    ) {
        trace!(
            "delegate_peripheral_didreadrssi_error {}",
            CoreBluetoothUtils::peripheral_debug(peripheral)
        );
        if error == nil {}
    }
}
