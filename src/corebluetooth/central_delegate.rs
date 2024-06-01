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
        core_bluetooth::{
            cbuuid_to_uuid, characteristic_debug, descriptor_debug, peripheral_debug, service_debug,
        },
        id, nsuuid_to_uuid, StrongPtr,
    },
};
use futures::channel::mpsc::Sender;
use futures::sink::SinkExt;
use log::{error, trace};
use objc2::{declare_class, msg_send_id, rc::Retained, ClassType, DeclaredClass};
use objc2::{mutability, runtime::AnyObject};
use objc2_core_bluetooth::{CBCentralManagerDelegate, CBPeripheralDelegate};
use objc2_foundation::{
    NSArray, NSData, NSDictionary, NSError, NSNumber, NSObject, NSObjectProtocol, NSString,
};
use std::convert::TryInto;
use std::{
    collections::HashMap,
    fmt::{self, Debug, Formatter},
    ops::Deref,
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

declare_class!(
    #[derive(Debug)]
    pub struct CentralDelegate;

    unsafe impl ClassType for CentralDelegate {
        type Super = NSObject;
        type Mutability = mutability::InteriorMutable;
        const NAME: &'static str = "BtlePlugCentralManagerDelegate";
    }

    impl DeclaredClass for CentralDelegate {
        type Ivars = Sender<CentralDelegateEvent>;
    }

    unsafe impl NSObjectProtocol for CentralDelegate {}

    unsafe impl CBCentralManagerDelegate for CentralDelegate {
        #[method(centralManagerDidUpdateState:)]
        fn delegate_centralmanagerdidupdatestate(
            &self,
            _central: id,
        ) {
            trace!("delegate_centralmanagerdidupdatestate");
            self.send_event(CentralDelegateEvent::DidUpdateState);
        }

        // #[method(centralManager:willRestoreState:)]
        // fn delegate_centralmanager_willrestorestate(&self, _central: id, _dict: id) {
        //     trace!("delegate_centralmanager_willrestorestate");
        // }

        #[method(centralManager:didConnectPeripheral:)]
        fn delegate_centralmanager_didconnectperipheral(
            &self,
            _central: id,
            peripheral: id,
        ) {
            trace!(
                "delegate_centralmanager_didconnectperipheral {}",
                peripheral_debug(peripheral)
            );
            cb::peripheral_setdelegate(peripheral, self);
            cb::peripheral_discoverservices(peripheral);
            let peripheral_uuid = nsuuid_to_uuid(&cb::peer_identifier(peripheral));
            self.send_event(
                CentralDelegateEvent::ConnectedDevice { peripheral_uuid },
            );
        }

        #[method(centralManager:didDisconnectPeripheral:error:)]
        fn delegate_centralmanager_diddisconnectperipheral_error(
            &self,
            _central: id,
            peripheral: id,
            _error: id,
        ) {
            trace!(
                "delegate_centralmanager_diddisconnectperipheral_error {}",
                peripheral_debug(peripheral)
            );
            let peripheral_uuid = nsuuid_to_uuid(&cb::peer_identifier(peripheral));
            self.send_event(
                CentralDelegateEvent::DisconnectedDevice { peripheral_uuid },
            );
        }

        #[method(centralManager:didFailToConnectPeripheral:error:)]
        fn delegate_centralmanager_didfailtoconnectperipheral_error(
            &self,
            _central: id,
            peripheral: id,
            error: Option<&NSError>,
        ) {
            trace!("delegate_centralmanager_didfailtoconnectperipheral_error");
            let peripheral_uuid = nsuuid_to_uuid(&cb::peer_identifier(peripheral));
            let error_description = error.map(|error| error.localizedDescription().to_string());
            self.send_event(
                CentralDelegateEvent::ConnectionFailed {
                    peripheral_uuid,
                    error_description,
                },
            );
        }

        #[method(centralManager:didDiscoverPeripheral:advertisementData:RSSI:)]
        fn delegate_centralmanager_diddiscoverperipheral_advertisementdata_rssi(
            &self,
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
            self.send_event(
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

                    self.send_event(
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

                self.send_event(
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

                self.send_event(
                    CentralDelegateEvent::Services {
                        peripheral_uuid,
                        service_uuids,
                        rssi: rssi_value,
                    },
                );
            }
        }
    }

    unsafe impl CBPeripheralDelegate for CentralDelegate {
        #[method(peripheral:didDiscoverServices:)]
        fn delegate_peripheral_diddiscoverservices(
            &self,
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
                self.send_event(
                    CentralDelegateEvent::DiscoveredServices {
                        peripheral_uuid,
                        services: service_map,
                    },
                );
            }
        }

        #[method(peripheral:didDiscoverIncludedServicesForService:error:)]
        fn delegate_peripheral_diddiscoverincludedservicesforservice_error(
            &self,
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

        #[method(peripheral:didDiscoverCharacteristicsForService:error:)]
        fn delegate_peripheral_diddiscovercharacteristicsforservice_error(
            &self,
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
                self.send_event(
                    CentralDelegateEvent::DiscoveredCharacteristics {
                        peripheral_uuid,
                        service_uuid,
                        characteristics,
                    },
                );
            }
        }

        #[method(peripheral:didDiscoverDescriptorsForCharacteristic:error:)]
        fn delegate_peripheral_diddiscoverdescriptorsforcharacteristic_error(
            &self,
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
                self.send_event(
                    CentralDelegateEvent::DiscoveredCharacteristicDescriptors {
                        peripheral_uuid,
                        service_uuid,
                        characteristic_uuid,
                        descriptors,
                    },
                );
            }
        }

        #[method(peripheral:didUpdateValueForCharacteristic:error:)]
        fn delegate_peripheral_didupdatevalueforcharacteristic_error(
            &self,
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
                self.send_event(
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

        #[method(peripheral:didUpdateNotificationStateForCharacteristic:error:)]
        fn delegate_peripheral_didwritevalueforcharacteristic_error(
            &self,
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
                self.send_event(
                    CentralDelegateEvent::CharacteristicWritten {
                        peripheral_uuid: nsuuid_to_uuid(&cb::peer_identifier(peripheral)),
                        service_uuid: cbuuid_to_uuid(cb::attribute_uuid(service)),
                        characteristic_uuid: cbuuid_to_uuid(cb::attribute_uuid(characteristic)),
                    },
                );
            }
        }

        #[method(peripheral:didWriteValueForCharacteristic:error:)]
        fn delegate_peripheral_didupdatenotificationstateforcharacteristic_error(
            &self,
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
                self.send_event(
                    CentralDelegateEvent::CharacteristicSubscribed {
                        peripheral_uuid,
                        service_uuid,
                        characteristic_uuid,
                    },
                );
            } else {
                self.send_event(
                    CentralDelegateEvent::CharacteristicUnsubscribed {
                        peripheral_uuid,
                        service_uuid,
                        characteristic_uuid,
                    },
                );
            }
        }

        #[method(peripheral:didReadRSSI:error:)]
        fn delegate_peripheral_didreadrssi_error(
            &self,
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

        #[method(peripheral:didUpdateValueForDescriptor:error:)]
        fn delegate_peripheral_didupdatevaluefordescriptor_error(
            &self,
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
                self.send_event(
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

        #[method(peripheral:didWriteValueForDescriptor:error:)]
        fn delegate_peripheral_didwritevaluefordescriptor_error(
            &self,
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
                self.send_event(
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
);

impl CentralDelegate {
    pub fn new(sender: Sender<CentralDelegateEvent>) -> Retained<Self> {
        let this = CentralDelegate::alloc().set_ivars(sender);
        unsafe { msg_send_id![super(this), init] }
    }

    fn send_event(&self, event: CentralDelegateEvent) {
        let mut sender = self.ivars().clone();
        futures::executor::block_on(async {
            if let Err(e) = sender.send(event).await {
                error!("Error sending delegate event: {}", e);
            }
        });
    }
}

fn localized_description(error: Option<&NSError>) -> String {
    if let Some(error) = error {
        error.localizedDescription().to_string()
    } else {
        "".to_string()
    }
}

fn get_characteristic_value(characteristic: id) -> Vec<u8> {
    trace!("Getting data!");
    let value = cb::characteristic_value(characteristic);
    let v = value.map(|value| value.bytes().into());
    trace!("BluetoothGATTCharacteristic::get_value -> {:?}", v);
    v.unwrap_or_default()
}
