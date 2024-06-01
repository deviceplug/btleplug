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
    framework::cb,
    utils::{
        core_bluetooth::{cbuuid_to_uuid, characteristic_debug, peripheral_debug, service_debug},
        nsuuid_to_uuid, StrongPtr,
    },
};
use futures::channel::mpsc::{self, Receiver, Sender};
use futures::sink::SinkExt;
use log::{error, trace};
use objc2::runtime::{
    AnyClass as Class, AnyObject as Object, AnyProtocol as Protocol, ClassBuilder, Sel,
};
use objc2::{class, msg_send, sel};
use std::convert::TryInto;
use std::{
    collections::HashMap,
    fmt::{self, Debug, Formatter},
    ops::Deref,
    os::raw::c_void,
    sync::Once,
};
use uuid::Uuid;

pub enum CentralDelegateEvent {
    DidUpdateState,
    DiscoveredPeripheral {
        cbperipheral: StrongPtr,
    },
    DiscoveredServices {
        peripheral_uuid: Uuid,
        /// Service UUID to CBService
        services: HashMap<Uuid, StrongPtr>,
    },
    ManufacturerData {
        peripheral_uuid: Uuid,
        manufacturer_id: u16,
        data: Vec<u8>,
        rssi: i16,
    },
    ServiceData {
        peripheral_uuid: Uuid,
        service_data: HashMap<Uuid, Vec<u8>>,
        rssi: i16,
    },
    Services {
        peripheral_uuid: Uuid,
        service_uuids: Vec<Uuid>,
        rssi: i16,
    },
    // DiscoveredIncludedServices(Uuid, HashMap<Uuid, StrongPtr>),
    DiscoveredCharacteristics {
        peripheral_uuid: Uuid,
        service_uuid: Uuid,
        /// Characteristic UUID to CBCharacteristic
        characteristics: HashMap<Uuid, StrongPtr>,
    },
    DiscoveredCharacteristicDescriptors {
        peripheral_uuid: Uuid,
        service_uuid: Uuid,
        characteristic_uuid: Uuid,
        descriptors: HashMap<Uuid, StrongPtr>,
    },
    ConnectedDevice {
        peripheral_uuid: Uuid,
    },
    ConnectionFailed {
        peripheral_uuid: Uuid,
        error_description: Option<String>,
    },
    DisconnectedDevice {
        peripheral_uuid: Uuid,
    },
    CharacteristicSubscribed {
        peripheral_uuid: Uuid,
        service_uuid: Uuid,
        characteristic_uuid: Uuid,
    },
    CharacteristicUnsubscribed {
        peripheral_uuid: Uuid,
        service_uuid: Uuid,
        characteristic_uuid: Uuid,
    },
    CharacteristicNotified {
        peripheral_uuid: Uuid,
        service_uuid: Uuid,
        characteristic_uuid: Uuid,
        data: Vec<u8>,
    },
    CharacteristicWritten {
        peripheral_uuid: Uuid,
        service_uuid: Uuid,
        characteristic_uuid: Uuid,
    },
    DescriptorNotified {
        peripheral_uuid: Uuid,
        service_uuid: Uuid,
        characteristic_uuid: Uuid,
        descriptor_uuid: Uuid,
        data: Vec<u8>,
    },
    DescriptorWritten {
        peripheral_uuid: Uuid,
        service_uuid: Uuid,
        characteristic_uuid: Uuid,
        descriptor_uuid: Uuid,
    },
}

