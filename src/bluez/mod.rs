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

pub mod adapter;
mod bluez_dbus;
pub mod manager;
mod util;

const BLUEZ_DEST: &str = "org.bluez";

const DEFAULT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum AttributeType {
    Service,
    Characteristic,
    Descriptor,
}
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
struct Handle {
    typ: AttributeType,
    parent: u16,
    handle: u16,
}

impl PartialOrd for Handle {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        if self.handle == other.parent {
            return Some(std::cmp::Ordering::Greater);
        }
        if self.handle < other.handle {
            return Some(std::cmp::Ordering::Less);
        }
        if self.handle == other.handle {
            return Some(std::cmp::Ordering::Equal);
        }
        if self.handle > other.handle {
            return Some(std::cmp::Ordering::Greater);
        }
        None
    }
}
impl Ord for Handle {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.partial_cmp(other).unwrap()
    }
}
impl std::str::FromStr for Handle {
    type Err = crate::Error;

    fn from_str(s: &str) -> std::result::Result<Handle, crate::Error> {
        // serviceXXXX/charYYYY/descriptorZZZZ
        let mut handle = Handle {
            typ: AttributeType::Service,
            parent: 0,
            handle: 0,
        };
        let get_handle = |p| {
            u16::from_str_radix(&s[p..].trim_start_matches(char::is_alphabetic)[..4], 16).unwrap()
        };

        if let Some(descriptor) = s.find("descriptor") {
            handle.typ = AttributeType::Descriptor;
            handle.handle = get_handle(descriptor);
            handle.parent = get_handle(descriptor - 5);
        } else if let Some(characteristic) = s.find("char") {
            handle.typ = AttributeType::Characteristic;
            handle.handle = get_handle(characteristic);
            handle.parent = get_handle(characteristic - 5);
        } else if let Some(service) = s.find("service") {
            handle.typ = AttributeType::Service;
            handle.handle = get_handle(service);
            handle.parent = 0
        } else {
            return Err(crate::Error::Other("Can't parse".to_string()));
        }

        Ok(handle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_descriptor_handle() {
        let handle: Handle =
            "/org/bluez/hci0/dev_01_02_03_04_05_06/service0025/char0026/descriptor0027"
                .parse()
                .unwrap();
        assert_eq!(
            handle,
            Handle {
                typ: AttributeType::Descriptor,
                handle: 0x27_u16,
                parent: 0x26_u16
            }
        );
    }
    #[test]
    fn test_parse_characteristic_handle() {
        let handle: Handle = "/org/bluez/hci0/dev_01_02_03_04_05_06/service0025/char0026"
            .parse()
            .unwrap();
        assert_eq!(
            handle,
            Handle {
                typ: AttributeType::Characteristic,
                handle: 0x26_u16,
                parent: 0x25_u16
            }
        );
    }
    #[test]
    fn test_parse_service_handle() {
        let handle: Handle = "/org/bluez/hci0/dev_01_02_03_04_05_06/service0025"
            .parse()
            .unwrap();
        assert_eq!(
            handle,
            Handle {
                typ: AttributeType::Service,
                handle: 0x25_u16,
                parent: 0_u16
            }
        );
    }
}
