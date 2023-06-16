// btleplug Source Code File
//
// Copyright 2020 Nonpolynomial Labs LLC. All rights reserved.
//
// Licensed under the BSD 3-Clause license. See LICENSE file in the project root
// for full license information.
//
// Some portions of this file are taken and/or modified from Rumble
// (https://github.com/mwylde/rumble), using a dual MIT/Apache License under the
// following copyright:
//
// Copyright (c) 2014 The Rust Project Developers

use super::super::utils;
use crate::api::Descriptor;

use uuid::Uuid;
use windows::Devices::Bluetooth::GenericAttributeProfile::GattDescriptor;

#[derive(Debug)]
pub struct BLEDescriptor {
    descriptor: GattDescriptor,
}

impl BLEDescriptor {
    pub fn new(descriptor: GattDescriptor) -> Self {
        Self { descriptor }
    }

    pub fn uuid(&self) -> Uuid {
        utils::to_uuid(&self.descriptor.Uuid().unwrap())
    }

    pub fn to_descriptor(&self, service_uuid: Uuid, characteristic_uuid: Uuid) -> Descriptor {
        let uuid = self.uuid();
        Descriptor {
            uuid,
            service_uuid,
            characteristic_uuid,
        }
    }
}