impl Debug for CentralDelegateEvent {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            CentralDelegateEvent::DidUpdateState => f.debug_tuple("DidUpdateState").finish(),
            CentralDelegateEvent::DiscoveredPeripheral { cbperipheral } => f
                .debug_struct("CentralDelegateEvent")
                .field("cbperipheral", cbperipheral.deref())
                .finish(),
            CentralDelegateEvent::DiscoveredServices {
                peripheral_uuid,
                services,
            } => f
                .debug_struct("DiscoveredServices")
                .field("peripheral_uuid", peripheral_uuid)
                .field("services", &services.keys().collect::<Vec<_>>())
                .finish(),
            CentralDelegateEvent::DiscoveredCharacteristics {
                peripheral_uuid,
                service_uuid,
                characteristics,
            } => f
                .debug_struct("DiscoveredCharacteristics")
                .field("peripheral_uuid", peripheral_uuid)
                .field("service_uuid", service_uuid)
                .field(
                    "characteristics",
                    &characteristics.keys().collect::<Vec<_>>(),
                )
                .finish(),
            CentralDelegateEvent::DiscoveredCharacteristicDescriptors {
                peripheral_uuid,
                service_uuid,
                characteristic_uuid,
                descriptors,
            } => f
                .debug_struct("DiscoveredCharacteristicDescriptors")
                .field("peripheral_uuid", peripheral_uuid)
                .field("service_uuid", service_uuid)
                .field("characteristic_uuid", characteristic_uuid)
                .field("descriptors", &descriptors.keys().collect::<Vec<_>>())
                .finish(),
            CentralDelegateEvent::ConnectedDevice { peripheral_uuid } => f
                .debug_struct("ConnectedDevice")
                .field("peripheral_uuid", peripheral_uuid)
                .finish(),
            CentralDelegateEvent::ConnectionFailed {
                peripheral_uuid,
                error_description,
            } => f
                .debug_struct("ConnectionFailed")
                .field("peripheral_uuid", peripheral_uuid)
                .field("error_description", error_description)
                .finish(),
            CentralDelegateEvent::DisconnectedDevice { peripheral_uuid } => f
                .debug_struct("DisconnectedDevice")
                .field("peripheral_uuid", peripheral_uuid)
                .finish(),
            CentralDelegateEvent::CharacteristicSubscribed {
                peripheral_uuid,
                service_uuid,
                characteristic_uuid,
            } => f
                .debug_struct("CharacteristicSubscribed")
                .field("peripheral_uuid", peripheral_uuid)
                .field("service_uuid", service_uuid)
                .field("characteristic_uuid", characteristic_uuid)
                .finish(),
            CentralDelegateEvent::CharacteristicUnsubscribed {
                peripheral_uuid,
                service_uuid,
                characteristic_uuid,
            } => f
                .debug_struct("CharacteristicUnsubscribed")
                .field("peripheral_uuid", peripheral_uuid)
                .field("service_uuid", service_uuid)
                .field("characteristic_uuid", characteristic_uuid)
                .finish(),
            CentralDelegateEvent::CharacteristicNotified {
                peripheral_uuid,
                service_uuid,
                characteristic_uuid,
                data,
            } => f
                .debug_struct("CharacteristicNotified")
                .field("peripheral_uuid", peripheral_uuid)
                .field("service_uuid", service_uuid)
                .field("characteristic_uuid", characteristic_uuid)
                .field("data", data)
                .finish(),
            CentralDelegateEvent::CharacteristicWritten {
                peripheral_uuid,
                service_uuid,
                characteristic_uuid,
            } => f
                .debug_struct("CharacteristicWritten")
                .field("service_uuid", service_uuid)
                .field("peripheral_uuid", peripheral_uuid)
                .field("characteristic_uuid", characteristic_uuid)
                .finish(),
            CentralDelegateEvent::ManufacturerData {
                peripheral_uuid,
                manufacturer_id,
                data,
                rssi,
            } => f
                .debug_struct("ManufacturerData")
                .field("peripheral_uuid", peripheral_uuid)
                .field("manufacturer_id", manufacturer_id)
                .field("data", data)
                .field("rssi", rssi)
                .finish(),
            CentralDelegateEvent::ServiceData {
                peripheral_uuid,
                service_data,
                rssi,
            } => f
                .debug_struct("ServiceData")
                .field("peripheral_uuid", peripheral_uuid)
                .field("service_data", service_data)
                .field("rssi", rssi)
                .finish(),
            CentralDelegateEvent::Services {
                peripheral_uuid,
                service_uuids,
                rssi,
            } => f
                .debug_struct("Services")
                .field("peripheral_uuid", peripheral_uuid)
                .field("service_uuids", service_uuids)
                .field("rssi", rssi)
                .finish(),
            CentralDelegateEvent::DescriptorNotified {
                peripheral_uuid,
                service_uuid,
                characteristic_uuid,
                descriptor_uuid,
                data,
            } => f
                .debug_struct("DescriptorNotified")
                .field("peripheral_uuid", peripheral_uuid)
                .field("service_uuid", service_uuid)
                .field("characteristic_uuid", characteristic_uuid)
                .field("descriptor_uuid", descriptor_uuid)
                .field("data", data)
                .finish(),
            CentralDelegateEvent::DescriptorWritten {
                peripheral_uuid,
                service_uuid,
                characteristic_uuid,
                descriptor_uuid,
            } => f
                .debug_struct("DescriptorWritten")
                .field("service_uuid", service_uuid)
                .field("peripheral_uuid", peripheral_uuid)
                .field("characteristic_uuid", characteristic_uuid)
                .field("descriptor_uuid", descriptor_uuid)
                .finish(),
        }
    }
}

