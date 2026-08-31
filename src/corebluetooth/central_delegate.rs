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

use super::utils::nsstring_to_string;
use super::utils::{core_bluetooth::cbuuid_to_uuid, nsuuid_to_uuid};
use futures::channel::mpsc::Sender;
use futures::sink::SinkExt;
use log::{error, trace, warn};
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{AnyThread, ClassType, DefinedClass, define_class, msg_send, rc::Retained};
use objc2_core_bluetooth::{
    CBAdvertisementDataLocalNameKey, CBAdvertisementDataManufacturerDataKey,
    CBAdvertisementDataServiceDataKey, CBAdvertisementDataServiceUUIDsKey,
    CBAdvertisementDataTxPowerLevelKey, CBCentralManager, CBCentralManagerDelegate,
    CBCharacteristic, CBDescriptor, CBManagerState, CBPeripheral, CBPeripheralDelegate, CBService,
    CBUUID,
};
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
    DidUpdateState {
        state: CBManagerState,
    },
    DiscoveredPeripheral {
        cbperipheral: Retained<CBPeripheral>,
        advertisement_name: Option<String>,
    },
    DiscoveredServices {
        peripheral_uuid: Uuid,
        services: HashMap<Uuid, Retained<CBService>>,
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
    ServicesModified {
        peripheral_uuid: Uuid,
    },
    // DiscoveredIncludedServices(Uuid, HashMap<Uuid, Retained<CBService>>),
    DiscoveredCharacteristics {
        peripheral_uuid: Uuid,
        service_uuid: Uuid,
        /// Characteristic UUID to CBCharacteristic
        characteristics: HashMap<Uuid, Retained<CBCharacteristic>>,
    },
    DiscoveredCharacteristicDescriptors {
        peripheral_uuid: Uuid,
        service_uuid: Uuid,
        characteristic_uuid: Uuid,
        descriptors: HashMap<Uuid, Retained<CBDescriptor>>,
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
        error: Option<String>,
    },
    CharacteristicUnsubscribed {
        peripheral_uuid: Uuid,
        service_uuid: Uuid,
        characteristic_uuid: Uuid,
        error: Option<String>,
    },
    CharacteristicNotified {
        peripheral_uuid: Uuid,
        service_uuid: Uuid,
        characteristic_uuid: Uuid,
        data: Vec<u8>,
        error: Option<String>,
    },
    CharacteristicWritten {
        peripheral_uuid: Uuid,
        service_uuid: Uuid,
        characteristic_uuid: Uuid,
        error: Option<String>,
    },
    DescriptorNotified {
        peripheral_uuid: Uuid,
        service_uuid: Uuid,
        characteristic_uuid: Uuid,
        descriptor_uuid: Uuid,
        data: Vec<u8>,
        error: Option<String>,
    },
    DescriptorWritten {
        peripheral_uuid: Uuid,
        service_uuid: Uuid,
        characteristic_uuid: Uuid,
        descriptor_uuid: Uuid,
        error: Option<String>,
    },
    TxPowerLevel {
        peripheral_uuid: Uuid,
        tx_power_level: i16,
    },
    DidReadRssi {
        peripheral_uuid: Uuid,
        rssi: i16,
        error: Option<String>,
    },
    ReadyToSendWriteWithoutResponse {
        peripheral_uuid: Uuid,
    },
}

