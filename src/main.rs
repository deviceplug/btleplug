extern crate libc;
#[macro_use] extern crate nix;

use std::ptr;
use libc::{c_char, getsockopt, setsockopt, c_void, memcpy, socket, SOCK_RAW};
use std::mem;
use std::ffi::{CString, CStr};
use nix::sys::ioctl;
use nix::Errno;

#[derive(Copy)]
#[derive(Debug)]
#[repr(C)]
pub struct BDAddr {
    pub b : [ u8 ; 6usize ]
}

impl Clone for BDAddr {
    fn clone(&self) -> Self { *self }
}

#[derive(Copy)]
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
            event_mask: [0, 0],
            opcode: 0
        };
    }
}

impl Clone for HCIFilter {
    fn clone(&self) -> Self { *self }
}

#[derive(Copy)]
#[derive(Debug)]
#[repr(C)]
pub struct HCIDevReq {
    dev_id: u16,
    dev_opt: u32,
}

impl Clone for HCIDevReq {
    fn clone(&self) -> Self { *self }
}


#[derive(Copy)]
#[repr(C)]
pub struct HCIDevListReq {
    dev_num: u16,
    dev_reqs: [HCIDevReq; 0],
}

impl Clone for HCIDevListReq {
    fn clone(&self) -> Self { *self }
}


#[derive(Copy)]
#[repr(C)]
pub struct HCIDevStats {
    pub err_rx : u32,
    pub err_tx : u32,
    pub cmd_tx : u32,
    pub evt_rx : u32,
    pub acl_tx : u32,
    pub acl_rx : u32,
    pub sco_tx : u32,
    pub sco_rx : u32,
    pub byte_rx : u32,
    pub byte_tx : u32,
}

impl Clone for HCIDevStats{
    fn clone(&self) -> Self { *self }
}

#[derive(Copy)]
#[repr(C)]
pub struct HCIDevInfo {
    pub dev_id : u16,
    pub name : [c_char; 8],
    pub bdaddr : BDAddr,
    pub flags : u32,
    pub type_ : u8,
    pub features : [u8; 8],
    pub pkt_type : u32,
    pub link_policy : u32,
    pub link_mode : u32,
    pub acl_mtu : u16,
    pub acl_pkts : u16,
    pub sco_mtu : u16,
    pub sco_pkts : u16,
    pub stat : HCIDevStats,
}

impl Clone for HCIDevInfo {
    fn clone(&self) -> Self { *self }
}


#[derive(Copy)]
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

#[derive(Copy)]
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

extern {
    fn memset(
        __s : *mut ::std::os::raw::c_void, __c : i32, __n : usize
    ) -> *mut ::std::os::raw::c_void;

    fn read(
        __fd : i32, __buf : *mut ::std::os::raw::c_void, __nbytes : usize
    ) -> isize;

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

    fn ba2str(ba : *const BDAddr, str : *mut u8) -> i32;

    fn hci_read_bd_addr(dd: i32, bdaddr: *const BDAddr, to: i32);
}
#[no_mangle]
pub unsafe extern fn hci_set_bit(
    mut nr : i32, mut addr : *mut ::std::os::raw::c_void
) {
    let _rhs = 1i32 << (nr & 31i32);
    let _lhs
    = &mut *(addr as (*mut u32)).offset((nr >> 5i32) as (isize));
    *_lhs = *_lhs | _rhs as (u32);
}

#[no_mangle]
pub unsafe extern fn hci_clear_bit(
    mut nr : i32, mut addr : *mut ::std::os::raw::c_void
) {
    let _rhs = !(1i32 << (nr & 31i32));
    let _lhs
    = &mut *(addr as (*mut u32)).offset((nr >> 5i32) as (isize));
    *_lhs = *_lhs & _rhs as (u32);
}

