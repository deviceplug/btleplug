use super::{
    advertisement_data_type, parse_advertised_name, parse_service_data, should_accept_name,
};
use crate::api::bleuuid::{uuid_from_u16, uuid_from_u32};

#[test]
fn advertised_name_removes_only_nul_padding() {
    let name = parse_advertised_name(b" Device Name \0\0", true).unwrap();

    assert_eq!(name.value, " Device Name ");
    assert!(name.is_complete);
}

#[test]
fn advertised_name_rejects_empty_and_invalid_utf8() {
    assert!(parse_advertised_name(b"\0\0", false).is_none());
    assert!(parse_advertised_name(&[0xff], true).is_none());
}

#[test]
fn complete_name_cannot_be_replaced_by_short_name() {
    assert!(!should_accept_name(true, false));
    assert!(should_accept_name(true, true));
    assert!(should_accept_name(false, false));
    assert!(should_accept_name(false, true));
}

#[test]
fn parse_service_data_16bit_short_data_returns_none() {
    assert!(parse_service_data(advertisement_data_type::SERVICE_DATA_16_BIT_UUID, &[]).is_none());
    assert!(
        parse_service_data(advertisement_data_type::SERVICE_DATA_16_BIT_UUID, &[0x00]).is_none()
    );
}

#[test]
fn parse_service_data_16bit_valid_data() {
    let data = vec![0xAB, 0xCD, 0x01, 0x02, 0x03];
    let (uuid, rest) =
        parse_service_data(advertisement_data_type::SERVICE_DATA_16_BIT_UUID, &data).unwrap();
    assert_eq!(uuid, uuid_from_u16(0xCDAB));
    assert_eq!(rest, vec![0x01, 0x02, 0x03]);
}

#[test]
fn parse_service_data_32bit_short_data_returns_none() {
    assert!(
        parse_service_data(
            advertisement_data_type::SERVICE_DATA_32_BIT_UUID,
            &[0x00, 0x00, 0x00]
        )
        .is_none()
    );
}

#[test]
fn parse_service_data_32bit_valid_data() {
    let data = vec![0xAB, 0xCD, 0xEF, 0x00, 0x04, 0x05];
    let (uuid, rest) =
        parse_service_data(advertisement_data_type::SERVICE_DATA_32_BIT_UUID, &data).unwrap();
    assert_eq!(uuid, uuid_from_u32(0x00EFCDAB));
    assert_eq!(rest, vec![0x04, 0x05]);
}

#[test]
fn parse_service_data_128bit_short_data_returns_none() {
    assert!(
        parse_service_data(
            advertisement_data_type::SERVICE_DATA_128_BIT_UUID,
            &[0x00; 15]
        )
        .is_none()
    );
}

#[test]
fn parse_service_data_128bit_valid_data() {
    let data = [
        0xFB, 0x34, 0x9B, 0x5F, 0x80, 0x00, 0x00, 0x80, 0x00, 0x10, 0x00, 0x00, 0x0F, 0x18, 0x00,
        0x00, 0x99,
    ];
    let (uuid, rest) =
        parse_service_data(advertisement_data_type::SERVICE_DATA_128_BIT_UUID, &data).unwrap();
    assert_eq!(uuid, uuid_from_u16(0x180F));
    assert_eq!(rest, vec![0x99]);
}

#[test]
fn parse_service_data_unknown_type_returns_none() {
    assert!(parse_service_data(0xFF, &[0x01, 0x02]).is_none());
}
