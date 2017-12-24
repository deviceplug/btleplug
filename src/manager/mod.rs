use std::sync::Mutex;

use libc;
use libc::{c_void, socket, SOCK_RAW};
use std::mem;
use nix;

use util::handle_error;
use adapter::{Adapter, HCIDevReq, ConnectedAdapter};
use device::Device;

pub const AF_BLUETOOTH: i32 = 31;
const BTPROTO_HCI: i32 = 1;

// #define HCIGETDEVLIST	_IOR('H', 210, int)
static HCI_GET_DEV_LIST_MAGIC: usize = (2u32 << 0i32 + 8i32 + 8i32 + 14i32 |
    (b'H' as (i32) << 0i32 + 8i32) as (u32) | (210i32 << 0i32) as (u32)) as
    (usize) | 4 /* (sizeof(i32)) */ << 0i32 + 8i32 + 8i32;

// #define HCIDEVUP	_IOW('H', 201, int)
ioctl!(write_int hci_dev_up with b'H', 201);
// #define HCIDEVDOWN	_IOW('H', 202, int)
ioctl!(write_int hci_dev_down with b'H', 202);

pub enum Event {
    AdapterEnabled(Adapter),
    AdapterDisabled(Adapter),
    DeviceDiscovered(Device),
    DeviceConnected(Device),
    DeviceDisconnected(Device),
}

pub type Callback = fn (Event) -> ();

#[derive(Copy)]
#[repr(C)]
struct HCIDevListReq {
    dev_num: u16,
    dev_reqs: [HCIDevReq; 0],
}

impl Clone for HCIDevListReq {
    fn clone(&self) -> Self { *self }
}


pub struct Manager {
    ctl_fd: Mutex<i32>
}

impl Manager {
    pub fn new() -> nix::Result<Manager> {
        let fd = handle_error(unsafe { socket(AF_BLUETOOTH, SOCK_RAW, BTPROTO_HCI) })?;
        Ok(Manager { ctl_fd: Mutex::new(fd) })
    }

    pub fn adapters(&self) -> nix::Result<Vec<Adapter>> {
        let mut result: Vec<Adapter> = vec![];

        let ctl = self.ctl_fd.lock().unwrap();

        let mut buf = vec![0u8; 16usize *
            mem::size_of::<HCIDevReq>() + mem::size_of::<u16>()];
        let dl: *mut HCIDevListReq = buf.as_mut_ptr() as (*mut HCIDevListReq);
        let dr: *mut HCIDevReq;

        unsafe {
            (*dl).dev_num = 16u16;
            dr = (*dl).dev_reqs.as_mut_ptr();

            handle_error(
                libc::ioctl(*ctl, HCI_GET_DEV_LIST_MAGIC as libc::c_ulong, dl as (*mut c_void)))?;

            for i in 0..(*dl).dev_num {
                result.push(Adapter::from_dev_id(*ctl,
                                                 (*dr.offset(i as isize)).dev_id)?);
            }
        }

        Ok(result)
    }

    pub fn update(&self, adapter: &Adapter) -> nix::Result<Adapter> {
        let ctl = self.ctl_fd.lock().unwrap();
        Adapter::from_dev_id(*ctl, adapter.dev_id)
    }

    pub fn down(&self, adapter: &Adapter) -> nix::Result<Adapter> {
        let ctl = self.ctl_fd.lock().unwrap();
        unsafe {
            hci_dev_down(*ctl, adapter.dev_id as i32)?;
        }
        Adapter::from_dev_id(*ctl, adapter.dev_id)
    }

    pub fn up(&self, adapter: &Adapter) -> nix::Result<Adapter> {
        let ctl = self.ctl_fd.lock().unwrap();
        unsafe {
            hci_dev_up(*ctl, adapter.dev_id as i32)?;
        }
        Adapter::from_dev_id(*ctl, adapter.dev_id)
    }

    pub fn connect(&self, adapter: &Adapter, callbacks: Vec<Callback>) -> nix::Result<ConnectedAdapter> {
        ConnectedAdapter::new(adapter, callbacks)
    }
}