#[no_mangle]
pub unsafe extern fn hci_test_bit(
    mut nr : i32, mut addr : *mut ::std::os::raw::c_void
) -> i32 {
    (*(addr as (*mut u32)).offset(
        (nr >> 5i32) as (isize)
    ) & (1i32 << (nr & 31i32)) as (u32)) as (i32)
}

#[no_mangle]
pub unsafe extern fn hci_filter_clear(mut f : *mut HCIFilter) {
    memset(
        f as (*mut ::std::os::raw::c_void),
        0i32,
        ::std::mem::size_of::<HCIFilter>()
    );
}

#[no_mangle]
pub unsafe extern fn hci_filter_set_ptype(
    mut t : i32, mut f : *mut HCIFilter
) {
    hci_set_bit(
        if t == 0xffi32 { 0i32 } else { t & 31i32 },
        &mut (*f).type_mask as (*mut u32) as (*mut ::std::os::raw::c_void)
    );
}

#[no_mangle]
pub unsafe extern fn hci_filter_clear_ptype(
    mut t : i32, mut f : *mut HCIFilter
) {
    hci_clear_bit(
        if t == 0xffi32 { 0i32 } else { t & 31i32 },
        &mut (*f).type_mask as (*mut u32) as (*mut ::std::os::raw::c_void)
    );
}

#[no_mangle]
pub unsafe extern fn hci_filter_test_ptype(
    mut t : i32, mut f : *mut HCIFilter
) -> i32 {
    hci_test_bit(
        if t == 0xffi32 { 0i32 } else { t & 31i32 },
        &mut (*f).type_mask as (*mut u32) as (*mut ::std::os::raw::c_void)
    )
}

#[no_mangle]
pub unsafe extern fn hci_filter_all_ptypes(mut f : *mut HCIFilter) {
    memset(
        &mut (*f).type_mask as (*mut u32) as (*mut ::std::os::raw::c_void),
        0xffi32,
        ::std::mem::size_of::<u32>()
    );
}

#[no_mangle]
pub unsafe extern fn hci_filter_set_event(
    mut e : i32, mut f : *mut HCIFilter
) {
    hci_set_bit(
        e & 63i32,
        &mut (*f).event_mask as (*mut [u32; 2]) as (*mut ::std::os::raw::c_void)
    );
}

#[no_mangle]
pub unsafe extern fn hci_filter_clear_event(
    mut e : i32, mut f : *mut HCIFilter
) {
    hci_clear_bit(
        e & 63i32,
        &mut (*f).event_mask as (*mut [u32; 2]) as (*mut ::std::os::raw::c_void)
    );
}

#[no_mangle]
pub unsafe extern fn hci_filter_test_event(
    mut e : i32, mut f : *mut HCIFilter
) -> i32 {
    hci_test_bit(
        e & 63i32,
        &mut (*f).event_mask as (*mut [u32; 2]) as (*mut ::std::os::raw::c_void)
    )
}

#[no_mangle]
pub unsafe extern fn hci_filter_all_events(mut f : *mut HCIFilter) {
    memset(
        (*f).event_mask.as_mut_ptr() as (*mut ::std::os::raw::c_void),
        0xffi32,
        ::std::mem::size_of::<[u32; 2]>()
    );
}

#[no_mangle]
pub unsafe extern fn hci_filter_set_opcode(
    mut opcode : i32, mut f : *mut HCIFilter
) {
    (*f).opcode = opcode as (u16);
}

#[no_mangle]
pub unsafe extern fn hci_filter_clear_opcode(mut f : *mut HCIFilter) {
    (*f).opcode = 0u16;
}

#[no_mangle]
pub unsafe extern fn hci_filter_test_opcode(
    mut opcode : i32, mut f : *mut HCIFilter
) -> i32 {
    ((*f).opcode as (i32) == opcode) as (i32)
}

extern {
    fn perror(str: *const c_char);
}

