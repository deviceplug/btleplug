use std::sync::Mutex;

use libc;
use libc::{c_void, SOCK_RAW, AF_BLUETOOTH};
use std::mem;

use bluez::util::handle_error;
use bluez::adapter::{Adapter, ConnectedAdapter};
use bluez::constants::*;
use ::Result;

struct HciIoctls {}

// in a private struct to hide
impl HciIoctls {
    // #define HCIDEVUP	_IOW('H', 201, int)
    ioctl!(write_int hci_dev_up with b'H', 201);
    // #define HCIDEVDOWN	_IOW('H', 202, int)
    ioctl!(write_int hci_dev_down with b'H', 202);
}

#[derive(Debug, Copy)]
#[repr(C)]
struct HCIDevReq {
    pub dev_id: u16,
    pub dev_opt: u32,
}

impl Clone for HCIDevReq {
    fn clone(&self) -> Self { *self }
}

#[derive(Copy)]
#[repr(C)]
struct HCIDevListReq {
    dev_num: u16,
    dev_reqs: [HCIDevReq; 0],
}

impl Clone for HCIDevListReq {
    fn clone(&self) -> Self { *self }
}


/// This struct is the interface into BlueZ. It can be used to list, manage, and connect to bluetooth
/// adapters.
pub struct Manager {
    ctl_fd: Mutex<i32>
}

impl Manager {
    /// Constructs a new manager to communicate with the BlueZ system. Only one Manager should be
    /// created by your application.
    pub fn new() -> Result<Manager> {
        let fd = handle_error(unsafe { libc::socket(AF_BLUETOOTH, SOCK_RAW, BTPROTO_HCI) })?;
        Ok(Manager { ctl_fd: Mutex::new(fd) })
    }

    /// Returns the list of adapters available on the system.
    pub fn adapters(&self) -> Result<Vec<Adapter>> {
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

    /// Updates the state of an adapter.
    pub fn update(&self, adapter: &Adapter) -> Result<Adapter> {
        let ctl = self.ctl_fd.lock().unwrap();
        Adapter::from_dev_id(*ctl, adapter.dev_id)
    }

    /// Disables an adapter.
    pub fn down(&self, adapter: &Adapter) -> Result<Adapter> {
        let ctl = self.ctl_fd.lock().unwrap();
        unsafe { HciIoctls::hci_dev_down(*ctl, adapter.dev_id as i32)? };
        Adapter::from_dev_id(*ctl, adapter.dev_id)
    }

    /// Enables an adapater.
    pub fn up(&self, adapter: &Adapter) -> Result<Adapter> {
        let ctl = self.ctl_fd.lock().unwrap();
        unsafe {
            HciIoctls::hci_dev_up(*ctl, adapter.dev_id as i32)?;
        }
        Adapter::from_dev_id(*ctl, adapter.dev_id)
    }

    /// Establishes a connection to an adapter. Returns a `ConnectedAdapter`, which is the
    /// [`Central`](../../api/trait.Central.html) implementation for BlueZ.
    pub fn connect(&self, adapter: &Adapter) -> Result<ConnectedAdapter> {
        ConnectedAdapter::new(adapter)
    }
}
