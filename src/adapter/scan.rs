use std::io::prelude::*;
use std::mem;
use std::os::unix::io::FromRawFd;
use std::os::unix::net::UnixStream;

use bincode::{deserialize, Infinite};

use libc::{setsockopt, c_void, memcpy};

use nix;

use serde_derive;

use ::util::handle_error;
use ::adapter::{BDAddr, ConnectedAdapter};

#[link(name = "bluetooth")]
extern {
    fn ba2str(ba : *const BDAddr, str : *mut u8) -> i32;

    // fn hci_read_bd_addr(dd: i32, bdaddr: *const BDAddr, to: i32);
}

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


#[derive(Copy, Deserialize)]
#[repr(C)]
pub struct EvtLeMetaEvent {
    pub subevent : u8,
    pub data : [u8; 0],
}

impl Clone for EvtLeMetaEvent {
    fn clone(&self) -> Self { *self }
}

impl EvtLeMetaEvent {
    fn default() -> EvtLeMetaEvent {
        EvtLeMetaEvent { subevent: 0, data: [] }
    }
}

#[derive(Copy, Deserialize)]
#[repr(C)]
pub struct LeAdvertisingInfo {
    pub evt_type : u8,
    pub bdaddr_type : u8,
    pub bdaddr : BDAddr,
    pub length : u8,
    pub data : [u8; 0],
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

unsafe fn eir_parse_name(mut eir: *mut u8, eir_len: usize, buf: *mut u8, buf_size: usize) {
    let mut offset = 0usize;
    while offset < eir_len {
        let field_len = *eir.offset(0);
        // check for the end of EIR
        if field_len == 0 {
            break;
        }

        if offset + (field_len as usize) > eir_len {
            println!("Failed -- unknown");
            return;
        }

        let t = *eir.offset(1);
        if t == EIR_NAME_SHORT || t == EIR_NAME_COMPLETE {
            let name_len = field_len - 1;
            if name_len as usize > buf_size {
                print!("Failed -- too big");
                return;
            }

            memcpy(buf as (*mut c_void),
                   &mut *eir.offset(2) as (*mut u8) as (*const c_void),
                   name_len as usize);
            return;
        }

        offset = offset.wrapping_add(
            (field_len as (i32) + 1i32) as (usize)
        );
        eir = eir.offset((field_len as (i32) + 1i32) as (isize));
    }
}

impl ConnectedAdapter {
    pub fn print_devices(&mut self) -> nix::Result<()> {
        let mut nf = HCIFilter::default();
        nf.set_ptype(HCI_EVENT_PKT);
        nf.set_event(HCI_LE_META_EVENT);

        println!("{:#?}", nf);

        unsafe {
            let nf_ptr: *mut c_void = &mut nf as *mut _ as *mut c_void;
            handle_error(setsockopt(self.dd, SOL_HCI, HCI_FILTER, nf_ptr,
                                    mem::size_of_val(&nf) as u32))?;
        }

        let mut stream = unsafe { UnixStream::from_raw_fd(self.dd) };

        loop {
            let mut buf = [0u8; 260];
            let mut len = stream.read(&mut buf).unwrap() as i32;

            unsafe {
                let mut meta: *mut EvtLeMetaEvent;
                let mut info: *mut LeAdvertisingInfo;
                let mut addr = [0u8; 18];

                let ptr = buf.as_mut_ptr().offset((1 + HCI_EVENT_HDR_SIZE) as (isize));
                len = len - (1 + HCI_EVENT_HDR_SIZE);
                meta = ptr as (*mut ::std::os::raw::c_void) as (*mut EvtLeMetaEvent);
                if (*meta).subevent as i32 != 2 {
                    break;
                }

                info = (*meta).data.as_mut_ptr().offset(1) as (*mut LeAdvertisingInfo);
                let mut name = [0u8; 30];
                ba2str(&mut (*info).bdaddr, addr.as_mut_ptr());

                eir_parse_name(
                    (*info).data.as_mut_ptr(),
                    (*info).length as (usize),
                    name.as_mut_ptr(),
                    ::std::mem::size_of::<[u8; 30]>().wrapping_sub(1usize)
                );

                let addr_s = String::from_utf8_unchecked(addr.to_vec());
                let name_s = String::from_utf8_unchecked(name.to_vec());
                println!("{} {}\n", addr_s, name_s);
            }
        }

        Ok(())
    }
}
