use std::slice::Iter;
use std::iter::Take;
use std::sync::Mutex;

use libc;
use libc::{SOCK_RAW, AF_BLUETOOTH};
use nix::sys::ioctl::ioctl_param_type;
use std::mem;

use bluez::util::handle_error;
use bluez::adapter::{Adapter, ConnectedAdapter};
use bluez::constants::*;
use bluez::ioctl;
use ::Result;

#[derive(Debug, Copy)]
#[repr(C)]
pub struct HCIDevReq {
    pub dev_id: u16,
    pub dev_opt: u32,
}

impl Clone for HCIDevReq {
    fn clone(&self) -> Self { *self }
}

impl Default for HCIDevReq {
    fn default() -> Self {
        HCIDevReq {
            dev_id: 0,
            dev_opt: 0,
        }
    }
}

#[derive(Copy)]
#[repr(C)]
pub struct HCIDevListReq {
    dev_num: u16,
    dev_reqs: [HCIDevReq; 16],
}

impl HCIDevListReq {
    pub fn iter(&self) -> Take<Iter<HCIDevReq>> {
        self.dev_reqs.iter().take(self.dev_num as usize)
    }
}

impl Clone for HCIDevListReq {
    fn clone(&self) -> Self { *self }
}

impl Default for HCIDevListReq {
    fn default() -> Self {
        HCIDevListReq {
            dev_num: 16u16,
            dev_reqs: unsafe { mem::zeroed() },
        }
    }
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

        let mut dev_list = HCIDevListReq::default();

        unsafe {
            ioctl::hci_get_dev_list(*ctl, &mut dev_list)?;
        }

        for dev_req in dev_list.iter() {
            let adapter = Adapter::from_dev_id(*ctl, dev_req.dev_id)?;
            result.push(adapter);
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
        unsafe { ioctl::hci_dev_down(*ctl, adapter.dev_id as ioctl_param_type)? };
        Adapter::from_dev_id(*ctl, adapter.dev_id)
    }

    /// Enables an adapater.
    pub fn up(&self, adapter: &Adapter) -> Result<Adapter> {
        let ctl = self.ctl_fd.lock().unwrap();
        unsafe {
            ioctl::hci_dev_up(*ctl, adapter.dev_id as ioctl_param_type)?;
        }
        Adapter::from_dev_id(*ctl, adapter.dev_id)
    }

    /// Establishes a connection to an adapter. Returns a `ConnectedAdapter`, which is the
    /// [`Central`](../../api/trait.Central.html) implementation for BlueZ.
    pub fn connect(&self, adapter: &Adapter) -> Result<ConnectedAdapter> {
        ConnectedAdapter::new(adapter)
    }
}
