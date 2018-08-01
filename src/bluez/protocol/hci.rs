use nom::{le_u8, le_u16, le_u32, le_u64, le_i8, IResult, Err, ErrorKind};
use num::FromPrimitive;
use bytes::{BytesMut, BufMut};


use ::api::{BDAddr, AddressType};
use bluez::constants::*;
use bluez::protocol::*;


#[cfg(test)]
mod tests {
    use ::api::BDAddr;
    use super::*;
    use super::LEAdvertisingData::*;
    use super::HCIStatus;

    #[test]
    fn test_decode_device_discovery() {
        let buf = [4,62,40,2,1,4,0,192,74,150,234,218,116,28,18,9,76,69,68,66,
            108,117,101,45,69,65,57,54,52,65,67,48,32,5,18,16,0,20,0,2,10,4,190];

        let expected = Message::LEAdvertisingReport(
            LEAdvertisingInfo {
                evt_type: 4,
                bdaddr_type: 0,
                bdaddr: BDAddr {
                    address: [192, 74, 150, 234, 218, 116],
                },
                data: vec![
                    LocalName(String::from("LEDBlue-EA964AC0 ")),
                    SlaveConnectionIntervalRange(16, 20),
                    TxPowerLevel(4),
                ]
            }
        );

        assert_eq!(message(&buf), Ok((&[][..], expected)));
    }

    #[test]
    fn test_decode_device_discovery2() {
        let buf = [4, 62, 23, 2, 1, 0, 0, 192, 74, 150, 234, 218, 116, 11, 2,
            1, 6, 7, 2, 240, 255, 229, 255, 224, 255, 194];

        let expected = Message::LEAdvertisingReport(
            LEAdvertisingInfo {
                evt_type: 0,
                bdaddr_type: 0,
                bdaddr: BDAddr {
                    address: [192, 74, 150, 234, 218, 116],
                },
                data: vec![
                    Flags(AdvertisingFlags::BR_EDR_NOT_SUPPORTED |
                        AdvertisingFlags::LE_GENERAL_DISCOVERABLE_MODE),
                    ServiceClassUUID16(0xFFF0),
                    ServiceClassUUID16(0xFFE5),
                    ServiceClassUUID16(0xFFE0),
                ]
            }
        );

        assert_eq!(message(&buf), Ok((&[][..], expected)));
    }

    #[test]
    fn test_bd_addr() {
        let buf = [192u8,74,150,234,218,116];
        assert_eq!(bd_addr(&buf), Ok((&[][..],BDAddr {
            address: [192, 74, 150, 234, 218, 116]})))
    }

    #[test]
    fn test_le_advertising_info() {
        let buf = [1,4,0,192,74,150,234,218,116,11,2,1,6,7,2,240,255,229,255,224,255];

        assert_eq!(le_advertising_info(&buf), Ok((&[][..], LEAdvertisingInfo {
            evt_type: 4,
            bdaddr_type: 0,
            bdaddr: BDAddr {
                address: [192,74,150,234,218,116],
            },
            data: vec![
                Flags(AdvertisingFlags::BR_EDR_NOT_SUPPORTED |
                    AdvertisingFlags::LE_GENERAL_DISCOVERABLE_MODE),
                ServiceClassUUID16(65520),
                ServiceClassUUID16(65509),
                ServiceClassUUID16(65504)],
        })));
    }

    #[test]
    fn test_le_advertising_data() {
        let buf = [7, 2, 240, 255, 229, 255, 224, 255];

        assert_eq!(le_advertising_data(&buf), Ok((&[][..],
                                                            vec![ServiceClassUUID16(65520),
                                                                 ServiceClassUUID16(65509),
                                                                 ServiceClassUUID16(65504)])));

        let buf = [18,9,76,69,68,66,108,117,101,45,69,65,57,55,66,55,65,51,32];
        assert_eq!(le_advertising_data(&buf), Ok((&[][..], vec![
            LocalName(String::from("LEDBlue-EA97B7A3 "))])));
    }

