use std::io;
use std::mem;

use bincode::deserialize;
use nom::le_u8;

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

    #[test]
    fn test_decode_device_discovery1() {
        let buf = [
            4u8, 62, 23, 2, 1, 0, 0, 192, 74, 150, 234, 218, 116, 11, 2, 1, 6, 7, 2,
            240, 255, 229, 255, 224, 255, 190];

        let expected = Message::LEAdvertisingReport(
            LeAdvertisingInfo {
                evt_type: 0,
                bdaddr_type: 0,
                bdaddr: BDAddr {
                    address: [116u8, 218, 234, 150, 74, 192]
                },
                length: 11
            },
            vec![2, 1, 6, 7, 2, 240, 255, 229, 255, 224, 255]
        );

        let device = AdapterDecoder::decode(&buf).unwrap();
    }

    #[test]
    fn test_decode_device_discovery2() {
        let buf = [4,62,40,2,1,4,0,192,74,150,234,218,116,28,18,9,76,69,68,66,108,117,101,
            45,69,65,57,54,52,65,67,48,32,5,18,16,0,20,0,2,10,4,190];

        let expected = Message::LEAdvertisingReport(
            LeAdvertisingInfo {
                evt_type: 4,
                bdaddr_type: 0,
                bdaddr: BDAddr {
                    address: [116u8, 218, 234, 150, 74, 192],
                },
                length: 28
            },
            vec![18,9,76,69,68,66,108,117,101,45,69,65,57,54,52,65,67,48,32,5,18,16,0,20,0,2,10,4]
        );

        let device = AdapterDecoder::decode(&buf).unwrap();
    }

    #[test]
    fn test_bd_addr() {
        let buf = [192u8,74,150,234,218,116];
        assert_eq!(bd_addr(&buf), IResult::Done(&[][..],BDAddr {
            address: [192, 74, 150, 234, 218, 116]}))
    }

    #[test]
    fn test_le_advertising_info() {
        let buf = [4,0,192,74,150,234,218,116,28];

        assert_eq!(le_advertising_info(&buf), IResult::Done(&[][..], LeAdvertisingInfo {
                evt_type: 4,
                bdaddr_type: 0,
                bdaddr: BDAddr {
                    address: [192,74,150,234,218,116],
                },
                length: 28
            }));
    }
}

enum Message {
    LEAdvertisingReport(LeAdvertisingInfo, Vec<u8>)
}

#[derive(Copy, Deserialize, Debug, PartialEq)]
#[repr(C)]
pub struct LeAdvertisingInfo {
    pub evt_type : u8,
    pub bdaddr_type : u8,
    pub bdaddr : BDAddr,
    pub length : u8,
}

impl Clone for LeAdvertisingInfo {
    fn clone(&self) -> Self { *self }
}

pub struct AdapterDecoder {
}

const HCI_EVENT_HDR_SIZE: i32 = 2;
const EIR_NAME_SHORT: u8 = 0x08;  // shortened local name
const EIR_NAME_COMPLETE: u8 = 0x09;  // complete local name

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


fn parse_name(data: Vec<u8>) -> Option<String> {
    let len = data.len();
    let mut iter = data.into_iter();
    let mut offset = 0usize;
    while offset < len {
        let field_len = iter.next()? as usize;

        // check for the end of EIR
        if field_len == 0 {
            break;
        }

        let t = iter.next()?;
        if t == EIR_NAME_SHORT || t == EIR_NAME_COMPLETE {
            let name_len = field_len - 1;
            let bytes: Vec<u8> = iter.take(field_len - 1).collect();
            if bytes.len() < name_len {
                return None;
            } else {
                return String::from_utf8(bytes).ok();
            }
        }

        offset += field_len;
    }
    return None;
}

struct Frame {
    message_type: EventType,
}

named!(le_advertising_info<&[u8], LeAdvertisingInfo>,
    do_parse!(
       evt_type: le_u8 >>
       bdaddr_type: le_u8 >>
       bdaddr: bd_addr >>
       length: le_u8 >>
       (
         LeAdvertisingInfo {
           evt_type, bdaddr_type, bdaddr, length
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



// format is
// [MESSAGE_TYPE(u8), ???(u8), length(u8), ...]
impl AdapterDecoder {
    fn decode(buf: &[u8]) -> io::Result<Message> {
        unimplemented!();
    }
//    pub fn decode(buf: &[u8]) -> io::Result<Option<(Message, usize)>> {
//        let idx = 1usize + HCI_EVENT_HDR_SIZE as usize;
//
//        if buf.len() < idx + 2 {
//            return Ok(None)
//        }
//
//        let sub_event = buf[idx];
//        match sub_event {
//            2 => AdapterDecoder::decode_device(&buf[idx + 2..]),
//            _ => panic!("Unknown sub_event {}", sub_event)
//        }
//    }
//
//    fn decode_device(buf: &[u8]) -> io::Result<Option<(Event, usize)>> {
//        let mut idx = 0usize;
//
//        if buf.len() < mem::size_of::<LeAdvertisingInfo>() {
//            return Ok(None)
//        }
//
//        let info: LeAdvertisingInfo = deserialize(&buf[idx..]).unwrap();
//        idx += mem::size_of_val(&info);
//
//        if buf.len() < idx + info.length as usize {
//            return Ok(None)
//        }
//
//        let data: Vec<u8> = buf[idx..idx + info.length as usize].to_vec();
//        idx += info.length as usize;
//        let name = parse_name(data);
//
//        let device = Device {
//            addr: info.bdaddr,
//            name
//        };
//
//        Ok(Some((Event::DeviceDiscovered(device), idx)))
//    }
}
