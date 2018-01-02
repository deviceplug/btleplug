use nom::{le_u8, le_u16, le_u32, le_i8, IResult, Err, ErrorKind};
use num::FromPrimitive;

use ::adapter::{BDAddr, AddressType};
use ::device::{Characteristic, CharacteristicUUID};
use ::constants::*;

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

        assert_eq!(message(&buf), IResult::Done(&[][..], expected));
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

    #[test]
    fn test_acl_data_packet() {
        let buf = [2, 64, 32, 9, 0, 5, 0, 4, 0, 1, 16, 1, 0, 16];
        assert_eq!(Decoder::decode(&buf), IResult::Done(
            &[][..],
            Message::ACLDataPacket {
                handle: 64,
                cid: 5,
                data: vec![4, 0, 1, 16, 1, 0, 16],
                len: 7,
            }
        ))
    }
}

#[derive(Debug, PartialEq)]
pub enum Message {
    LEAdvertisingReport(LEAdvertisingInfo),
    LEConnComplete(LEConnInfo),
    HCICommandComplete(CommandComplete),
    LEScanEnableCommand {
        enable: bool,
        filter_duplicates: bool,
    },
    HCICommand {
        command: CommandType,
        data: Vec<u8>,
    },
    CommandStatus {
        command: CommandType,
        status: u8,
    },
    ACLDataPacket {
        handle: u16,
        cid: u16,
        data: Vec<u8>,
        len: u16
    },
    ACLDataContinuation {
        handle: u16,
        data: Vec<u8>,
    }
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

#[derive(Debug, PartialEq)]
pub struct LEConnInfo {
    pub handle: u16,
    pub role: u8,
    pub bdaddr: BDAddr,
    pub bdaddr_type: u8,
    pub interval: u16,
    pub latency: u16,
    pub supervision_timeout: u16,
    pub master_clock_accuracy: u8,
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
enum HCIEventSubType {
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

enum_from_primitive! {
#[derive(Debug, PartialEq)]
#[repr(u16)]
pub enum CommandType {
    Reset = OCF_RESET as u16 | (OGF_HOST_CTL as u16) << 10,
    ReadLEHostSupported = OCF_READ_LE_HOST_SUPPORTED | (OGF_HOST_CTL as u16) << 10,
    WriteLEHostSupported = OCF_WRITE_LE_HOST_SUPPORTED | (OGF_HOST_CTL as u16) << 10,
    ReadLocalVersion = OCF_READ_LOCAL_VERSION | (OGF_INFO_PARAM as u16) << 10,
    ReadBDAddr = OCF_READ_BD_ADDR | (OGF_INFO_PARAM as u16) << 10,
    ReadRSSI = OCF_READ_RSSI | (OGF_STATUS_PARAM as u16) << 10,

    LESetEventMask = OCF_LE_SET_EVENT_MASK | (OGF_LE_CTL as u16) << 10,
    LESetScanParameters = OCF_LE_SET_SCAN_PARAMETERS | (OGF_LE_CTL as u16) << 10,
    LESetScanEnabled = OCF_LE_SET_SCAN_ENABLE | (OGF_LE_CTL as u16) << 10,
    LECreateConnection = OCF_LE_CREATE_CONN | (OGF_LE_CTL as u16) << 10,
    LEConnectionUpdate = OCF_LE_CONN_UPDATE | (OGF_LE_CTL as u16) << 10,
    LEStartEncryption = OCF_LE_START_ENCRYPTION | (OGF_LE_CTL as u16) << 10,
}}

#[allow(dead_code)]
#[derive(Debug, PartialEq)]
pub enum CommandComplete {
    Reset,
    ReadLEHostSupported { le: u8, simul: u8 },
    ReadLocalVersion {
        hci_version: u8,
        hci_revision: u16,
        lmp_version: i8,
        manufacturer: u16,
        lmp_sub_version: u8,
    },
    ReadBDAddr {
        address_type: AddressType,
        address: BDAddr,
    },
    LESetScanParameters,
    LESetScanEnabled {
        enabled: bool,
    },
    ReadRSSI {
        handle: u16,
        rssi: u8
    },
    Other {
        command: CommandType,
        status: u8,
        data: Vec<u8>
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

named!(le_conn_complete<&[u8], LEConnInfo>,
    do_parse!(
       // TODO: check this
       skip: le_u8 >>
       handle: le_u16 >>
       role: le_u8 >>
       bdaddr_type: le_u8 >>
       bdaddr: bd_addr >>
       interval: le_u16 >>
       latency: le_u16 >>
       supervision_timeout: le_u16 >>
       master_clock_accuracy: le_u8 >>
       (
           LEConnInfo {
              handle, role, bdaddr_type, bdaddr, interval, latency,
              supervision_timeout, master_clock_accuracy
           }
       )));

fn le_meta_event(i: &[u8]) -> IResult<&[u8], Message> {
    let (i, le_type) = try_parse!(i, map_opt!(le_u8, |b| LEEventType::from_u8(b)));
    let (i, result) = match le_type {
        LEEventType::LEAdvertisingReport => {
            try_parse!(i, map!(le_advertising_info, |x| Message::LEAdvertisingReport(x)))
        }
        LEEventType::LEConnComplete => {
            try_parse!(i, map!(le_conn_complete, |x| Message::LEConnComplete(x)))
        }
        _ => {
            warn!("Unhandled le_type {:?}", le_type);
            return IResult::Error(Err::Code(ErrorKind::Custom(1)))
        }
    };
    IResult::Done(i, result)
}

fn cmd_complete(i: &[u8]) -> IResult<&[u8], Message> {
    use self::CommandComplete::*;

    let (i, _skip) = try_parse!(i, le_u8);
    let (i, cmd) = try_parse!(i, map_opt!(le_u16, |b| CommandType::from_u16(b)));
    let (i, status) = try_parse!(i, le_u8);
    let result = match cmd {
        CommandType::Reset => Reset,
        CommandType::ReadLEHostSupported => {
            let (i, le) = try_parse!(i, le_u8);
            let (_, simul) = try_parse!(i, le_u8);
            ReadLEHostSupported { le, simul }
        },
        CommandType::ReadBDAddr => {
            let (i, address_type) = try_parse!(i, map_opt!(le_u8, |b| AddressType::from_u8(b)));
            let (_, address) = try_parse!(i, bd_addr);

            ReadBDAddr { address_type, address }
        },
        CommandType::LESetScanParameters => LESetScanParameters,
        CommandType::LESetScanEnabled => {
            // TODO: not 100% sure about this
            let enabled = status == 0;
            LESetScanEnabled { enabled }
        },
        CommandType::ReadRSSI => {
            let (i, handle) = try_parse!(i, le_u16);
            let (_, rssi) = try_parse!(i, le_u8);
            ReadRSSI { handle, rssi }
        },
        x => {
            Other {
                command: x,
                status,
                data: i.clone().to_owned()
            }
        }
    };

    IResult::Done(&i, Message::HCICommandComplete(result))
}

fn hci_event_pkt(i: &[u8]) -> IResult<&[u8], Message> {
    use self::HCIEventSubType::*;
    let (i, sub_type) = try_parse!(i, map_opt!(le_u8, |b| HCIEventSubType::from_u8(b)));
    let (i, data) = try_parse!(i, length_data!(le_u8));
    let result = match sub_type {
        LEMetaEvent => try_parse!(data, le_meta_event).1,
        CmdComplete => try_parse!(data, cmd_complete).1,
        CmdStatus => {
            let (data, status) = try_parse!(data, le_u8);
            let (data, _) = try_parse!(data, le_u8);
            let (_, command) = try_parse!(data, map_opt!(le_u16, |b| CommandType::from_u16(b)));
            Message::CommandStatus {
                command, status,
            }
        },
        _ => {
            warn!("unhandled HCIEventPkt subtype {:?}", sub_type);
            return IResult::Error(Err::Code(ErrorKind::Custom(4)))
        }
    };
    IResult::Done(i, result)
}

fn hci_command_pkt(i: &[u8]) -> IResult<&[u8], Message> {
    let (i, cmd) = try_parse!(i, map_opt!(le_u16, CommandType::from_u16));
    let (i, data) = try_parse!(i, length_data!(le_u8));
    let result = match cmd {
        CommandType::LESetScanEnabled => {
            let (data, enable) = try_parse!(data, le_u8);
            let (_, filter_duplicates) = try_parse!(data, le_u8);
            Message::LEScanEnableCommand {
                enable: enable == 1,
                filter_duplicates: filter_duplicates == 1,
            }
        },
        other => {
            Message::HCICommand {
                command: other,
                data: data.to_owned(),
            }
        }
    };
    IResult::Done(i, result)
}

fn hci_acldata_pkt(i: &[u8]) -> IResult<&[u8], Message> {
    let (i, head) = try_parse!(i, le_u16); // 2
    let flags = head >> 12;
    let handle = head & 0x0FFF;
    let message = match flags {
        ACL_START | ACL_START_NO_FLUSH => {
            let (i, dlen) = try_parse!(i, le_u16);
            let (i, cid) = try_parse!(i, le_u16);
            Message::ACLDataPacket {
                handle,
                cid,
                data: i.clone().to_owned(),
                len: dlen - 2,
            }
        }
        ACL_CONT => {
            Message::ACLDataContinuation {
                handle,
                data: i.clone().to_owned(),
            }
        },
        x => {
            warn!("unknown flag type: {}", x);
            return IResult::Error(Err::Code(ErrorKind::Custom(11)));
        }
    };
    IResult::Done(&i[i.len()..], message)
}

fn message(i: &[u8]) -> IResult<&[u8], Message> {
    use self::EventType::*;

    let (i, typ) = try_parse!(i, map_opt!(le_u8, |b| EventType::from_u8(b)));
    match typ {
        HCIEventPkt => hci_event_pkt(i),
        HCICommandPkt => hci_command_pkt(i),
        HCIAclDataPkt => hci_acldata_pkt(i),
    }
}

fn characteristics(i: &[u8]) -> IResult<&[u8], Vec<Characteristic>> {
    let (i, opcode) = try_parse!(i, le_u8);

    let (i, result) = match opcode {
        ATT_OP_READ_BY_TYPE_RESP => {
            let (i, rec_len) = try_parse!(i, le_u8);
            let num = i.len() / rec_len as usize;
            let mut result = Vec::with_capacity(num);
            for _ in 0..num {
                let (i, start_handle) = try_parse!(i, le_u16);
                let (i, properties) = try_parse!(i, le_u8);
                let (i, value_handle) = try_parse!(i, le_u16);
                let (_, uuid) = if rec_len == 7 {
                    try_parse!(i, map!(le_u16, |b| CharacteristicUUID::B16(b)))
                } else {
                    try_parse!(i, map!(parse_uuid_128, |b| CharacteristicUUID::B128(b)))
                };
                result.push(Characteristic {
                    start_handle, properties, value_handle, uuid
                });
            }
            (&[][..], result)
        }
        x => {
            warn!("unhandled characteristics op type {}", x);
            (&[][..], vec![])
        }
    };

    IResult::Done(i, result)
}

pub struct Decoder {
}


impl Decoder {
    pub fn decode(buf: &[u8]) -> IResult<&[u8], Message> {
        message(buf)
    }

    pub fn decode_characteristics(buf: &[u8]) -> IResult<&[u8], Vec<Characteristic>> {
        characteristics(buf)
    }
}
