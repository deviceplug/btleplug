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

use winrt::*;

import!(
    dependencies
      os
    types
    windows::devices::bluetooth::generic_attribute_profile::{
        GattCharacteristic,
        GattCharacteristicProperties,
        GattClientCharacteristicConfigurationDescriptorValue,
        GattCommunicationStatus,
        GattDeviceService,
        GattDeviceServicesResult,
        GattValueChangedEventArgs,
    }
    windows::devices::bluetooth::advertisement::*
    windows::devices::bluetooth::{
        BluetoothConnectionStatus,
        BluetoothLEDevice,
    }
    windows::devices::radios::{
        Radio,
        RadioKind
    }
    windows::foundation::{
        EventRegistrationToken,
        TypedEventHandler,
    }
    windows::storage::streams::{
        DataReader,
        DataWriter,
    }
);

