use std::io;
use std::mem;

use bincode::deserialize;

use ::device::Device;
use ::adapter::BDAddr;
use ::manager::Event;


#[derive(Copy, Deserialize, Debug)]
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

impl AdapterDecoder {
    pub fn decode(buf: &[u8]) -> io::Result<Option<(Event, usize)>> {
        let idx = 1usize + HCI_EVENT_HDR_SIZE as usize;

        if buf.len() < idx + 2 {
            return Ok(None)
        }

        let sub_event = buf[idx];
        match sub_event {
            2 => AdapterDecoder::decode_device(&buf[idx + 2..]),
            _ => panic!("Unknown sub_event {}", sub_event)
        }
    }

    fn decode_device(buf: &[u8]) -> io::Result<Option<(Event, usize)>> {
        let mut idx = 0usize;

        if buf.len() < mem::size_of::<LeAdvertisingInfo>() {
            return Ok(None)
        }

        let info: LeAdvertisingInfo = deserialize(&buf[idx..]).unwrap();
        idx += mem::size_of_val(&info);

        if buf.len() < idx + info.length as usize {
            return Ok(None)
        }

        let data: Vec<u8> = buf[idx..idx + info.length as usize].to_vec();
        idx += info.length as usize;
        let name = parse_name(data);

        let device = Device {
            addr: info.bdaddr,
            name
        };

        Ok(Some((Event::DeviceDiscovered(device), idx)))
    }
}
