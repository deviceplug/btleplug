use std::io::prelude::*;
use std::mem;
use std::os::unix::io::FromRawFd;
use std::os::unix::net::UnixStream;

use bincode::deserialize;

use libc::{setsockopt, c_void};

use nix;

use ::util::handle_error;
use ::adapter::{BDAddr, ConnectedAdapter};
use ::device::Device;

fn hci_set_bit_safe(nr: i32, cur: i32) -> i32 {
    cur | 1 << (nr & 31)
}

#[derive(Copy, Debug)]
#[repr(C)]
pub struct HCIFilter {
    pub type_mask : u32,
    pub event_mask : [u32; 2],
    pub opcode : u16,
}

impl HCIFilter {
    fn default() -> HCIFilter {
        return HCIFilter {
            type_mask: 0,
            event_mask: [0u32; 2],
            opcode: 0
        };
    }

    fn set_ptype(&mut self, ptype: i32) {
        if ptype >= 32 {
            panic!("input too large");
        }

        let nr =  if ptype == 0xff { 0 } else { ptype & 31 };
        self.type_mask = hci_set_bit_safe(nr, self.type_mask as i32) as u32
    }

    fn set_event(&mut self, event: i32) {
        let byte = (event >> 5) as usize;
        if byte > self.event_mask.len() {
            panic!("input too large");
        }

        self.event_mask[byte] = hci_set_bit_safe(event & 63,
                                                 self.event_mask[byte] as i32) as u32;
    }
}

impl Clone for HCIFilter {
    fn clone(&self) -> Self { *self }
}


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

// hci.h
const HCI_FILTER: i32 = 2;
const HCI_EVENT_PKT: i32 = 0x04;
const HCI_LE_META_EVENT: i32 = 0x3E;
const HCI_EVENT_HDR_SIZE: i32 = 2;

// bluetooth.h
const SOL_HCI: i32 = 0;

// local
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

impl ConnectedAdapter {
    pub fn devices(&mut self) -> nix::Result<Vec<Device>> {
        let mut nf = HCIFilter::default();
        nf.set_ptype(HCI_EVENT_PKT);
        nf.set_event(HCI_LE_META_EVENT);

        unsafe {
            let nf_ptr: *mut c_void = &mut nf as *mut _ as *mut c_void;
            handle_error(setsockopt(self.dd, SOL_HCI, HCI_FILTER, nf_ptr,
                                    mem::size_of_val(&nf) as u32))?;
        }

        let mut stream = unsafe { UnixStream::from_raw_fd(self.dd) };

        let mut devices: Vec<Device> = vec![];
        loop {
            let mut buf = [0u8; 260];
            let mut idx = 1usize + HCI_EVENT_HDR_SIZE as usize;
            let len = stream.read(&mut buf).unwrap();

            let sub_event = buf[idx];
            idx += 1;

            if sub_event != 2 {
                break;
            }

            idx += 1;
            let info: LeAdvertisingInfo = deserialize(&buf[idx..len]).unwrap();
            idx += mem::size_of_val(&info);

            let data: Vec<u8> = buf[idx..idx + info.length as usize].to_vec();
            let name = parse_name(data);

            devices.push(Device {
                addr: info.bdaddr,
                name
            })
        }

        Ok(devices)
    }

    pub fn print_devices(&mut self) -> nix::Result<()> {
        println!("{:#?}", self.devices()?);
        Ok(())
    }
}
