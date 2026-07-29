// btleplug Source Code File
//
// Copyright 2020 Nonpolynomial Labs LLC. All rights reserved.
//
// Licensed under the BSD 3-Clause license. See LICENSE file in the project root
// for full license information.

/// GAP Appearance advertising data type.
pub(crate) const APPEARANCE_DATA_TYPE: u8 = 0x19;

/// Parse the payload of a GAP Appearance advertising data section.
pub(crate) fn parse_appearance(data: &[u8]) -> Option<u16> {
    let bytes: [u8; 2] = data.try_into().ok()?;
    Some(u16::from_le_bytes(bytes))
}

/// Parse GAP Appearance from a length-prefixed Bluetooth LE advertising record.
#[cfg(any(target_os = "android", test))]
pub(crate) fn parse_appearance_from_advertisement(data: &[u8]) -> Option<u16> {
    let mut offset = 0;

    while let Some(&length) = data.get(offset) {
        offset += 1;

        if length == 0 {
            break;
        }

        let end = offset.checked_add(usize::from(length))?;
        let section = data.get(offset..end)?;
        let (&data_type, payload) = section.split_first()?;

        if data_type == APPEARANCE_DATA_TYPE {
            if let Some(appearance) = parse_appearance(payload) {
                return Some(appearance);
            }
        }

        offset = end;
    }

    None
}

#[cfg(test)]
mod tests {
    use super::{APPEARANCE_DATA_TYPE, parse_appearance, parse_appearance_from_advertisement};

    #[test]
    fn parses_appearance_payload_as_little_endian() {
        assert_eq!(parse_appearance(&[0x80, 0x04]), Some(0x0480));
        assert_eq!(parse_appearance(&[0x00, 0x00]), Some(0x0000));
    }

    #[test]
    fn rejects_appearance_payloads_that_are_not_exactly_two_bytes() {
        assert_eq!(parse_appearance(&[]), None);
        assert_eq!(parse_appearance(&[0x80]), None);
        assert_eq!(parse_appearance(&[0x80, 0x04, 0x00]), None);
    }

    #[test]
    fn finds_appearance_among_other_advertising_sections() {
        let advertisement = [
            2,
            0x01,
            0x06,
            3,
            APPEARANCE_DATA_TYPE,
            0x80,
            0x04,
            2,
            0x0a,
            0xf8,
        ];

        assert_eq!(
            parse_appearance_from_advertisement(&advertisement),
            Some(0x0480)
        );
    }

    #[test]
    fn skips_invalid_appearance_section_and_accepts_a_later_valid_one() {
        let advertisement = [
            2,
            APPEARANCE_DATA_TYPE,
            0x80,
            3,
            APPEARANCE_DATA_TYPE,
            0x40,
            0x03,
        ];

        assert_eq!(
            parse_appearance_from_advertisement(&advertisement),
            Some(0x0340)
        );
    }

    #[test]
    fn handles_missing_and_malformed_advertising_sections() {
        assert_eq!(parse_appearance_from_advertisement(&[]), None);
        assert_eq!(parse_appearance_from_advertisement(&[0]), None);
        assert_eq!(parse_appearance_from_advertisement(&[2, 0x01, 0x06]), None);
        assert_eq!(
            parse_appearance_from_advertisement(&[3, APPEARANCE_DATA_TYPE, 0x80,]),
            None
        );
        assert_eq!(
            parse_appearance_from_advertisement(&[0, 3, APPEARANCE_DATA_TYPE, 0x80, 0x04,]),
            None
        );
    }
}
