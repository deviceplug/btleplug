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

use nom::{le_u8, le_u16, IResult};

use crate::api::{Characteristic, UUID, CharPropFlags, ValueNotification};

use crate::bluez::constants::*;
use crate::bluez::protocol::*;
use bytes::{BytesMut, BufMut};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_characteristics() {
        let buf = [9, 7, 2, 0, 2, 3, 0, 0, 42, 4, 0, 2, 5, 0, 1, 42, 6, 0, 10, 7, 0, 2, 42];
        assert_eq!(characteristics(&buf), Ok((
            &[][..],
            Ok(vec![
                Characteristic {
                    start_handle: 2,
                    value_handle: 3,
                    end_handle: 0xFFFF,
                    uuid: UUID::B16(0x2A00),
                    properties: CharPropFlags::READ
                },
                Characteristic {
                    start_handle: 4,
                    value_handle: 5,
                    end_handle: 0xFFFF,
                    uuid: UUID::B16(0x2A01),
                    properties: CharPropFlags::READ
                },
                Characteristic {
                    start_handle: 6,
                    value_handle: 7,
                    end_handle: 0xFFFF,
                    uuid: UUID::B16(0x2A02),
                    properties: CharPropFlags::READ | CharPropFlags::WRITE
                },
            ]
        ))))
    }

    #[test]
    fn test_value_notification() {
        let buf = [27, 46, 0, 165, 17, 5, 0, 0, 130, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        assert_eq!(value_notification(&buf), Ok((
            &[][..],
            ValueNotification {
                uuid: UUID::B16(0),
                handle: Some(46),
                value: vec![165, 17, 5, 0, 0, 130, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            })
        ));
    }

    #[test]
    fn test_error() {
        let buf = [1, 8, 32, 0, 10];
        assert_eq!(characteristics(&buf), Ok((
            &[][..],
            Err(ErrorResponse {
                request_opcode: 0x08,
                handle: 0x20,
                error_code: 0x0a,
            })
        )))
    }

    #[test]
    fn test_read_req() {
        let expected: Vec<u8> = vec![0x0A, 0x25, 0x00];
        assert_eq!(expected, read_req(0x0025));
    }
}

#[derive(Debug, PartialEq)]
pub struct NotifyResponse {
    pub typ: u8,
    pub handle: u16,
    pub value: u16,
}

named!(pub notify_response<&[u8], NotifyResponse>,
   do_parse!(
      _op: tag!(&[ATT_OP_READ_BY_TYPE_RESP]) >>
      typ: le_u8 >>
      handle: le_u16 >>
      value: le_u16 >>
      (
        NotifyResponse { typ, handle, value }
      )
   ));

#[derive(Debug, PartialEq)]
pub struct ExchangeMTURequest {
    pub client_rx_mtu: u16,
}

named!(pub mtu_request<&[u8], ExchangeMTURequest>,
    do_parse!(
      _op: tag!(&[ATT_OP_EXCHANGE_MTU_REQ]) >>
      client_rx_mtu: le_u16 >>
      (
        ExchangeMTURequest { client_rx_mtu }
      )
    ));

#[derive(Debug, PartialEq)]
pub struct ErrorResponse {
    request_opcode: u8,
    handle: u16,
    error_code: u8,
}

named!(pub error_response<&[u8], ErrorResponse>,
    do_parse!(
        request_opcode: le_u8 >>
        handle: le_u16 >>
        error_code: le_u8 >>
        (
           ErrorResponse { request_opcode, handle, error_code }
        )
));

named!(pub value_notification<&[u8], ValueNotification>,
    do_parse!(
        _op: tag!(&[ATT_OP_VALUE_NOTIFICATION]) >>
        handle: le_u16 >>
        value: many1!(complete!(le_u8)) >>
        (
            ValueNotification {
                // This will need to be set elsewhere.
                uuid: UUID::B16(0),
                handle: Some(handle),
                value
            }
        )
));

fn characteristic(i: &[u8], b16_uuid: bool) -> IResult<&[u8], Characteristic> {
    let (i, start_handle) = try_parse!(i, le_u16);
    let (i, properties) = try_parse!(i, le_u8);
    let (i, value_handle) = try_parse!(i, le_u16);
    let (i, uuid) = if b16_uuid {
        try_parse!(i, map!(le_u16, |b| UUID::B16(b)))
    } else {
        try_parse!(i, map!(parse_uuid_128, |b| UUID::B128(b)))
    };

    Ok((i, Characteristic {
        start_handle,
        value_handle,
        end_handle: 0xFFFF,
        uuid,
        properties: CharPropFlags::from_bits_truncate(properties),
    }))
}

pub fn characteristics(i: &[u8]) -> IResult<&[u8], Result<Vec<Characteristic>, ErrorResponse>> {
    let (i, opcode) = try_parse!(i, le_u8);

    let (i, result) = match opcode {
        ATT_OP_ERROR_RESP => {
            try_parse!(i, map!(error_response, |r| Err(r)))
        }
        ATT_OP_READ_BY_TYPE_RESP => {
            let (i, rec_len) = try_parse!(i, le_u8);
            let num = i.len() / rec_len as usize;
            let b16_uuid = rec_len == 7;
            try_parse!(i, map!(count!(apply!(characteristic, b16_uuid), num), |r| Ok(r)))
        }
        x => {
            warn!("unhandled characteristics op type {} for {:?}", x, i);
            (&[][..], Ok(vec![]))
        }
    };

    Ok((i, result))
}

pub fn read_by_type_req(start_handle: u16, end_handle: u16, uuid: UUID) -> Vec<u8> {
    let mut buf = BytesMut::with_capacity(3 + uuid.size());
    buf.put_u8(ATT_OP_READ_BY_TYPE_REQ);
    buf.put_u16_le(start_handle);
    buf.put_u16_le(end_handle);
    match uuid {
        UUID::B16(u) => buf.put_u16_le(u),
        UUID::B128(u) => buf.put_slice(&u),
    }
    buf.to_vec()
}

pub fn read_req(handle: u16) -> Vec<u8> {
    let mut buf = BytesMut::with_capacity(3);
    buf.put_u8(ATT_OP_READ_REQ);
    buf.put_u16_le(handle);
    buf.to_vec()
}
