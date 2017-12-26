use std::mem;
use std::io;

use libc::{setsockopt, c_void, write};

use nix;
use bytes::{BytesMut, BufMut, LittleEndian};

use ::util::handle_error;
use ::adapter::ConnectedAdapter;
use ::adapter::protocol::Protocol;
use ::constants::*;


fn hci_set_bit(nr: i32, cur: i32) -> i32 {
    cur | 1 << (nr & 31)
}

#[link(name = "bluetooth")]
extern {
    fn hci_le_set_scan_parameters(dev_id: i32,
                                  typ : u8,
                                  interval: u16,
                                  window: u16,
                                  own_type: u8,
                                  filter: u8,
                                  time_out: i32) -> i32;

    fn hci_le_set_scan_enable(dev_id: i32,
                              enable: u8,
                              filter_dup: u8,
                              time_out: i32) -> i32;
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
        self.type_mask = hci_set_bit(nr, self.type_mask as i32) as u32
    }

    fn set_event(&mut self, event: i32) {
        let byte = (event >> 5) as usize;
        if byte > self.event_mask.len() {
            panic!("input too large");
        }

        self.event_mask[byte] = hci_set_bit(event & 63,
                                            self.event_mask[byte] as i32) as u32;
    }
}

impl Clone for HCIFilter {
    fn clone(&self) -> Self { *self }
}

impl ConnectedAdapter {
    fn set_scan_params(&self) -> nix::Result<()> {
        let mut data = BytesMut::with_capacity(7);
        data.put_u8(1); // scan_type = active
        data.put_u16::<LittleEndian>(0x0010); // interval ms
        data.put_u16::<LittleEndian>(0x0010); // window ms
        data.put_u8(0); // own_type = public
        data.put_u8(0); // filter_policy = public
        let mut buf = Protocol::hci(LE_SET_SCAN_PARAMETERS_CMD, &*data);
        Protocol::write(self.adapter_fd, &mut *buf)
    }

    fn set_scan_enabled(&self, enabled: bool) -> nix::Result<()> {
        let mut data = BytesMut::with_capacity(2);
        data.put_u8(if enabled { 1 } else { 0 }); // enabled
        data.put_u8(1); // filter duplicates

        let mut buf = Protocol::hci(LE_SET_SCAN_ENABLE_CMD, &*data);
        Protocol::write(self.adapter_fd, &mut *buf)
    }

    pub fn start_scan(&self) -> nix::Result<()> {
        // self.set_scan_enabled(false).unwrap();
        self.set_scan_params().unwrap();
        self.set_scan_enabled(true)
    }
}