// hci.h
const HCI_MAX_DEV: usize = 16;
const HCI_FILTER: i32 = 2;
const HCI_EVENT_PKT: i32 = 0x04;
const HCI_LE_META_EVENT: i32 = 0x3E;
const HCI_EVENT_HDR_SIZE: i32 = 2;

// bluetooth.h
const SOL_HCI: i32 = 0;
const SOL_L2CAP: i32 = 6;
const SOL_SCO: i32 = 17;
const SOL_RFCOMM: i32 = 18;

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

unsafe fn print_devices(dd: i32, filter_type: u8) {
    let mut nf = HCIFilter::default();
    let mut of = HCIFilter::default();

    let mut olen = mem::size_of::<HCIFilter>() as u32;

    let of_ptr: *mut c_void = &mut of as *mut _ as *mut c_void;

    if getsockopt(dd, SOL_HCI, HCI_FILTER, of_ptr, &mut olen) < 0 {
        let s = CString::new("Failed to get sock opts").unwrap();
        perror(s.as_ptr());
    }

    hci_filter_clear(&mut nf);
    hci_filter_set_ptype(HCI_EVENT_PKT, &mut nf);
    hci_filter_set_event(HCI_LE_META_EVENT, &mut nf);

    let nf_ptr: *mut c_void = &mut nf as *mut _ as *mut c_void;

    if setsockopt(dd, SOL_HCI, HCI_FILTER, nf_ptr, mem::size_of_val(&nf) as u32) < 0 {
        let s = CString::new("Failed to get set opts").unwrap();
        perror(s.as_ptr());
    }

    let mut buf: [u8; 260] = std::mem::uninitialized();
    loop {
        let mut meta : *mut EvtLeMetaEvent;
        let mut info : *mut LeAdvertisingInfo;
        let mut addr = [0u8; 18];
        let mut len : i32;
        loop {
            if !({
                len = read(
                    dd,
                    buf.as_mut_ptr() as (*mut ::std::os::raw::c_void),
                    ::std::mem::size_of::<[u8; 260]>()
                ) as (i32);
                len
            } < 0i32) {
                break;
            }
        }

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

// #define HCIDEVUP	_IOW('H', 201, int)
// #define HCIDEVDOWN	_IOW('H', 202, int)

static AF_BLUETOOTH: i32 = 31;
static BTPROTO_HCI: i32 = 1;

ioctl!(write_int hci_dev_up with b'H', 201);
ioctl!(write_int hci_dev_down with b'H', 202);

unsafe fn reset(dev_id: i32) {
    //let mut addr: BDAddr = std::mem::uninitialized();
    //hci_read_bd_addr(dd, &mut addr, 1000);

    let ctl: i32 = socket(AF_BLUETOOTH, SOCK_RAW, BTPROTO_HCI);
    println!("CTL = {}", ctl);
    if ctl < 0 {
        let s = CString::new("Failed to down device").unwrap();
        perror(s.as_ptr());
        panic!("failed");
    }

    println!("ctl: {}", ctl);

    hci_dev_down(ctl, dev_id).unwrap();
    hci_dev_up(ctl, dev_id).unwrap();
}

struct Adapter {
    dev_id: i32,
    name: String
}

fn handle_error(v: i32) -> nix::Result<i32> {
    if v < 0 {
        Err(nix::Error::Sys(Errno::last()))
    } else {
        Ok(v)
    }
}

fn get_control_socket() -> nix::Result<i32> {
    handle_error(unsafe { socket(AF_BLUETOOTH, SOCK_RAW, BTPROTO_HCI) })
}

// #define HCIGETDEVLIST	_IOR('H', 210, int)
ioctl!(read hci_get_dev_list with b'H', 210; i32);
// #define HCIGETDEVINFO	_IOR('H', 211, int)
ioctl!(read hci_get_dev_info with b'H', 211; i32);


static HCI_GET_DEV_LIST_MAGIC: usize = (2u32 << 0i32 + 8i32 + 8i32 + 14i32 |
    (b'H' as (i32) << 0i32 + 8i32) as (u32) | (210i32 << 0i32) as (u32)) as
    (usize) | 4 /* (sizeof(i32)) */ << 0i32 + 8i32 + 8i32;

static HCI_GET_DEV_MAGIC: usize = (2u32 << 0i32 + 8i32 + 8i32 + 14i32 |
    (b'H' as (i32) << 0i32 + 8i32) as (u32) | (211i32 << 0i32) as (u32)) as (usize) |
    4 /* (sizeof(i32)) */ << 0i32 + 8i32 + 8i32;

fn get_adapters() -> nix::Result<Vec<Adapter>> {
    let result: Vec<Adapter> = vec![];

    let ctl = get_control_socket()?;

    println!("ctl = {}", ctl);

    unsafe {
        let mut dl: *mut HCIDevListReq;
        let mut dr: *mut HCIDevReq;



        dl = libc::malloc(
            16usize.wrapping_mul(
                ::std::mem::size_of::<HCIDevReq>()
            ).wrapping_add(
                ::std::mem::size_of::<u16>()
            )) as (*mut HCIDevListReq);

        (*dl).dev_num = 16u16;
        dr = (*dl).dev_reqs.as_mut_ptr();

        if dl.is_null() {
            panic!("Failed to allocate memory");
        }
        handle_error(libc::ioctl(ctl, HCI_GET_DEV_LIST_MAGIC as libc::c_ulong, dl as (*mut c_void)))?;

        println!("size {}", (*dl).dev_num);

        for i in 0..(*dl).dev_num {
            let mut di = HCIDevInfo {
                dev_id: (*dr.offset(i as isize)).dev_id,
                name: [0i8; 8],
                bdaddr: BDAddr { b: [0u8; 6] },
                flags: 0u32,
                type_: 0u8,
                features: [0u8; 8],
                pkt_type: 0u32,
                link_policy: 0u32,
                link_mode: 0u32,
                acl_mtu: 0u16,
                acl_pkts: 0u16,
                sco_mtu: 0u16,
                sco_pkts: 0u16,
                stat: HCIDevStats {
                    err_rx: 0u32,
                    err_tx: 0u32,
                    cmd_tx: 0u32,
                    evt_rx: 0u32,
                    acl_tx: 0u32,
                    acl_rx: 0u32,
                    sco_tx: 0u32,
                    sco_rx: 0u32,
                    byte_rx: 0u32,
                    byte_tx: 0u32
                }};

            handle_error(libc::ioctl(ctl, HCI_GET_DEV_MAGIC as libc::c_ulong,
                                     &mut di as (*mut HCIDevInfo) as (*mut c_void)))?;

            println!("{:?}", CStr::from_ptr(di.name.as_ptr()));
        }
    };

    Ok(result)
}

fn main() {
    get_adapters().unwrap();

//    unsafe {
//
//        let dev_id = hci_get_route(ptr::null());
//        let dd = hci_open_dev(dev_id);
//        println!("dd {:?}", dd);
//
//        reset(dev_id);
//
//        let own_type: u8 = 0x00;
//        let scan_type: u8 = 0x01;
//        let filter_policy: u8 = 0x00;
//        let interval: u16 = 0x0010;
//        let window: u16 = 0x0010;
//        let filter_dup: u8 = 1;
//        let filter_type: u8 = 0;
//
//        let e1 = hci_le_set_scan_parameters(dd, scan_type, interval, window,
//                                             own_type, filter_policy, 1000);
//
//        if e1 < 0 {
//            let s = CString::new("Failed to set scan parameters").unwrap();
//            perror(s.as_ptr());
//        }
//
//        let e2 = hci_le_set_scan_enable(dd, 1, filter_dup, 1000);
//        if e2 < 0 {
//            let s = CString::new("Failed to enable scan").unwrap();
//            perror(s.as_ptr());
//        }
//
//        print_devices(dd, filter_type);
//    };

}