    #[test]
    fn test_acl_data_packet() {
        let buf = [2, 64, 32, 9, 0, 5, 0, 4, 0, 1, 16, 1, 0, 16];
        assert_eq!(message(&buf), Ok((
            &[][..],
            Message::ACLDataPacket(ACLData {
                handle: 64,
                cid: 4,
                data: vec![1, 16, 1, 0, 16],
                len: 5,
            }),
        )))
    }

    #[test]
    fn test_cmd_status() {
        let buf = [4, 15, 4, 0, 1, 22, 32];
        assert_eq!(message(&buf), Ok((
            &[][..],
            Message::CommandStatus {
                command: CommandType::LEReadRemoteUsedFeatures,
                status: HCIStatus::Success,
            }
        )));
    }

    #[test]
    fn test_recv_le_meta() {
        let buf = [4, 62, 12, 4, 0, 64, 0, 1, 0, 0, 0, 0, 0, 0, 0];
        assert_eq!(message(&buf), Ok((
            &[][..],
            Message::LEReadRemoteUsedFeaturesComplete {
                status: HCIStatus::Success,
                handle: 64,
                flags: LEFeatureFlags::LE_ENCRYPTION,
            }
        )))
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct ACLData {
    pub handle: u16,
    pub cid: u16,
    pub data: Vec<u8>,
    pub len: u16,
}

bitflags! {
    pub struct LEFeatureFlags: u64 {
        const LE_ENCRYPTION = 0x0001;
        const CONNECTION_PARAMETERS_REQUEST_PROCEDURE = 0x0002;
        const EXTENDED_REJECT_INDICATION = 0x0004;
        const SLAVE_INITIATED_FEATURES_EXCHANGE = 0x0008;
        const PING = 0x0010;
        const DATA_PACKET_LENGTH_EXTENSION = 0x0020;
        const LL_PRIVACY = 0x0040;
        const EXTENDED_SCANNER_FILTER_POLICIES = 0x0080;
        const LE_2M_PHY = 0x0100;
        const STABLE_MODULATION_INDEX_TX = 0x0200;
        const STABLE_MODULATION_INDEX_RX = 0x0400;
        const LE_CODED_PHY = 0x0800;
        const LE_EXTENDED_ADVERTISING = 0x1000;
        const LE_PERIODIC_ADVERTISING = 0x2000;
        const CHANNEL_SELECTION_ALGORITHM_2 = 0x4000;
        const POWER_CLASS_1 = 0x8000;
        const MINIMUM_NUMBER_OF_USED_CHANNELS_PROCEDURE = 0x10000;
    }
}

#[derive(Debug, PartialEq)]
pub enum Message {
    LEAdvertisingReport(LEAdvertisingInfo),
    LEConnComplete(LEConnInfo),
    LEConnUpdate(LEConnUpdateInfo),
    LEReadRemoteUsedFeaturesComplete {
        status: HCIStatus,
        handle: u16,
        flags: LEFeatureFlags,
    },
    HCICommandComplete(CommandComplete),
    LEScanEnableCommand {
        enable: bool,
        filter_duplicates: bool,
    },
    HCICommand {
        command: CommandType,
        data: Vec<u8>,
    },
    DisconnectComplete {
        status: HCIStatus,
        handle: u16,
        reason: HCIStatus,
    },
    CommandStatus {
        command: CommandType,
        status: HCIStatus,
    },
    ACLDataPacket(ACLData),
    ACLDataContinuation {
        handle: u16,
        data: Vec<u8>,
    }
}

bitflags! {
    pub struct AdvertisingFlags: u8 {
        const LE_LIMITED_DISCOVERABLE_MODE = 0x01;
        const LE_GENERAL_DISCOVERABLE_MODE = 0x02;
        const BR_EDR_NOT_SUPPORTED = 0x04;
        const SIMULTANEOUS_LE_BR_EDR_TO_SAME_DEVICE_CAPABLE_CONTROLLER = 0x08;
        const SIMULTANEOUS_LE_BR_EDR_TO_SAME_DEVICE_CAPABLE_HOST = 0x10;
        const RESERVED3 = 0x20;
        const RESERVED2 = 0x40;
        const RESERVED1 = 0x80;
    }
}

#[derive(Debug, PartialEq)]
pub enum LEAdvertisingData {
    Flags(AdvertisingFlags),
    ServiceClassUUID16(u16),
    ServiceClassUUID128([u8; 16]),
    LocalName(String),
    TxPowerLevel(i8),
    SlaveConnectionIntervalRange(u16, u16),
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

#[derive(Debug, PartialEq)]
pub struct LEConnUpdateInfo {
    pub status: HCIStatus,
    pub handle: u16,
    pub interval: u16,
    pub latency: u16,
    pub supervision_timeout: u16,
}


enum_from_primitive! {
#[derive(Debug, PartialEq)]
#[repr(u8)]
pub enum HCIStatus {
    ACLConnectionAlreadyExists = 0x0B,
    AuthenticationFialure = 0x05,
    ChannelAssessmentNotSupported = 0x2E,
    CommandDisallowed = 0x0C,
    CoarseClockAdjustRejected = 0x40,
    ConnectionAcceptanceTimeoutExceeded = 0x10,
    ConnectionFailuedtoEstablish = 0x3E,
    ConnectionLimitExceeded = 0x09,
    ConnectionRejectedDuetoLimitedResources = 0x0D,
    ConnectionRejectedNoSuitableChannelFound = 0x39,
    ConnectionRejectedForSecurityReasons = 0x0E,
    ConnectionRejectedDuetoUnacceptableBDADDR = 0x0F,
    ConnectionTerminatedByLocalHost = 0x16,
    ConnectionTerminatedDuetoMICFailure = 0x3D,
    ConnectionTimeout = 0x08,
    ControllerBusy = 0x3A,
    DifferentTransactionCollision = 0x2A,
    DirectedAdvertisingTimeout = 0x3C,
    EncryptModeNotAcceptable = 0x25,
    ExtendedInquiryResponseTooLarge = 0x36,
    HostBusyPairing = 0x38,
    HardwareFailure = 0x03,
    InstantPassed = 0x28,
    InsufficientSecurity = 0x2F,
    InvalidHCICommandParameters = 0x12,
    InvalidLMPParamaters = 0x1E,
    LinkKeyCanNotBeChanged = 0x26,
    LMPErrorTransactionCollision = 0x23,
    LMPLLResponseTimeout = 0x22,
    LMPDUNotAllowed = 0x24,
    MACConnectionFailed = 0x3F,
    MemoryCapabilityExceeded = 0x07,
    PageTimeout = 0x04,
    PairingNotAllowed = 0x18,
    PairingWithUnitKeyNotSupported = 0x29,
    ParamaterOutOfMandatoryRange = 0x30,
    PinKeyMissing = 0x06,
    QOSReject = 0x2D,
    QOSUnacceptableParameter = 0x2C,
    RemoteDeviceTerminatedConnectionDueToLowResources = 0x14,
    RemoteDeviceTerminatedConnectionDuetoPowerOff = 0x15,
    RemoteUserTerminatedConnection = 0x13,
    RepeatedAttempts = 0x17,
    RequestQOSNotSupported = 0x27,
    Reserved2B = 0x2B,
    Reserved31 = 0x31,
    Reserved33 = 0x33,
    ReservedSlotViolation = 0x34,
    RoleChangeNotAllowed = 0x21,
    RoleSwitchFailed = 0x35,
    RoleSwitchPending = 0x32,
    SCOAirModeRejected = 0x1D,
    SCOIntervalRejected = 0x1C,
    SCOOffsetRejected = 0x1B,
    SimplePairingNotSupportedByHost = 0x37,
    SynchonousConnectionLimitExceeded = 0x0A,
    UnacceptableConnectionParameters = 0x3B,
    UnknownConnectionID = 0x02,
    UnknownHCICommand = 0x01,
    UnknownLMPPDU = 0x19,
    UnspecifiedError = 0x1F,
    UnsupportedParamter = 0x11,
    UnsupportedLMPParameterValue = 0x20,
    UnsupportedRemoteFeature = 0x1A,
    Success = 0x00,
}
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
    LEReadRemoteUsedFeaturesComplete = 4,
}}

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

    ChangeLocalName = 0x0C13,
    WriteExtendedInquiryResponse = 0x0C52,

    LESetEventMask = OCF_LE_SET_EVENT_MASK | (OGF_LE_CTL as u16) << 10,
    LESetScanParameters = OCF_LE_SET_SCAN_PARAMETERS | (OGF_LE_CTL as u16) << 10,
    LESetScanEnabled = OCF_LE_SET_SCAN_ENABLE | (OGF_LE_CTL as u16) << 10,
    LECreateConnection = OCF_LE_CREATE_CONN | (OGF_LE_CTL as u16) << 10,
    LEConnectionUpdate = OCF_LE_CONN_UPDATE | (OGF_LE_CTL as u16) << 10,
    LEStartEncryption = OCF_LE_START_ENCRYPTION | (OGF_LE_CTL as u16) << 10,

    LESetAdvertisingData = 0x2008,
    LESetScanResponseData = 0x2009,
    LEAddDeviceToWhiteList = 0x2011,
    LERemoveDeviceFromWhiteList = 0x2012,
    LEReadRemoteUsedFeatures = 0x2016,

    Disconnect = 0x0406,
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

fn le_advertising_data(i: &[u8]) -> IResult<&[u8], Vec<LEAdvertisingData>> {
    use self::LEAdvertisingData::*;
    let (i, len) = try_parse!(i, le_u8);
    let (i, typ) = try_parse!(i, le_u8);

    let len = len as usize - 1;
    // let mut result = vec![];
    let (i, result)= match typ {
        0x1 => {
            try_parse!(i, map!(le_u8, |u| vec![Flags(AdvertisingFlags::from_bits_truncate(u))]))
        }
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
        0x12 => {
            try_parse!(i, do_parse!(
              min: le_u16 >>
              max: le_u16 >>
              (vec![SlaveConnectionIntervalRange(min, max)])
            ))
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
            debug!("Unknown field type {} in {:?}", typ, i);
            (&i[len as usize..], vec![])
        }
    };
    Ok((i, result))
}

named!(le_advertising_info<&[u8], LEAdvertisingInfo>,
    do_parse!(
       // TODO: support counts other than 1
       _count: le_u8 >>
       evt_type: le_u8 >>
       bdaddr_type: le_u8 >>
       bdaddr: bd_addr >>
       data: length_value!(le_u8, fold_many0!(complete!(le_advertising_data), Vec::new(), |mut acc: Vec<_>, x| {
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
       _skip: le_u8 >>
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

named!(le_read_remote_used_features_complete<&[u8], Message>,
    do_parse!(
      status: map_opt!(le_u8, |b| HCIStatus::from_u8(b)) >>
      handle: le_u16 >>
      flags: le_u64 >>
      (
          Message::LEReadRemoteUsedFeaturesComplete {
              status, handle,
              flags: LEFeatureFlags::from_bits_truncate(flags),
          }
      )
    )
);

named!(le_conn_update_complete<&[u8], Message>,
    do_parse!(
        status: map_opt!(le_u8, |b| HCIStatus::from_u8(b)) >>
        handle: le_u16 >>
        interval: le_u16 >>
        latency: le_u16 >>
        supervision_timeout: le_u16 >>
        (
          Message::LEConnUpdate(LEConnUpdateInfo {
            status, handle, interval, latency, supervision_timeout
          })
        )
));

fn le_meta_event(i: &[u8]) -> IResult<&[u8], Message> {
    let (i, le_type) = try_parse!(i, map_opt!(le_u8, |b| LEEventType::from_u8(b)));
    let (i, result) = match le_type {
        LEEventType::LEAdvertisingReport => {
            try_parse!(i, map!(le_advertising_info, |x| Message::LEAdvertisingReport(x)))
        }
        LEEventType::LEConnComplete => {
            try_parse!(i, map!(le_conn_complete, |x| Message::LEConnComplete(x)))
        }
        LEEventType::LEReadRemoteUsedFeaturesComplete => {
            try_parse!(i, le_read_remote_used_features_complete)
        }
        LEEventType::LEConnUpdateComplete => {
            try_parse!(i, le_conn_update_complete)
        }
    };
    Ok((i, result))
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
            LESetScanEnabled { enabled: status == 1 }
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

    Ok((&i, Message::HCICommandComplete(result)))
}

named!(disconnect_complete<&[u8], Message>,
    do_parse!(
      status: map_opt!(le_u8, |b| HCIStatus::from_u8(b)) >>
      handle: le_u16 >>
      reason: map_opt!(le_u8, |b| HCIStatus::from_u8(b)) >>
      (
          Message::DisconnectComplete {
              status, handle, reason
          }
      )
    )
);

fn hci_event_pkt(i: &[u8]) -> IResult<&[u8], Message> {
    use self::HCIEventSubType::*;
    let (i, sub_type) = try_parse!(i, map_opt!(le_u8, |b| HCIEventSubType::from_u8(b)));
    let (i, data) = try_parse!(i, length_data!(le_u8));
    let result = match sub_type {
        LEMetaEvent => try_parse!(data, le_meta_event).1,
        CmdComplete => try_parse!(data, cmd_complete).1,
        CmdStatus => {
            let (data, status) = try_parse!(data, map_opt!(le_u8, |b| HCIStatus::from_u8(b)));
            let (data, _) = try_parse!(data, le_u8);
            let (_, command) = try_parse!(data, map_opt!(le_u16, |b| CommandType::from_u16(b)));
            Message::CommandStatus {
                command, status,
            }
        },
        DisconnComplete => try_parse!(data, disconnect_complete).1,
        _ => {
            warn!("Unhandled HCIEventPkt subtype {:?}", sub_type);
            return Err(Err::Error(error_position!(i, ErrorKind::Custom(4))));
        }
    };
    Ok((i, result))
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
    Ok((i, result))
}

fn hci_acldata_pkt(i: &[u8]) -> IResult<&[u8], Message> {
    let (i, head) = try_parse!(i, le_u16); // 2
    let flags = head >> 12;
    let handle = head & 0x0FFF;
    let (i, message) = match flags {
        ACL_START | ACL_START_NO_FLUSH => {
            // the length of this packet
            let (i, dlen) = try_parse!(i, le_u16);
            // the length of the message, which may span multiple packets
            let (i, plen) = try_parse!(i, le_u16);
            let (i, cid) = try_parse!(i, le_u16);
            let (i, data) = try_parse!(i, take!(dlen - 4));
            (i, Message::ACLDataPacket(ACLData {
                handle,
                cid,
                data: data.to_owned(),
                len: plen,
            }))
        }
        ACL_CONT => {
            (&[][..], Message::ACLDataContinuation {
                handle,
                data: i.clone().to_owned(),
            })
        },
        x => {
            warn!("unknown flag type: {}", x);
            return Err(Err::Error(error_position!(i, ErrorKind::Custom(11))));
        }
    };
    Ok((i, message))
}

pub fn message(i: &[u8]) -> IResult<&[u8], Message> {
    use self::EventType::*;

    let (i, typ) = try_parse!(i, map_opt!(le_u8, |b| EventType::from_u8(b)));
    match typ {
        HCICommandPkt => hci_command_pkt(i), // 1
        HCIAclDataPkt => hci_acldata_pkt(i), // 2
        HCIEventPkt => hci_event_pkt(i),     // 4
    }
}

pub fn hci_command(command: u16, data: &[u8]) -> BytesMut {
    let mut buf = BytesMut::with_capacity(4 + data.len());

    // header
    buf.put_u8(HCI_COMMAND_PKT);
    buf.put_u16_le(command);

    // len
    buf.put_u8(data.len() as u8);

    // data
    buf.put(data);
    buf
}
