use std::io;
use std::mem;

use bincode::deserialize;
use nom::{le_u8, le_u16, le_u32, le_i8, IResult, Err, ErrorKind};
use num::FromPrimitive;

use ::device::Device;
use ::adapter::BDAddr;
use ::manager::Event;


#[cfg(test)]
mod tests {
    use ::device::Device;
    use ::manager::Event;
    use ::adapter::BDAddr;
    use nom::IResult;
    use super::*;
    use super::LEAdvertisingData::*;
    use super::Message::*;

    #[test]
    fn test_decode_device_discovery2() {
        let buf = [4,62,40,2,1,4,0,192,74,150,234,218,116,28,18,9,76,69,68,66,
            108,117,101,45,69,65,57,54,52,65,67,48,32,5,18,16,0,20,0,2,10,4,190];

        // vec![
        // 18,9,76,69,68,66,108,117,101,45,69,65,57,54,52,65,67,48,32,
        // 5,18,16,0,20,0,
        // 2,10,4]
        let expected = Message::LEAdvertisingReport(
            LEAdvertisingInfo {
                evt_type: 4,
                bdaddr_type: 0,
                bdaddr: BDAddr {
                    address: [192, 74, 150, 234, 218, 116],
                },
                data: vec![
                    LocalName(String::from("LEDBlue-EA964AC0 ")),
                    TxPowerLevel(4)
                ]
            }
        );

        let device = assert_eq!(message(&buf), IResult::Done(&[][..], expected));
    }

    #[test]
    fn test_bd_addr() {
        let buf = [192u8,74,150,234,218,116];
        assert_eq!(bd_addr(&buf), IResult::Done(&[][..],BDAddr {
            address: [192, 74, 150, 234, 218, 116]}))
    }

    #[test]
    fn test_le_advertising_info() {
        let buf = [1, 4,0,192,74,150,234,218,116,11,2,1,6,7,2,240,255,229,255,224,255];

        assert_eq!(le_advertising_info(&buf), IResult::Done(&[][..], LEAdvertisingInfo {
                evt_type: 4,
                bdaddr_type: 0,
                bdaddr: BDAddr {
                    address: [192,74,150,234,218,116],
                },
                data: vec![ServiceClassUUID16(65520),
                           ServiceClassUUID16(65509),
                           ServiceClassUUID16(65504)],
        }));
    }

    #[test]
    fn test_le_advertising_data() {
        let buf = [7, 2, 240, 255, 229, 255, 224, 255];

        assert_eq!(le_advertising_data(&buf), IResult::Done(&[][..],
           vec![ServiceClassUUID16(65520),
               ServiceClassUUID16(65509),
               ServiceClassUUID16(65504)]));

        let buf = [18,9,76,69,68,66,108,117,101,45,69,65,57,55,66,55,65,51,32];
        assert_eq!(le_advertising_data(&buf), IResult::Done(&[][..], vec![
            LocalName(String::from("LEDBlue-EA97B7A3 "))]));
    }
}

#[derive(Debug, PartialEq)]
pub enum Message {
    LEAdvertisingReport(LEAdvertisingInfo),
}

#[derive(Debug, PartialEq)]
pub enum LEAdvertisingData {
    ServiceClassUUID16(u16),
    ServiceClassUUID128([u8; 16]),
    LocalName(String),
    TxPowerLevel(i8),
    SolicitationUUID16(u16),
    SolicitationUUID128([u8; 16]),
    ServiceData16(u16, Vec<u8>),
    ServiceData32(u32, Vec<u8>),
    ServiceData128([u8; 16], Vec<u8>),
    SolicitationUUID32(u32),
    ManufacturerSpecific(Vec<u8>),
}

#[derive(Debug, PartialEq)]
pub struct LEAdvertisingInfo {
    pub evt_type: u8,
    pub bdaddr_type: u8,
    pub bdaddr: BDAddr,
    pub data: Vec<LEAdvertisingData>
}

pub struct AdapterDecoder {
}

enum_from_primitive! {
#[derive(Debug, PartialEq)]
#[repr(u8)]
enum EventType {
    HCICommandPkt = 1,
    HCIAclDataPkt = 2,
    HCIEventPkt = 4,
}
}

enum_from_primitive! {
#[derive(Debug, PartialEq)]
#[repr(u8)]
enum SubEventType {
    DisconnComplete = 0x05,
    EncryptChange = 0x08,
    CmdComplete = 0x0e,
    CmdStatus = 0x0f,
    LEMetaEvent = 0x3e,
}
}

enum_from_primitive! {
#[derive(Debug, PartialEq)]
#[repr(u8)]
enum LEEventType {
    LEConnComplete = 1,
    LEAdvertisingReport = 2,
    LEConnUpdateComplete = 3,
}
}

