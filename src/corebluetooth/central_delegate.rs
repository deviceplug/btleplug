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

use super::{
    framework::{cb, nil, ns},
    utils::{
        core_bluetooth::{cbuuid_to_uuid, characteristic_debug, peripheral_debug, service_debug},
        nsdata_to_vec, nsuuid_to_uuid,
    },
};
use futures::channel::mpsc::{self, Receiver, Sender};
use futures::sink::SinkExt;
use libc::{c_char, c_void};
use log::{error, trace};
use objc::{
    declare::ClassDecl,
    rc::StrongPtr,
    runtime::{Class, Object, Protocol, Sel},
};
use objc::{msg_send, sel, sel_impl};
use std::ffi::CStr;
use std::{
    collections::HashMap,
    fmt::{self, Debug, Formatter},
    ops::Deref,
    slice,
    sync::Once,
};
use uuid::Uuid;

pub enum CentralDelegateEvent {
    DidUpdateState,
    DiscoveredPeripheral(StrongPtr),
    // Peripheral UUID, HashMap Service Uuid to StrongPtr
    DiscoveredServices(Uuid, HashMap<Uuid, StrongPtr>),
    ManufacturerData(Uuid, u16, Vec<u8>),
    ServiceData(Uuid, HashMap<Uuid, Vec<u8>>),
    Services(Uuid, Vec<Uuid>),
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

impl Debug for CentralDelegateEvent {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            CentralDelegateEvent::DidUpdateState => f.debug_tuple("DidUpdateState").finish(),
            CentralDelegateEvent::DiscoveredPeripheral(p) => f
                .debug_tuple("CentralDelegateEvent")
                .field(p.deref())
                .finish(),
            CentralDelegateEvent::DiscoveredServices(uuid, services) => f
                .debug_tuple("DiscoveredServices")
                .field(uuid)
                .field(&services.keys().collect::<Vec<_>>())
                .finish(),
            CentralDelegateEvent::DiscoveredCharacteristics(uuid, characteristics) => f
                .debug_tuple("DiscoveredCharacteristics")
                .field(uuid)
                .field(&characteristics.keys().collect::<Vec<_>>())
                .finish(),
            CentralDelegateEvent::ConnectedDevice(uuid) => {
                f.debug_tuple("ConnectedDevice").field(uuid).finish()
            }
            CentralDelegateEvent::DisconnectedDevice(uuid) => {
                f.debug_tuple("DisconnectedDevice").field(uuid).finish()
            }
            CentralDelegateEvent::CharacteristicSubscribed(uuid1, uuid2) => f
                .debug_tuple("CharacteristicSubscribed")
                .field(uuid1)
                .field(uuid2)
                .finish(),
            CentralDelegateEvent::CharacteristicUnsubscribed(uuid1, uuid2) => f
                .debug_tuple("CharacteristicUnsubscribed")
                .field(uuid1)
                .field(uuid2)
                .finish(),
            CentralDelegateEvent::CharacteristicNotified(uuid1, uuid2, vec) => f
                .debug_tuple("CharacteristicNotified")
                .field(uuid1)
                .field(uuid2)
                .field(vec)
                .finish(),
            CentralDelegateEvent::CharacteristicWritten(uuid1, uuid2) => f
                .debug_tuple("CharacteristicWritten")
                .field(uuid1)
                .field(uuid2)
                .finish(),
            CentralDelegateEvent::ManufacturerData(uuid, manufacturer_id, manufacturer_data) => f
                .debug_tuple("ManufacturerData")
                .field(uuid)
                .field(manufacturer_id)
                .field(manufacturer_data)
                .finish(),
            CentralDelegateEvent::ServiceData(uuid, service_data) => f
                .debug_tuple("ServiceData")
                .field(uuid)
                .field(service_data)
                .finish(),
            CentralDelegateEvent::Services(uuid, services) => f
                .debug_tuple("Services")
                .field(uuid)
                .field(services)
                .finish(),
        }
    }
}

pub mod CentralDelegate {
    use std::convert::TryInto;