pub mod CentralDelegate {
    use objc2::runtime::AnyObject;
    use objc2_foundation::{NSArray, NSData, NSDictionary, NSError, NSNumber, NSObject, NSString};

    use crate::corebluetooth::utils::{core_bluetooth::descriptor_debug, id};

    use super::*;

    pub fn delegate() -> (id, Receiver<CentralDelegateEvent>) {
        let (sender, receiver) = mpsc::channel::<CentralDelegateEvent>(256);
        let sendbox = Box::new(sender);
        let delegate = unsafe {
            let mut delegate: id = msg_send![delegate_class(), alloc];
            delegate = msg_send![
                delegate,
                initWithSender: Box::into_raw(sendbox) as *mut c_void
            ];
            delegate
        };
        (delegate, receiver)
    }

    pub fn delegate_drop_channel(delegate: id) {
        unsafe {
            let _ = Box::from_raw(*(&*delegate).get_ivar::<*mut c_void>(DELEGATE_SENDER_IVAR)
                as *mut Sender<CentralDelegateEvent>);
        }
    }

    const DELEGATE_SENDER_IVAR: &str = "_sender";

    fn delegate_class() -> &'static Class {
        trace!("delegate_class");
        static REGISTER_DELEGATE_CLASS: Once = Once::new();
        REGISTER_DELEGATE_CLASS.call_once(|| {
            let mut decl = ClassBuilder::new("BtlePlugCentralManagerDelegate", class!(NSObject)).unwrap();
            decl.add_protocol(Protocol::get("CBCentralManagerDelegate").unwrap());

            decl.add_ivar::<*mut c_void>(DELEGATE_SENDER_IVAR); /* crossbeam_channel::Sender<DelegateMessage>* */
            unsafe {
                // Initialization
                decl.add_method(sel!(initWithSender:),
                                delegate_init as extern fn(_, _, _) -> _);

                // CentralManager Events
                decl.add_method(sel!(centralManagerDidUpdateState:),
                                delegate_centralmanagerdidupdatestate as extern fn(_, _, _));
                // decl.add_method(sel!(centralManager:willRestoreState:),
                //                 delegate_centralmanager_willrestorestate as extern fn(_, _, _, _));
                decl.add_method(sel!(centralManager:didConnectPeripheral:),
                                delegate_centralmanager_didconnectperipheral as extern fn(_, _, _, _));
                decl.add_method(sel!(centralManager:didDisconnectPeripheral:error:),
                                delegate_centralmanager_diddisconnectperipheral_error as extern fn(_, _, _, _, _));
                decl.add_method(sel!(centralManager:didFailToConnectPeripheral:error:),
                                delegate_centralmanager_didfailtoconnectperipheral_error as extern fn(_, _, _, _, _));
                decl.add_method(sel!(centralManager:didDiscoverPeripheral:advertisementData:RSSI:),
                                delegate_centralmanager_diddiscoverperipheral_advertisementdata_rssi as extern fn(_, _, _, _, _, _));

                // Peripheral events
                decl.add_method(sel!(peripheral:didDiscoverServices:),
                                delegate_peripheral_diddiscoverservices as extern fn(_, _, _, _));
                decl.add_method(sel!(peripheral:didDiscoverIncludedServicesForService:error:),
                                delegate_peripheral_diddiscoverincludedservicesforservice_error as extern fn(_, _, _, _, _));
                decl.add_method(sel!(peripheral:didDiscoverCharacteristicsForService:error:),
                                delegate_peripheral_diddiscovercharacteristicsforservice_error as extern fn(_, _, _, _, _));
                decl.add_method(sel!(peripheral:didDiscoverDescriptorsForCharacteristic:error:),
                                delegate_peripheral_diddiscoverdescriptorsforcharacteristic_error as extern fn(_, _, _, _, _));
                decl.add_method(sel!(peripheral:didUpdateValueForCharacteristic:error:),
                                delegate_peripheral_didupdatevalueforcharacteristic_error as extern fn(_, _, _, _, _));
                decl.add_method(sel!(peripheral:didUpdateNotificationStateForCharacteristic:error:),
                                delegate_peripheral_didupdatenotificationstateforcharacteristic_error as extern fn(_, _, _, _, _));
                decl.add_method(sel!(peripheral:didWriteValueForCharacteristic:error:),
                                delegate_peripheral_didwritevalueforcharacteristic_error as extern fn(_, _, _, _, _));
                decl.add_method(sel!(peripheral:didReadRSSI:error:),
                                delegate_peripheral_didreadrssi_error as extern fn(_, _, _, _, _));
                decl.add_method(sel!(peripheral:didUpdateValueForDescriptor:error:),
                                delegate_peripheral_didupdatevaluefordescriptor_error as extern fn(_, _, _, _, _));
                decl.add_method(sel!(peripheral:didWriteValueForDescriptor:error:),
                                delegate_peripheral_didwritevaluefordescriptor_error as extern fn(_, _, _, _, _));
            }

            decl.register();
        });