impl Debug for CentralDelegateEvent {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            CentralDelegateEvent::DidUpdateState { state } => f
                .debug_struct("CentralDelegateEvent")
                .field("state", state)
                .finish(),
            CentralDelegateEvent::DiscoveredPeripheral {
                cbperipheral,
                advertisement_name,
            } => f
                .debug_struct("CentralDelegateEvent")
                .field("cbperipheral", cbperipheral.deref())
                .field("advertisement_name", advertisement_name)
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
                ..
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
                ..
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
                ..
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
                ..
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
            CentralDelegateEvent::ServicesModified { peripheral_uuid } => f
                .debug_struct("ServicesModified")
                .field("peripheral_uuid", peripheral_uuid)
                .finish(),
            CentralDelegateEvent::DescriptorNotified {
                peripheral_uuid,
                service_uuid,
                characteristic_uuid,
                descriptor_uuid,
                data,
                ..
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
                ..
            } => f
                .debug_struct("DescriptorWritten")
                .field("service_uuid", service_uuid)
                .field("peripheral_uuid", peripheral_uuid)
                .field("characteristic_uuid", characteristic_uuid)
                .field("descriptor_uuid", descriptor_uuid)
                .finish(),
            CentralDelegateEvent::TxPowerLevel {
                peripheral_uuid,
                tx_power_level,
            } => f
                .debug_struct("TxPowerLevel")
                .field("peripheral_uuid", peripheral_uuid)
                .field("tx_power_level", tx_power_level)
                .finish(),
            CentralDelegateEvent::DidReadRssi {
                peripheral_uuid,
                rssi,
                ..
            } => f
                .debug_struct("DidReadRssi")
                .field("peripheral_uuid", peripheral_uuid)
                .field("rssi", rssi)
                .finish(),
            CentralDelegateEvent::ReadyToSendWriteWithoutResponse { peripheral_uuid } => f
                .debug_struct("ReadyToSendWriteWithoutResponse")
                .field("peripheral_uuid", peripheral_uuid)
                .finish(),
        }
    }
}

