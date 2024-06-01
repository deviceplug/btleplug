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

use objc2::rc::Retained;
use objc2_core_bluetooth::CBUUID;
use objc2_foundation::NSString;
use uuid::Uuid;

/// Convert a CBUUID object to the standard Uuid type.
pub fn cbuuid_to_uuid(cbuuid: &CBUUID) -> Uuid {
    // NOTE: CoreBluetooth tends to return uppercase UUID strings, and only 4
    // character long if the UUID is short (16 bits). It can also return 8
    // character strings if the rest of the UUID matches the generic UUID.
    let uuid = unsafe { cbuuid.UUIDString() }.to_string();
    let long = if uuid.len() == 4 {
        format!("0000{}-0000-1000-8000-00805f9b34fb", uuid)
    } else if uuid.len() == 8 {
        format!("{}-0000-1000-8000-00805f9b34fb", uuid)
    } else {
        uuid
    };
    let uuid_string = long.to_lowercase();
    uuid_string.parse().unwrap()
}

/// Convert a `Uuid` to a `CBUUID`.
pub fn uuid_to_cbuuid(uuid: Uuid) -> Retained<CBUUID> {
    unsafe { CBUUID::UUIDWithString(&NSString::from_str(&uuid.to_string())) }
}

#[cfg(test)]
mod tests {
    use objc2_foundation::ns_string;

    use super::*;

    #[test]
    fn parse_uuid_short() {
        let uuid_string = "1234";
        let uuid_nsstring = NSString::from_str(uuid_string);
        let cbuuid = unsafe { CBUUID::UUIDWithString(&uuid_nsstring) };
        let uuid = cbuuid_to_uuid(&*cbuuid);
        assert_eq!(
            uuid,
            Uuid::from_u128(0x00001234_0000_1000_8000_00805f9b34fb)
        );
    }

    #[test]
    fn parse_uuid_long() {
        let uuid_nsstring = ns_string!("12345678-0000-1111-2222-333344445555");
        let cbuuid = unsafe { CBUUID::UUIDWithString(uuid_nsstring) };
        let uuid = cbuuid_to_uuid(&*cbuuid);
        assert_eq!(
            uuid,
            Uuid::from_u128(0x12345678_0000_1111_2222_333344445555)
        );
    }

    #[test]
    fn cbuuid_roundtrip() {
        for uuid in [
            Uuid::from_u128(0x00001234_0000_1000_8000_00805f9b34fb),
            Uuid::from_u128(0xabcd1234_0000_1000_8000_00805f9b34fb),
            Uuid::from_u128(0x12345678_0000_1111_2222_333344445555),
        ] {
            assert_eq!(cbuuid_to_uuid(&*uuid_to_cbuuid(uuid)), uuid);
        }
    }
}