        class!(BtlePlugCentralManagerDelegate)
    }

    fn localized_description(error: Option<&NSError>) -> String {
        if let Some(error) = error {
            error.localizedDescription().to_string()
        } else {
            "".to_string()
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

    pub mod methods {
        use super::*;

        pub extern "C" fn delegate_init(
            delegate: &mut Object,
            _cmd: Sel,
            sender: *mut c_void,
        ) -> id {
            trace!("delegate_init");
            // TODO Should these maybe be Option<T>, so we can denote when we've
            // dropped? Not quite sure how delegate lifetime works here.
            unsafe {
                trace!("Storing off ivars!");
                *delegate.get_mut_ivar(DELEGATE_SENDER_IVAR) = sender;
            }
            delegate
        }

        pub fn get_characteristic_value(characteristic: id) -> Vec<u8> {
            trace!("Getting data!");
            let value = cb::characteristic_value(characteristic);
            let v = value.map(|value| value.bytes().into());
            trace!("BluetoothGATTCharacteristic::get_value -> {:?}", v);
            v.unwrap_or_default()
        }

        ////////////////////////////////////////////////////////////////
        //
        // CentralManager Handlers
        //
        ////////////////////////////////////////////////////////////////

        pub extern "C" fn delegate_centralmanagerdidupdatestate(
            delegate: &mut Object,
            _cmd: Sel,
            _central: id,
        ) {
            trace!("delegate_centralmanagerdidupdatestate");
            send_delegate_event(delegate, CentralDelegateEvent::DidUpdateState);
        }

        // extern fn delegate_centralmanager_willrestorestate(_delegate: &mut Object, _cmd: Sel, _central: id, _dict: id) {
        //     trace!("delegate_centralmanager_willrestorestate");
        // }

        pub extern "C" fn delegate_centralmanager_didconnectperipheral(
            delegate: &mut Object,
            _cmd: Sel,
            _central: id,
            peripheral: id,
        ) {
            trace!(
                "delegate_centralmanager_didconnectperipheral {}",
                peripheral_debug(peripheral)
            );
            cb::peripheral_setdelegate(peripheral, delegate);
            cb::peripheral_discoverservices(peripheral);
            let peripheral_uuid = nsuuid_to_uuid(&cb::peer_identifier(peripheral));
            send_delegate_event(
                delegate,
                CentralDelegateEvent::ConnectedDevice { peripheral_uuid },
            );
        }

        pub extern "C" fn delegate_centralmanager_diddisconnectperipheral_error(
            delegate: &mut Object,
            _cmd: Sel,
            _central: id,
            peripheral: id,
            _error: id,
        ) {
            trace!(
                "delegate_centralmanager_diddisconnectperipheral_error {}",
                peripheral_debug(peripheral)
            );
            let peripheral_uuid = nsuuid_to_uuid(&cb::peer_identifier(peripheral));
            send_delegate_event(
                delegate,
                CentralDelegateEvent::DisconnectedDevice { peripheral_uuid },
            );
        }

        pub extern "C" fn delegate_centralmanager_didfailtoconnectperipheral_error(
            delegate: &mut Object,
            _cmd: Sel,
            _central: id,
            peripheral: id,
            error: Option<&NSError>,
        ) {
            trace!("delegate_centralmanager_didfailtoconnectperipheral_error");
            let peripheral_uuid = nsuuid_to_uuid(&cb::peer_identifier(peripheral));
            let error_description = error.map(|error| error.localizedDescription().to_string());
            send_delegate_event(
                delegate,
                CentralDelegateEvent::ConnectionFailed {
                    peripheral_uuid,
                    error_description,
                },
            );
        }

        pub extern "C" fn delegate_centralmanager_diddiscoverperipheral_advertisementdata_rssi(
            delegate: &mut Object,
            _cmd: Sel,
            _central: id,
            peripheral: id,
            adv_data: &NSDictionary<NSString, AnyObject>,
            rssi: &NSNumber,
        ) {
            trace!(
                "delegate_centralmanager_diddiscoverperipheral_advertisementdata_rssi {}",
                peripheral_debug(peripheral)
            );

            let held_peripheral = unsafe { StrongPtr::retain(peripheral as *mut _).unwrap() };
            send_delegate_event(
                delegate,
                CentralDelegateEvent::DiscoveredPeripheral {
                    cbperipheral: held_peripheral,
                },
            );

            let rssi_value = rssi.as_i16();

            let peripheral_uuid = nsuuid_to_uuid(&cb::peer_identifier(peripheral));

            let manufacturer_data =
                adv_data.get(unsafe { cb::ADVERTISEMENT_DATA_MANUFACTURER_DATA_KEY });
            if let Some(manufacturer_data) = manufacturer_data {
                // SAFETY: manufacturer_data is `NSData`
                let manufacturer_data: *const AnyObject = manufacturer_data;
                let manufacturer_data: *const NSData = manufacturer_data.cast();
                let manufacturer_data = unsafe { &*manufacturer_data };

                if manufacturer_data.len() >= 2 {
                    let (manufacturer_id, manufacturer_data) =
                        manufacturer_data.bytes().split_at(2);

                    send_delegate_event(
                        delegate,
                        CentralDelegateEvent::ManufacturerData {
                            peripheral_uuid,
                            manufacturer_id: u16::from_le_bytes(
                                manufacturer_id.try_into().unwrap(),
                            ),
                            data: Vec::from(manufacturer_data),
                            rssi: rssi_value,
                        },
                    );
                }
            }

            let service_data = adv_data.get(unsafe { cb::ADVERTISEMENT_DATA_SERVICE_DATA_KEY });
            if let Some(service_data) = service_data {
                // SAFETY: service_data is `NSDictionary<CBUUID, NSData>`
                let service_data: *const AnyObject = service_data;
                let service_data: *const NSDictionary<NSObject, NSData> = service_data.cast();
                let service_data = unsafe { &*service_data };

                let mut result = HashMap::new();
                for uuid in service_data.keys() {
                    let data = &service_data[uuid];
                    result.insert(cbuuid_to_uuid(&**uuid), data.bytes().to_vec());
                }

                send_delegate_event(
                    delegate,
                    CentralDelegateEvent::ServiceData {
                        peripheral_uuid,
                        service_data: result,
                        rssi: rssi_value,
                    },
                );
            }

            let services = adv_data.get(unsafe { cb::ADVERTISEMENT_DATA_SERVICE_UUIDS_KEY });
            if let Some(services) = services {
                // SAFETY: services is `NSArray<CBUUID>`
                let services: *const AnyObject = services;
                let services: *const NSArray<NSObject> = services.cast();
                let services = unsafe { &*services };

                let mut service_uuids = Vec::new();
                for uuid in services {
                    service_uuids.push(cbuuid_to_uuid(&**uuid));
                }

                send_delegate_event(
                    delegate,
                    CentralDelegateEvent::Services {
                        peripheral_uuid,
                        service_uuids,
                        rssi: rssi_value,
                    },
                );
            }
        }

        ////////////////////////////////////////////////////////////////
        //
        // Peripheral Handlers
        //
        ////////////////////////////////////////////////////////////////

        pub extern "C" fn delegate_peripheral_diddiscoverservices(
            delegate: &mut Object,
            _cmd: Sel,
            peripheral: id,
            error: Option<&NSError>,
        ) {
            trace!(
                "delegate_peripheral_diddiscoverservices {} {}",
                peripheral_debug(peripheral),
                localized_description(error)
            );
            if error.is_none() {
                let services = cb::peripheral_services(peripheral).unwrap_or_default();
                let mut service_map = HashMap::new();
                for s in services {
                    // go ahead and ask for characteristics and other services
                    cb::peripheral_discovercharacteristicsforservice(peripheral, &s);
                    cb::peripheral_discoverincludedservicesforservice(peripheral, &s);

                    // Create the map entry we'll need to export.
                    let uuid = cbuuid_to_uuid(cb::attribute_uuid(&*s));
                    service_map.insert(uuid, s);
                }
                let peripheral_uuid = nsuuid_to_uuid(&cb::peer_identifier(peripheral));
                send_delegate_event(
                    delegate,
                    CentralDelegateEvent::DiscoveredServices {
                        peripheral_uuid,
                        services: service_map,
                    },
                );
            }
        }

        pub extern "C" fn delegate_peripheral_diddiscoverincludedservicesforservice_error(
            _delegate: &mut Object,
            _cmd: Sel,
            peripheral: id,
            service: id,
            error: Option<&NSError>,
        ) {
            trace!(
                "delegate_peripheral_diddiscoverincludedservicesforservice_error {} {} {}",
                peripheral_debug(peripheral),
                service_debug(service),
                localized_description(error)
            );
            if error.is_none() {
                let includes = cb::service_includedservices(service).unwrap_or_default();
                for s in includes {
                    cb::peripheral_discovercharacteristicsforservice(peripheral, &s);
                }
            }
        }

        pub extern "C" fn delegate_peripheral_diddiscovercharacteristicsforservice_error(
            delegate: &mut Object,
            _cmd: Sel,
            peripheral: id,
            service: id,
            error: Option<&NSError>,
        ) {
            trace!(
                "delegate_peripheral_diddiscovercharacteristicsforservice_error {} {} {}",
                peripheral_debug(peripheral),
                service_debug(service),
                localized_description(error)
            );
            if error.is_none() {
                let mut characteristics = HashMap::new();
                let chars = cb::service_characteristics(service).unwrap_or_default();
                for c in chars {
                    cb::peripheral_discoverdescriptorsforcharacteristic(peripheral, &c);
                    // Create the map entry we'll need to export.
                    let uuid = cbuuid_to_uuid(cb::attribute_uuid(&*c));
                    characteristics.insert(uuid, c);
                }
                let peripheral_uuid = nsuuid_to_uuid(&cb::peer_identifier(peripheral));
                let service_uuid = cbuuid_to_uuid(cb::attribute_uuid(service));
                send_delegate_event(
                    delegate,
                    CentralDelegateEvent::DiscoveredCharacteristics {
                        peripheral_uuid,
                        service_uuid,
                        characteristics,
                    },
                );
            }
        }

        pub extern "C" fn delegate_peripheral_diddiscoverdescriptorsforcharacteristic_error(
            delegate: &mut Object,
            _cmd: Sel,
            peripheral: id,
            characteristic: id,
            error: Option<&NSError>,
        ) {
            trace!(
                "delegate_peripheral_diddiscoverdescriptorsforcharacteristic_error {} {} {}",
                peripheral_debug(peripheral),
                characteristic_debug(characteristic),
                localized_description(error)
            );
            if error.is_none() {
                let mut descriptors = HashMap::new();
                let descs = cb::characteristic_descriptors(characteristic).unwrap_or_default();
                for d in descs {
                    // Create the map entry we'll need to export.
                    let uuid = cbuuid_to_uuid(cb::attribute_uuid(&*d));
                    descriptors.insert(uuid, d);
                }
                let peripheral_uuid = nsuuid_to_uuid(&cb::peer_identifier(peripheral));
                let service = cb::characteristic_service(characteristic);
                let service_uuid = cbuuid_to_uuid(cb::attribute_uuid(service));
                let characteristic_uuid = cbuuid_to_uuid(cb::attribute_uuid(characteristic));
                send_delegate_event(
                    delegate,
                    CentralDelegateEvent::DiscoveredCharacteristicDescriptors {
                        peripheral_uuid,
                        service_uuid,
                        characteristic_uuid,
                        descriptors,
                    },
                );
            }
        }

        pub extern "C" fn delegate_peripheral_didupdatevalueforcharacteristic_error(
            delegate: &mut Object,
            _cmd: Sel,
            peripheral: id,
            characteristic: id,
            error: Option<&NSError>,
        ) {
            trace!(
                "delegate_peripheral_didupdatevalueforcharacteristic_error {} {} {}",
                peripheral_debug(peripheral),
                characteristic_debug(characteristic),
                localized_description(error)
            );
            if error.is_none() {
                let service = cb::characteristic_service(characteristic);
                send_delegate_event(
                    delegate,
                    CentralDelegateEvent::CharacteristicNotified {
                        peripheral_uuid: nsuuid_to_uuid(&cb::peer_identifier(peripheral)),
                        service_uuid: cbuuid_to_uuid(cb::attribute_uuid(service)),
                        characteristic_uuid: cbuuid_to_uuid(cb::attribute_uuid(characteristic)),
                        data: get_characteristic_value(characteristic),
                    },
                );
                // Notify BluetoothGATTCharacteristic::read_value that read was successful.
            }
        }

        pub extern "C" fn delegate_peripheral_didwritevalueforcharacteristic_error(
            delegate: &mut Object,
            _cmd: Sel,
            peripheral: id,
            characteristic: id,
            error: Option<&NSError>,
        ) {
            trace!(
                "delegate_peripheral_didwritevalueforcharacteristic_error {} {} {}",
                peripheral_debug(peripheral),
                characteristic_debug(characteristic),
                localized_description(error)
            );
            if error.is_none() {
                let service = cb::characteristic_service(characteristic);
                send_delegate_event(
                    delegate,
                    CentralDelegateEvent::CharacteristicWritten {
                        peripheral_uuid: nsuuid_to_uuid(&cb::peer_identifier(peripheral)),
                        service_uuid: cbuuid_to_uuid(cb::attribute_uuid(service)),
                        characteristic_uuid: cbuuid_to_uuid(cb::attribute_uuid(characteristic)),
                    },
                );
            }
        }

        pub extern "C" fn delegate_peripheral_didupdatenotificationstateforcharacteristic_error(
            delegate: &mut Object,
            _cmd: Sel,
            peripheral: id,
            characteristic: id,
            _error: Option<&NSError>,
        ) {
            trace!("delegate_peripheral_didupdatenotificationstateforcharacteristic_error");
            // TODO check for error here
            let peripheral_uuid = nsuuid_to_uuid(&cb::peer_identifier(peripheral));
            let service = cb::characteristic_service(characteristic);
            let service_uuid = cbuuid_to_uuid(cb::attribute_uuid(service));
            let characteristic_uuid = cbuuid_to_uuid(cb::attribute_uuid(characteristic));
            if cb::characteristic_isnotifying(characteristic) {
                send_delegate_event(
                    delegate,
                    CentralDelegateEvent::CharacteristicSubscribed {
                        peripheral_uuid,
                        service_uuid,
                        characteristic_uuid,
                    },
                );
            } else {
                send_delegate_event(
                    delegate,
                    CentralDelegateEvent::CharacteristicUnsubscribed {
                        peripheral_uuid,
                        service_uuid,
                        characteristic_uuid,
                    },
                );
            }
        }

        pub extern "C" fn delegate_peripheral_didreadrssi_error(
            _delegate: &mut Object,
            _cmd: Sel,
            peripheral: id,
            _rssi: id,
            error: Option<&NSError>,
        ) {
            trace!(
                "delegate_peripheral_didreadrssi_error {}",
                peripheral_debug(peripheral)
            );
            if error.is_none() {}
        }

        pub extern "C" fn delegate_peripheral_didupdatevaluefordescriptor_error(
            delegate: &mut Object,
            _cmd: Sel,
            peripheral: id,
            descriptor: id,
            error: Option<&NSError>,
        ) {
            trace!(
                "delegate_peripheral_didupdatevaluefordescriptor_error {} {} {}",
                peripheral_debug(peripheral),
                descriptor_debug(descriptor),
                localized_description(error)
            );
            if error.is_none() {
                let characteristic = cb::descriptor_characteristic(descriptor);
                let service = cb::characteristic_service(characteristic);
                send_delegate_event(
                    delegate,
                    CentralDelegateEvent::DescriptorNotified {
                        peripheral_uuid: nsuuid_to_uuid(&cb::peer_identifier(peripheral)),
                        service_uuid: cbuuid_to_uuid(cb::attribute_uuid(service)),
                        characteristic_uuid: cbuuid_to_uuid(cb::attribute_uuid(characteristic)),
                        descriptor_uuid: cbuuid_to_uuid(cb::attribute_uuid(descriptor)),
                        data: get_characteristic_value(characteristic),
                    },
                );
                // Notify BluetoothGATTCharacteristic::read_value that read was successful.
            }
        }

        pub extern "C" fn delegate_peripheral_didwritevaluefordescriptor_error(
            delegate: &mut Object,
            _cmd: Sel,
            peripheral: id,
            descriptor: id,
            error: Option<&NSError>,
        ) {
            trace!(
                "delegate_peripheral_didwritevaluefordescriptor_error {} {} {}",
                peripheral_debug(peripheral),
                descriptor_debug(descriptor),
                localized_description(error)
            );
            if error.is_none() {
                let characteristic = cb::descriptor_characteristic(descriptor);
                let service = cb::characteristic_service(characteristic);
                send_delegate_event(
                    delegate,
                    CentralDelegateEvent::DescriptorWritten {
                        peripheral_uuid: nsuuid_to_uuid(&cb::peer_identifier(peripheral)),
                        service_uuid: cbuuid_to_uuid(cb::attribute_uuid(service)),
                        characteristic_uuid: cbuuid_to_uuid(cb::attribute_uuid(characteristic)),
                        descriptor_uuid: cbuuid_to_uuid(cb::attribute_uuid(descriptor)),
                    },
                );
            }
        }
    }

    pub use self::methods::*;
}