define_class!(
    #[derive(Debug)]
    #[unsafe(super(NSObject))]
    #[thread_kind = AnyThread]
    #[ivars = Sender<CentralDelegateEvent>]
    pub struct CentralDelegate;

    unsafe impl NSObjectProtocol for CentralDelegate {}

    unsafe impl CBCentralManagerDelegate for CentralDelegate {
        #[unsafe(method(centralManagerDidUpdateState:))]
        fn delegate_centralmanagerdidupdatestate(&self, central: &CBCentralManager) {
            trace!("delegate_centralmanagerdidupdatestate");
            let state = unsafe { central.state() };
            self.send_event(CentralDelegateEvent::DidUpdateState { state });
        }

        // #[method(centralManager:willRestoreState:)]
        // fn delegate_centralmanager_willrestorestate(&self, _central: &CBCentralManager, _dict: &NSDictionary<NSString, AnyObject>) {
        //     trace!("delegate_centralmanager_willrestorestate");
        // }

        #[unsafe(method(centralManager:didConnectPeripheral:))]
        fn delegate_centralmanager_didconnectperipheral(
            &self,
            _central: &CBCentralManager,
            peripheral: &CBPeripheral,
        ) {
            trace!(
                "delegate_centralmanager_didconnectperipheral {}",
                peripheral_debug(peripheral)
            );
            unsafe { peripheral.setDelegate(Some(ProtocolObject::from_ref(self))) };
            let id = unsafe { peripheral.identifier() };
            let peripheral_uuid = nsuuid_to_uuid(&id);
            self.send_event(CentralDelegateEvent::ConnectedDevice { peripheral_uuid });
        }

        #[unsafe(method(centralManager:didDisconnectPeripheral:error:))]
        fn delegate_centralmanager_diddisconnectperipheral_error(
            &self,
            _central: &CBCentralManager,
            peripheral: &CBPeripheral,
            error: Option<&NSError>,
        ) {
            trace!(
                "delegate_centralmanager_diddisconnectperipheral_error {} (error={:?})",
                peripheral_debug(peripheral),
                error
            );
            let id = unsafe { peripheral.identifier() };
            let peripheral_uuid = nsuuid_to_uuid(&id);
            self.send_event(CentralDelegateEvent::DisconnectedDevice { peripheral_uuid });
        }

        #[unsafe(method(centralManager:didFailToConnectPeripheral:error:))]
        fn delegate_centralmanager_didfailtoconnectperipheral_error(
            &self,
            _central: &CBCentralManager,
            peripheral: &CBPeripheral,
            error: Option<&NSError>,
        ) {
            trace!("delegate_centralmanager_didfailtoconnectperipheral_error");
            let id = unsafe { peripheral.identifier() };
            let peripheral_uuid = nsuuid_to_uuid(&id);
            let error_description = error.map(|error| error.localizedDescription().to_string());
            self.send_event(CentralDelegateEvent::ConnectionFailed {
                peripheral_uuid,
                error_description,
            });
        }

        #[unsafe(method(centralManager:didDiscoverPeripheral:advertisementData:RSSI:))]
        fn delegate_centralmanager_diddiscoverperipheral_advertisementdata_rssi(
            &self,
            _central: &CBCentralManager,
            peripheral: &CBPeripheral,
            adv_data: &NSDictionary<NSString, AnyObject>,
            rssi: &NSNumber,
        ) {
            trace!(
                "delegate_centralmanager_diddiscoverperipheral_advertisementdata_rssi {}",
                peripheral_debug(peripheral)
            );

            let advertisement_name = adv_data
                .objectForKey(unsafe { CBAdvertisementDataLocalNameKey })
                .and_then(|name| name.downcast::<NSString>().ok())
                .and_then(|name| unsafe { nsstring_to_string(&*name as *const NSString) });

            self.send_event(CentralDelegateEvent::DiscoveredPeripheral {
                cbperipheral: unsafe { Retained::retain(peripheral as *const _ as *mut _) }.unwrap(),
                advertisement_name,
            });

            let rssi_value = rssi.as_i16();

            let id = unsafe { peripheral.identifier() };
            let peripheral_uuid = nsuuid_to_uuid(&id);

            let manufacturer_data = adv_data.objectForKey(unsafe { CBAdvertisementDataManufacturerDataKey });
            if let Some(manufacturer_data) = manufacturer_data {
                // SAFETY: manufacturer_data is `NSData`
                let manufacturer_data = manufacturer_data.downcast::<NSData>().unwrap();

                if manufacturer_data.len() >= 2 {
                    let manufacturer_data_vec = manufacturer_data.to_vec();
                    let (manufacturer_id, manufacturer_data) = manufacturer_data_vec.split_at(2);

                    self.send_event(CentralDelegateEvent::ManufacturerData {
                        peripheral_uuid,
                        manufacturer_id: u16::from_le_bytes(manufacturer_id.try_into().unwrap()),
                        data: Vec::from(manufacturer_data),
                        rssi: rssi_value,
                    });
                }
            }

            let service_data = adv_data.objectForKey(unsafe { CBAdvertisementDataServiceDataKey });
            if let Some(service_data) = service_data {
                // SAFETY: service_data is `NSDictionary<CBUUID, NSData>`
                let service_data: Retained<NSDictionary<CBUUID, NSData>> = unsafe { Retained::cast_unchecked(service_data) };

                let mut result = HashMap::new();
                for uuid in service_data.keys() {
                    let data = service_data.objectForKey(&uuid).unwrap();
                    result.insert(cbuuid_to_uuid(&uuid), data.to_vec());
                }

                self.send_event(CentralDelegateEvent::ServiceData {
                    peripheral_uuid,
                    service_data: result,
                    rssi: rssi_value,
                });
            }

            let services = adv_data.objectForKey(unsafe { CBAdvertisementDataServiceUUIDsKey });
            if let Some(services) = services {
                // SAFETY: services is `NSArray<CBUUID>`
                let services: Retained<NSArray<CBUUID>> = unsafe { Retained::cast_unchecked(services) };

                let mut service_uuids = Vec::new();
                for uuid in services {
                    service_uuids.push(cbuuid_to_uuid(&uuid));
                }

                self.send_event(CentralDelegateEvent::Services {
                    peripheral_uuid,
                    service_uuids,
                    rssi: rssi_value,
                });
            }

            let tx_power_level = adv_data
                .objectForKey(unsafe { CBAdvertisementDataTxPowerLevelKey })
                .map(|val| {
                    val.downcast::<NSNumber>().unwrap().as_i16()
                });

            if let Some(tx_power_level) = tx_power_level {
                self.send_event(CentralDelegateEvent::TxPowerLevel {
                    peripheral_uuid,
                    tx_power_level,
                });
            }
        }
    }

    unsafe impl CBPeripheralDelegate for CentralDelegate {
        #[unsafe(method(peripheral:didDiscoverServices:))]
        fn delegate_peripheral_diddiscoverservices(
            &self,
            peripheral: &CBPeripheral,
            error: Option<&NSError>,
        ) {
            trace!(
                "delegate_peripheral_diddiscoverservices {} {}",
                peripheral_debug(peripheral),
                localized_description(error)
            );
            if error.is_none() {
                let services = unsafe { peripheral.services() }.unwrap_or_default();
                let mut service_map = HashMap::new();
                for s in services {
                    // go ahead and ask for characteristics and other services
                    unsafe {
                        peripheral.discoverCharacteristics_forService(None, &s);
                        peripheral.discoverIncludedServices_forService(None, &s);
                    }

                    // Create the map entry we'll need to export.
                    let raw_uuid = unsafe { s.UUID() };
                    let uuid = cbuuid_to_uuid(&raw_uuid);
                    service_map.insert(uuid, s);
                }
                let id = unsafe { peripheral.identifier() };
                let peripheral_uuid = nsuuid_to_uuid(&id);
                self.send_event(CentralDelegateEvent::DiscoveredServices {
                    peripheral_uuid,
                    services: service_map,
                });
            }
        }

        #[unsafe(method(peripheral:didDiscoverIncludedServicesForService:error:))]
        fn delegate_peripheral_diddiscoverincludedservicesforservice_error(
            &self,
            peripheral: &CBPeripheral,
            service: &CBService,
            error: Option<&NSError>,
        ) {
            trace!(
                "delegate_peripheral_diddiscoverincludedservicesforservice_error {} {} {}",
                peripheral_debug(peripheral),
                service_debug(service),
                localized_description(error)
            );
            if error.is_none() {
                let includes = unsafe { service.includedServices() }.unwrap_or_default();
                for s in includes {
                    unsafe { peripheral.discoverCharacteristics_forService(None, &s) };
                }
            }
        }

        #[unsafe(method(peripheral:didDiscoverCharacteristicsForService:error:))]
        fn delegate_peripheral_diddiscovercharacteristicsforservice_error(
            &self,
            peripheral: &CBPeripheral,
            service: &CBService,
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
                let chars = unsafe { service.characteristics() }.unwrap_or_default();
                for c in chars {
                    unsafe { peripheral.discoverDescriptorsForCharacteristic(&c) };
                    // Create the map entry we'll need to export.
                    let raw_uuid = unsafe { c.UUID() };
                    let uuid = cbuuid_to_uuid(&raw_uuid);
                    characteristics.insert(uuid, c);
                }
                let id = unsafe { peripheral.identifier() };
                let peripheral_uuid = nsuuid_to_uuid(&id);
                let raw_service_uuid = unsafe { service.UUID() };
                let service_uuid = cbuuid_to_uuid(&raw_service_uuid);
                self.send_event(CentralDelegateEvent::DiscoveredCharacteristics {
                    peripheral_uuid,
                    service_uuid,
                    characteristics,
                });
            }
        }

        #[unsafe(method(peripheral:didDiscoverDescriptorsForCharacteristic:error:))]
        fn delegate_peripheral_diddiscoverdescriptorsforcharacteristic_error(
            &self,
            peripheral: &CBPeripheral,
            characteristic: &CBCharacteristic,
            error: Option<&NSError>,
        ) {
            trace!(
                "delegate_peripheral_diddiscoverdescriptorsforcharacteristic_error {} {} {}",
                peripheral_debug(peripheral),
                characteristic_debug(characteristic),
                localized_description(error)
            );
            // Send the event even on error when the characteristic is associated
            // with a service, so discover_services() can complete.
            let mut descriptors = HashMap::new();
            if error.is_some() {
                warn!(
                    "Error discovering descriptors for characteristic {}, continuing with empty descriptors: {}",
                    characteristic_debug(characteristic),
                    localized_description(error)
                );
            }
            if error.is_none() {
                let descs = unsafe { characteristic.descriptors() }.unwrap_or_default();
                for d in descs {
                    let raw_uuid = unsafe { d.UUID() };
                    let uuid = cbuuid_to_uuid(&raw_uuid);
                    descriptors.insert(uuid, d);
                }
            }
            let id = unsafe { peripheral.identifier() };
            let peripheral_uuid = nsuuid_to_uuid(&id);
            let Some(service) = (unsafe { characteristic.service() }) else {
                warn!(
                    "Descriptor discovery completed for characteristic {} without an associated service",
                    characteristic_debug(characteristic)
                );
                return;
            };
            let raw_service_uuid = unsafe { service.UUID() };
            let service_uuid = cbuuid_to_uuid(&raw_service_uuid);
            let raw_char_uuid = unsafe { characteristic.UUID() };
            let characteristic_uuid = cbuuid_to_uuid(&raw_char_uuid);
            self.send_event(CentralDelegateEvent::DiscoveredCharacteristicDescriptors {
                peripheral_uuid,
                service_uuid,
                characteristic_uuid,
                descriptors,
            });
        }

        #[unsafe(method(peripheral:didUpdateValueForCharacteristic:error:))]
        fn delegate_peripheral_didupdatevalueforcharacteristic_error(
            &self,
            peripheral: &CBPeripheral,
            characteristic: &CBCharacteristic,
            error: Option<&NSError>,
        ) {
            trace!(
                "delegate_peripheral_didupdatevalueforcharacteristic_error {} {} {}",
                peripheral_debug(peripheral),
                characteristic_debug(characteristic),
                localized_description(error)
            );
            let Some(service) = (unsafe { characteristic.service() }) else {
                warn!(
                    "Characteristic value update for {} has no associated service",
                    characteristic_debug(characteristic)
                );
                return;
            };
            let id = unsafe { peripheral.identifier() };
            let peripheral_uuid = nsuuid_to_uuid(&id);
            let raw_service_uuid = unsafe { service.UUID() };
            let service_uuid = cbuuid_to_uuid(&raw_service_uuid);
            let raw_char_uuid = unsafe { characteristic.UUID() };
            let characteristic_uuid = cbuuid_to_uuid(&raw_char_uuid);
            self.send_event(CentralDelegateEvent::CharacteristicNotified {
                peripheral_uuid,
                service_uuid,
                characteristic_uuid,
                data: get_characteristic_value(characteristic),
                error: error.map(|e| e.localizedDescription().to_string()),
            });
        }

        #[unsafe(method(peripheral:didWriteValueForCharacteristic:error:))]
        fn delegate_peripheral_didwritevalueforcharacteristic_error(
            &self,
            peripheral: &CBPeripheral,
            characteristic: &CBCharacteristic,
            error: Option<&NSError>,
        ) {
            trace!(
                "delegate_peripheral_didwritevalueforcharacteristic_error {} {} {}",
                peripheral_debug(peripheral),
                characteristic_debug(characteristic),
                localized_description(error)
            );
            if error.is_some() {
                let Some(service) = (unsafe { characteristic.service() }) else { return };
                self.send_event(CentralDelegateEvent::CharacteristicWritten {
                    peripheral_uuid: nsuuid_to_uuid(&*unsafe { peripheral.identifier() }),
                    service_uuid: cbuuid_to_uuid(&*unsafe { service.UUID() }),
                    characteristic_uuid: cbuuid_to_uuid(&*unsafe { characteristic.UUID() }),
                    error: error.map(|e| e.localizedDescription().to_string()),
                });
            } else {
                let service = unsafe { characteristic.service() }.unwrap();
                let id = unsafe { peripheral.identifier() };
                let peripheral_uuid = nsuuid_to_uuid(&id);
                let raw_service_uuid = unsafe { service.UUID() };
                let service_uuid = cbuuid_to_uuid(&raw_service_uuid);
                let raw_char_uuid = unsafe { characteristic.UUID() };
                let characteristic_uuid = cbuuid_to_uuid(&raw_char_uuid);
                self.send_event(CentralDelegateEvent::CharacteristicWritten {
                    peripheral_uuid,
                    service_uuid,
                    characteristic_uuid,
                    error: error.map(|e| e.localizedDescription().to_string()),
                });
            }
        }

        #[unsafe(method(peripheral:didUpdateNotificationStateForCharacteristic:error:))]
        fn delegate_peripheral_didupdatenotificationstateforcharacteristic_error(
            &self,
            peripheral: &CBPeripheral,
            characteristic: &CBCharacteristic,
            error: Option<&NSError>,
        ) {
            trace!("delegate_peripheral_didupdatenotificationstateforcharacteristic_error");
            // TODO check for error here
            let id = unsafe { peripheral.identifier() };
            let peripheral_uuid = nsuuid_to_uuid(&id);
            let service = unsafe { characteristic.service() }.unwrap();
            let raw_service_uuid = unsafe { service.UUID() };
            let service_uuid = cbuuid_to_uuid(&raw_service_uuid);
            let raw_char_uuid = unsafe { characteristic.UUID() };
            let characteristic_uuid = cbuuid_to_uuid(&raw_char_uuid);
            if unsafe { characteristic.isNotifying() } {
                self.send_event(CentralDelegateEvent::CharacteristicSubscribed {
                    peripheral_uuid,
                    service_uuid,
                    characteristic_uuid,
                    error: error.map(|e| e.localizedDescription().to_string()),
                });
            } else {
                self.send_event(CentralDelegateEvent::CharacteristicUnsubscribed {
                    peripheral_uuid,
                    service_uuid,
                    characteristic_uuid,
                    error: error.map(|e| e.localizedDescription().to_string()),
                });
            }
        }

        #[unsafe(method(peripheral:didReadRSSI:error:))]
        fn delegate_peripheral_didreadrssi_error(
            &self,
            peripheral: &CBPeripheral,
            rssi: &NSNumber,
            error: Option<&NSError>,
        ) {
            trace!(
                "delegate_peripheral_didreadrssi_error {}",
                peripheral_debug(peripheral)
            );
            let id = unsafe { peripheral.identifier() };
            let peripheral_uuid = nsuuid_to_uuid(&id);
            let rssi_value = rssi.as_i16();
            self.send_event(CentralDelegateEvent::DidReadRssi {
                peripheral_uuid,
                rssi: rssi_value,
                error: error.map(|e| e.localizedDescription().to_string()),
            });
        }

        #[unsafe(method(peripheral:didUpdateValueForDescriptor:error:))]
        fn delegate_peripheral_didupdatevaluefordescriptor_error(
            &self,
            peripheral: &CBPeripheral,
            descriptor: &CBDescriptor,
            error: Option<&NSError>,
        ) {
            trace!(
                "delegate_peripheral_didupdatevaluefordescriptor_error {} {} {}",
                peripheral_debug(peripheral),
                descriptor_debug(descriptor),
                localized_description(error)
            );
            if let Some(error) = error {
                let Some(characteristic) = (unsafe { descriptor.characteristic() }) else { return };
                let Some(service) = (unsafe { characteristic.service() }) else { return };
                self.send_event(CentralDelegateEvent::DescriptorNotified {
                    peripheral_uuid: nsuuid_to_uuid(&*unsafe { peripheral.identifier() }),
                    service_uuid: cbuuid_to_uuid(&*unsafe { service.UUID() }),
                    characteristic_uuid: cbuuid_to_uuid(&*unsafe { characteristic.UUID() }),
                    descriptor_uuid: cbuuid_to_uuid(&*unsafe { descriptor.UUID() }),
                    data: Vec::new(), error: Some(error.localizedDescription().to_string()),
                });
            } else {
                let characteristic = unsafe { descriptor.characteristic() }.unwrap();
                let service = unsafe { characteristic.service() }.unwrap();
                let id = unsafe { peripheral.identifier() };
                let peripheral_uuid = nsuuid_to_uuid(&id);
                let raw_service_uuid = unsafe { service.UUID() };
                let service_uuid = cbuuid_to_uuid(&raw_service_uuid);
                let raw_char_uuid = unsafe { characteristic.UUID() };
                let characteristic_uuid = cbuuid_to_uuid(&raw_char_uuid);
                let raw_desc_uuid = unsafe { descriptor.UUID() };
                let descriptor_uuid = cbuuid_to_uuid(&raw_desc_uuid);
                self.send_event(CentralDelegateEvent::DescriptorNotified {
                    peripheral_uuid,
                    service_uuid,
                    characteristic_uuid,
                    descriptor_uuid,
                    data: get_descriptor_value(descriptor),
                    error: error.map(|e| e.localizedDescription().to_string()),
                });
                // Notify BluetoothGATTCharacteristic::read_value that read was successful.
            }
        }

        #[unsafe(method(peripheral:didWriteValueForDescriptor:error:))]
        fn delegate_peripheral_didwritevaluefordescriptor_error(
            &self,
            peripheral: &CBPeripheral,
            descriptor: &CBDescriptor,
            error: Option<&NSError>,
        ) {
            trace!(
                "delegate_peripheral_didwritevaluefordescriptor_error {} {} {}",
                peripheral_debug(peripheral),
                descriptor_debug(descriptor),
                localized_description(error)
            );
            if let Some(error) = error {
                let Some(characteristic) = (unsafe { descriptor.characteristic() }) else { return };
                let Some(service) = (unsafe { characteristic.service() }) else { return };
                self.send_event(CentralDelegateEvent::DescriptorWritten {
                    peripheral_uuid: nsuuid_to_uuid(&*unsafe { peripheral.identifier() }),
                    service_uuid: cbuuid_to_uuid(&*unsafe { service.UUID() }),
                    characteristic_uuid: cbuuid_to_uuid(&*unsafe { characteristic.UUID() }),
                    descriptor_uuid: cbuuid_to_uuid(&*unsafe { descriptor.UUID() }),
                    error: Some(error.localizedDescription().to_string()),
                });
            } else {
                let characteristic = unsafe { descriptor.characteristic() }.unwrap();
                let service = unsafe { characteristic.service() }.unwrap();
                let id = unsafe { peripheral.identifier() };
                let peripheral_uuid = nsuuid_to_uuid(&id);
                let raw_service_uuid = unsafe { service.UUID() };
                let service_uuid = cbuuid_to_uuid(&raw_service_uuid);
                let raw_char_uuid = unsafe { characteristic.UUID() };
                let characteristic_uuid = cbuuid_to_uuid(&raw_char_uuid);
                let raw_desc_uuid = unsafe { descriptor.UUID() };
                let descriptor_uuid = cbuuid_to_uuid(&raw_desc_uuid);
                self.send_event(CentralDelegateEvent::DescriptorWritten {
                    peripheral_uuid,
                    service_uuid,
                    characteristic_uuid,
                    descriptor_uuid,
                    error: error.map(|e| e.localizedDescription().to_string()),
                });
            }
        }

        #[unsafe(method(peripheral:didModifyServices:))]
        fn delegate_peripheral_didmodifyservices(
            &self,
            peripheral: &CBPeripheral,
            _invalidated_services: &NSArray<CBService>,
        ) {
            trace!(
                "delegate_peripheral_didmodifyservices {}",
                peripheral_debug(peripheral),
            );
            // This is a corebluetooth-only event that makes peripheral services unusable until discovery has been performed again.
            // https://developer.apple.com/documentation/corebluetooth/cbperipheraldelegate/peripheral(_:didmodifyservices:)?language=objc
            // Trigger the removal of internal corebluetooth peripheral discovered services. It is also expected that
            // discover_services() will be performed again on the peripheral at the API level as soon as is practical.
            // NOTE: the list of modified services does not appear to be particularly useful; a full service rediscovery is needed.
            let id = unsafe { peripheral.identifier() };
            let peripheral_uuid = nsuuid_to_uuid(&id);
            self.send_event(CentralDelegateEvent::ServicesModified {
                peripheral_uuid,
            });
        }

        #[unsafe(method(peripheralIsReadyToSendWriteWithoutResponse:))]
        fn delegate_peripheral_is_ready_to_send_write_without_response(
            &self,
            peripheral: &CBPeripheral,
        ) {
            trace!(
                "delegate_peripheral_is_ready_to_send_write_without_response {}",
                peripheral_debug(peripheral)
            );
            let id = unsafe { peripheral.identifier() };
            let peripheral_uuid = nsuuid_to_uuid(&id);
            self.send_event(CentralDelegateEvent::ReadyToSendWriteWithoutResponse {
                peripheral_uuid,
            });
        }
    }
);