named!(parse_uuid_128<&[u8], [u8; 16]>, count_fixed!(u8, le_u8, 16));

fn le_advertising_data(i: &[u8]) -> IResult<&[u8], Vec<LEAdvertisingData>> {
    use self::LEAdvertisingData::*;
    let (i, len) = try_parse!(i, le_u8);
    let (i, typ) = try_parse!(i, le_u8);

    let len = len as usize - 1;
    // let mut result = vec![];
    let (i, result)= match typ {
        0x02|0x03 =>  {
            try_parse!(i, count!(map!(le_u16, |u| ServiceClassUUID16(u)), len / 2))
        }
        0x06|0x07 => {
            try_parse!(i, count!(map!(parse_uuid_128,
                |b| ServiceClassUUID128(b)), len / 16))
        }
        0x08|0x09 => {
            try_parse!(i, map!(take!(len),
                |b| vec![LocalName(String::from_utf8_lossy(b).into_owned())]))
        }
        0x0A => {
            try_parse!(i, map!(le_i8, |b| vec![TxPowerLevel(b)]))
        }
        0x14 => {
            try_parse!(i, count!(map!(le_u16, |u| SolicitationUUID16(u)), len / 2))
        }
        0x15 => {
            try_parse!(i, count!(map!(parse_uuid_128,
                |b| SolicitationUUID128(b)), len / 16))
        }
        0x16 => {
            try_parse!(i, do_parse!(
                uuid: le_u16 >>
                data: count!(le_u8, len - 2) >>
                (vec![ServiceData16(uuid, data)])))
        }
        0x20 => {
            try_parse!(i, do_parse!(
                uuid: le_u32 >>
                data: count!(le_u8, len - 4) >>
                (vec![ServiceData32(uuid, data)])))
        }
        0x21 => {
            try_parse!(i, do_parse!(
                uuid: parse_uuid_128 >>
                data: count!(le_u8, len - 16) >>
                (vec![ServiceData128(uuid, data)])))
        }
        0x1F => {
            try_parse!(i, count!(map!(le_u32,
                |b| SolicitationUUID32(b)), len / 4))
        }
        0xFF => {
            try_parse!(i, map!(count!(le_u8, len), |b| vec![ManufacturerSpecific(b)]))
        }
        _ => {
            // skip this field
            debug!("Unknown field type {}", typ);
            (&i[len as usize..], vec![])
        }
    };
    IResult::Done(i, result)
}

named!(le_advertising_info<&[u8], LEAdvertisingInfo>,
    do_parse!(
       // TODO: support counts other than 1
       count: le_u8 >>
       evt_type: le_u8 >>
       bdaddr_type: le_u8 >>
       bdaddr: bd_addr >>
       data: length_value!(le_u8, fold_many0!(le_advertising_data, Vec::new(), |mut acc: Vec<_>, x| {
           acc.extend(x);
           acc
       })) >>
       (
         LEAdvertisingInfo {
           evt_type, bdaddr_type, bdaddr, data: data
         }
       )
    ));

named!(bd_addr<&[u8], BDAddr>,
    do_parse!(
      addr: take!(6) >> (
         BDAddr {
            address: [addr[0], addr[1], addr[2], addr[3], addr[4], addr[5]],
         })
));

fn le_meta_event(i: &[u8]) -> IResult<&[u8], Message> {
    let (i, le_type) = try_parse!(i, map_opt!(le_u8, |b| LEEventType::from_u8(b)));
    let (i, result) = match le_type {
        LEEventType::LEAdvertisingReport => {
            try_parse!(i, map!(le_advertising_info, |x| Message::LEAdvertisingReport(x)))
        },
        _ => {
            warn!("Unhandled le_type {:?}", le_type);
            return IResult::Error(Err::Code(ErrorKind::Custom(0)))
        }
    };
    IResult::Done(i, result)
}

fn message(i: &[u8]) -> IResult<&[u8], Message> {
    use self::Message::*;
    use self::EventType::*;
    use self::SubEventType::*;

    let (i, typ) = try_parse!(i, map_opt!(le_u8, |b| EventType::from_u8(b)));
    let (i, sub_typ) = try_parse!(i, map_opt!(le_u8, |b| SubEventType::from_u8(b)));
    let (i, data) = try_parse!(i, length_data!(le_u8));
    let (_, result) = match (typ, sub_typ) {
        (HCIEventPkt, LEMetaEvent) => {
            try_parse!(data, le_meta_event)
        },
        (typ, sub_typ) => {
            warn!("Unhandled type/subtype ({:?}, {:?})", typ, sub_typ);
            return IResult::Error(Err::Code(ErrorKind::Custom(0)))
        }
    };

    IResult::Done(i, result)
}

impl AdapterDecoder {
    pub fn decode(buf: &[u8]) -> IResult<&[u8], Message> {
        message(buf)
    }
}
