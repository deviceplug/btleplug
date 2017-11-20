extern crate libc;

use std::ptr;
use libc::{c_char, getsockopt, c_void};
use std::mem;
use std::ffi::CString;

#[repr(C)]
pub struct BDAddr {
    pub b : [ u8 ; 6usize ]
}

#[repr(C)]
#[derive(Default)]
pub struct HCIFilter {
    type_mask: u32,
    event_mask: [u32; 2usize],
    opcode: u16,
}


#[link(name = "bluetooth")]
extern {
    fn hci_get_route(bdaddr: *const BDAddr) -> i32;
    fn hci_open_dev(dev_id: i32) -> i32;

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

    fn hci_filter_clear(f: *mut HCIFilter);
    fn hci_filter_set_ptype(t: i32, f: *mut HCIFilter);
}

fn hci_set_bit(nr: i32, addr: *mut c_void) {
    *((uint32_t *) addr + (nr >> 5)) |= (1 << (nr & 31));
}


fn hci_filter_clear(f: &mut HCIFilter) {
    libc::memset(f, 0, mem::size_of_val(f));
}

fn hci_filter_set_ptype(t: i32, f: *mut HCIFilter) {
    hci_set_bit((t == HCI_VENDOR_PKT) ? 0 : (t & HCI_FLT_TYPE_BITS), &f->type_mask);
}

extern {
    fn perror(str: *const c_char);
}

// hci.h
static HCI_FILTER: i32 = 2;
static HCI_EVENT_PKT: i32 = 0x04;
static HCI_LE_META_EVENT: i32 = 0x3E;

// bluetooth.h
static SOL_HCI: i32 = 0;
static SOL_L2CAP: i32 = 6;
static SOL_SCO: i32 = 17;
static SOL_RFCOMM: i32 = 18;


unsafe fn print_devices(dd: i32, filter_type: u8) {
    let mut nf: HCIFilter = HCIFilter::default();
    let mut of: HCIFilter = HCIFilter::default();

    let mut olen = mem::size_of::<HCIFilter>() as u32;

    let of_ptr: *mut c_void = &mut of as *mut _ as *mut c_void;

    if getsockopt(dd, SOL_HCI, HCI_FILTER, of_ptr, &mut olen) < 0 {
        let s = CString::new("Failed to get sock opts").unwrap();
        perror(s.as_ptr());
    }

    hci_filter_clear(&mut nf);
    hci_filter_set_ptype(HCI_EVENT_PKT, &mut nf);

}

fn main() {
    unsafe {
        let dev_id = hci_get_route(ptr::null());
        let dd = hci_open_dev(dev_id);
        println!("dd {:?}", dd);

        let own_type: u8 = 0x00;
        let scan_type: u8 = 0x01;
        let filter_policy: u8 = 0x00;
        let interval: u16 = 0x0010;
        let window: u16 = 0x0010;
        let filter_dup: u8 = 1;
        let filter_type: u8 = 0;

        let e1 = hci_le_set_scan_parameters(dd, scan_type, interval, window,
                                             own_type, filter_policy, 1000);

        if e1 < 0 {
            let s = CString::new("Failed to set scan parameters").unwrap();
            perror(s.as_ptr());
        }

        let e2 = hci_le_set_scan_enable(dd, 1, filter_dup, 1000);
        if e2 < 0 {
            let s = CString::new("Failed to enable scan").unwrap();
            perror(s.as_ptr());
        }

        print_devices(dd, filter_type);
    };

}