impl CentralDelegate {
    pub fn new(sender: Sender<CentralDelegateEvent>) -> Retained<Self> {
        let this = CentralDelegate::alloc().set_ivars(sender);
        unsafe { msg_send![super(this), init] }
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

fn get_characteristic_value(characteristic: &CBCharacteristic) -> Vec<u8> {
    trace!("Getting data!");
    let v = unsafe { characteristic.value() }.map(|value| value.to_vec());
    trace!("BluetoothGATTCharacteristic::get_value -> {:?}", v);
    v.unwrap_or_default()
}

fn get_descriptor_value(descriptor: &CBDescriptor) -> Vec<u8> {
    trace!("Getting data!");
    let v = unsafe { descriptor.value() }.map(|value| {
        let mut clazz = value.class();
        // Find the root class until we reach NSObject
        while let Some(superclass) = clazz.superclass() {
            if superclass == NSObject::class() {
                break;
            }
            clazz = superclass;
        }

        match clazz.name().to_bytes() {
            b"NSString" => {
                let d: Retained<NSString> = value.downcast().unwrap();
                d.to_string().into_bytes()
            }
            b"NSData" => {
                let d: Retained<NSData> = value.downcast().unwrap();
                d.to_vec()
            }
            b"NSNumber" => {
                let d: Retained<NSNumber> = value.downcast().unwrap();
                d.stringValue().to_string().into_bytes()
            }
            _ => {
                error!("Unknown descriptor value class: {:?}", clazz);
                Vec::new()
            }
        }
    });
    trace!("BluetoothGATTDescriptor::get_value -> {:?}", v);
    v.unwrap_or_default()
}

fn peripheral_debug(peripheral: &CBPeripheral) -> String {
    let uuid = unsafe { peripheral.identifier() }.UUIDString();
    match unsafe { peripheral.name() } {
        Some(name) => {
            format!("CBPeripheral({}, {})", name, uuid)
        }
        _ => {
            format!("CBPeripheral({})", uuid)
        }
    }
}

fn service_debug(service: &CBService) -> String {
    let uuid = unsafe { service.UUID().UUIDString() };
    format!("CBService({})", uuid)
}

fn characteristic_debug(characteristic: &CBCharacteristic) -> String {
    let uuid = unsafe { characteristic.UUID().UUIDString() };
    format!("CBCharacteristic({})", uuid)
}

fn descriptor_debug(descriptor: &CBDescriptor) -> String {
    let uuid = unsafe { descriptor.UUID().UUIDString() };
    format!("CBDescriptor({})", uuid)
}