    use super::*;

    pub fn delegate() -> (*mut Object, Receiver<CentralDelegateEvent>) {
        let (sender, receiver) = mpsc::channel::<CentralDelegateEvent>(256);
        let sendbox = Box::new(sender);
        let delegate = unsafe {
            let mut delegate: *mut Object = msg_send![delegate_class(), alloc];
            delegate = msg_send![
                delegate,
                initWithSender: Box::into_raw(sendbox) as *mut c_void
            ];
            delegate
        };
        (delegate, receiver)
    }

    pub fn delegate_drop_channel(delegate: *mut Object) {
        unsafe {
            let _ = Box::from_raw(*(&*delegate).get_ivar::<*mut c_void>(DELEGATE_SENDER_IVAR)
                as *mut Sender<CentralDelegateEvent>);
        }
    }

    const DELEGATE_SENDER_IVAR: &str = "_sender";

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
            unsafe {
                // Initialization
                decl.add_method(sel!(initWithSender:),
                                delegate_init as extern fn(&mut Object, Sel, *mut c_void) -> *mut Object);

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

    fn delegate_get_sender_clone(delegate: &mut Object) -> Sender<CentralDelegateEvent> {
        unsafe {
            (*(*(&*delegate).get_ivar::<*mut c_void>(DELEGATE_SENDER_IVAR)
                as *mut Sender<CentralDelegateEvent>))
                .clone()
        }
    }

    fn send_delegate_event(delegate: &mut Object, event: CentralDelegateEvent) {
        let mut sender = delegate_get_sender_clone(delegate);
        futures::executor::block_on(async {
            if let Err(e) = sender.send(event).await {
                error!("Error sending delegate event: {}", e);
            }
        });
    }

    extern "C" fn delegate_init(
        delegate: &mut Object,
        _cmd: Sel,
        sender: *mut c_void,
    ) -> *mut Object {
        trace!("delegate_init");
        // TODO Should these maybe be Option<T>, so we can denote when we've
        // dropped? Not quite sure how delegate lifetime works here.
        unsafe {
            trace!("Storing off ivars!");
            delegate.set_ivar(DELEGATE_SENDER_IVAR, sender);
        }
        delegate
    }

    fn get_characteristic_value(characteristic: *mut Object) -> Vec<u8> {
        trace!("Getting data!");
        let value = cb::characteristic_value(characteristic);
        let v = nsdata_to_vec(value);
        trace!("BluetoothGATTCharacteristic::get_value -> {:?}", v);
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
            peripheral_debug(peripheral)
        );
        cb::peripheral_setdelegate(peripheral, delegate);
        cb::peripheral_discoverservices(peripheral);
        let uuid = nsuuid_to_uuid(cb::peer_identifier(peripheral));
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
            peripheral_debug(peripheral)
        );
        let uuid = nsuuid_to_uuid(cb::peer_identifier(peripheral));
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
        adv_data: *mut Object,
        _rssi: *mut Object,
    ) {
        trace!(
            "delegate_centralmanager_diddiscoverperipheral_advertisementdata_rssi {}",
            peripheral_debug(peripheral)
        );

        let held_peripheral;
        unsafe {
            held_peripheral = StrongPtr::retain(peripheral);
        }
        send_delegate_event(
            delegate,
            CentralDelegateEvent::DiscoveredPeripheral(held_peripheral),
        );

        let puuid = nsuuid_to_uuid(cb::peer_identifier(peripheral));

        let manufacturer_data = ns::dictionary_objectforkey(adv_data, unsafe {
            cb::ADVERTISEMENT_DATA_MANUFACTURER_DATA_KEY
        });
        if manufacturer_data != nil {
            // manufacturer_data: NSData
            let length = ns::data_length(manufacturer_data);
            if length >= 2 {
                let bytes = ns::data_bytes(manufacturer_data);
                let v = unsafe { slice::from_raw_parts(bytes, length as usize) };
                let (manufacturer_id, manufacturer_data) = v.split_at(2);

                send_delegate_event(
                    delegate,
                    CentralDelegateEvent::ManufacturerData(
                        puuid,
                        u16::from_le_bytes(manufacturer_id.try_into().unwrap()),
                        Vec::from(manufacturer_data),
                    ),
                );
            }
        }
        let service_data = ns::dictionary_objectforkey(adv_data, unsafe {
            cb::ADVERTISEMENT_DATA_SERVICE_DATA_KEY
        });
        if service_data != nil {
            // service_data: [CBUUID, NSData]
            let uuids = ns::dictionary_allkeys(service_data);
            let mut result = HashMap::new();
            for i in 0..ns::array_count(uuids) {
                let uuid = ns::array_objectatindex(uuids, i);
                let data = ns::dictionary_objectforkey(service_data, uuid);
                let data = nsdata_to_vec(data);
                result.insert(cbuuid_to_uuid(uuid), data);
            }

            send_delegate_event(delegate, CentralDelegateEvent::ServiceData(puuid, result));
        }

        let services = ns::dictionary_objectforkey(adv_data, unsafe {
            cb::ADVERTISEMENT_DATA_SERVICE_UUIDS_KEY
        });
        if services != nil {
            // services: [CBUUID]
            let mut result = Vec::new();
            for i in 0..ns::array_count(services) {
                let uuid = ns::array_objectatindex(services, i);

                result.push(cbuuid_to_uuid(uuid));
            }

            send_delegate_event(delegate, CentralDelegateEvent::Services(puuid, result));
        }
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
            peripheral_debug(peripheral),
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
                let uuid = cbuuid_to_uuid(cb::attribute_uuid(s));
                let held_service;
                unsafe {
                    held_service = StrongPtr::retain(s);
                }
                service_map.insert(uuid, held_service);
            }
            let puuid = nsuuid_to_uuid(cb::peer_identifier(peripheral));
            send_delegate_event(
                delegate,
                CentralDelegateEvent::DiscoveredServices(puuid, service_map),
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
            peripheral_debug(peripheral),
            service_debug(service),
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
            peripheral_debug(peripheral),
            service_debug(service),
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
                let uuid = cbuuid_to_uuid(cb::attribute_uuid(c));
                let held_char;
                unsafe {
                    held_char = StrongPtr::retain(c);
                }
                char_map.insert(uuid, held_char);
            }
            let puuid = nsuuid_to_uuid(cb::peer_identifier(peripheral));
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
            peripheral_debug(peripheral),
            characteristic_debug(characteristic),
            localized_description(error)
        );
        if error == nil {
            let v = get_characteristic_value(characteristic);
            let puuid = nsuuid_to_uuid(cb::peer_identifier(peripheral));
            let characteristic_uuid = cbuuid_to_uuid(cb::attribute_uuid(characteristic));
            send_delegate_event(
                delegate,
                CentralDelegateEvent::CharacteristicNotified(puuid, characteristic_uuid, v),
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
            peripheral_debug(peripheral),
            characteristic_debug(characteristic),
            localized_description(error)
        );
        if error == nil {
            let puuid = nsuuid_to_uuid(cb::peer_identifier(peripheral));
            let characteristic_uuid = cbuuid_to_uuid(cb::attribute_uuid(characteristic));
            send_delegate_event(
                delegate,
                CentralDelegateEvent::CharacteristicWritten(puuid, characteristic_uuid),
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
        let puuid = nsuuid_to_uuid(cb::peer_identifier(peripheral));
        let characteristic_uuid = cbuuid_to_uuid(cb::attribute_uuid(characteristic));
        if cb::characteristic_isnotifying(characteristic) == objc::runtime::YES {
            send_delegate_event(
                delegate,
                CentralDelegateEvent::CharacteristicSubscribed(puuid, characteristic_uuid),
            );
        } else {
            send_delegate_event(
                delegate,
                CentralDelegateEvent::CharacteristicUnsubscribed(puuid, characteristic_uuid),
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
            peripheral_debug(peripheral)
        );
        if error == nil {}
    }
}
