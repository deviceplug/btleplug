use std::io::Read;
use std::mem;
use std::os::unix::io::{FromRawFd, AsRawFd};
use std::os::unix::net::UnixStream;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::thread::JoinHandle;
use std::boxed::Box;
use std::collections::VecDeque;

use bincode::deserialize;

use libc::{setsockopt, c_void};

use nix;

use ::util::handle_error;
use ::adapter::{BDAddr, Adapter};
use ::adapter::parser::AdapterDecoder;
use ::device::Device;

fn hci_set_bit(nr: i32, cur: i32) -> i32 {
    cur | 1 << (nr & 31)
}

#[link(name = "bluetooth")]
extern {
    fn hci_open_dev(dev_id: i32) -> i32;

    fn hci_close_dev(dd: i32) -> i32;

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


// hci.h
const HCI_FILTER: i32 = 2;
const HCI_EVENT_PKT: i32 = 0x04;
const HCI_LE_META_EVENT: i32 = 0x3E;

// bluetooth.h
const SOL_HCI: i32 = 0;

// local

type DeviceCallback = fn (Device) -> ();

pub struct DeviceScanner {
    devices: Arc<Mutex<Vec<Device>>>,
    should_stop: Arc<AtomicBool>,
    handle: Box<JoinHandle<()>>,
}

impl DeviceScanner {
    pub fn start(adapter: Adapter, callback: Option<DeviceCallback>)
        -> nix::Result<DeviceScanner> {
        let own_type: u8 = 0x00;
        let scan_type: u8 = 0x01;
        let filter_policy: u8 = 0x00;
        let interval: u16 = 0x0010;
        let window: u16 = 0x0010;
        let filter_dup: u8 = 1;

        let mut nf = HCIFilter::default();
        nf.set_ptype(HCI_EVENT_PKT);
        nf.set_event(HCI_LE_META_EVENT);

        let devices: Arc<Mutex<Vec<Device>>> = Arc::new(Mutex::new(vec![]));
        let should_stop = Arc::new(AtomicBool::new(false));

        let mut stream = unsafe {
            let fd = handle_error(hci_open_dev(adapter.dev_id as i32))?;

            // start scan
            handle_error(hci_le_set_scan_parameters(
                fd, scan_type, interval, window,
                own_type, filter_policy, 10_000))?;

            handle_error(
                hci_le_set_scan_enable(fd, 1, filter_dup, 10_000))?;


            let nf_ptr: *mut c_void = &mut nf as *mut _ as *mut c_void;
            handle_error(setsockopt(fd, SOL_HCI, HCI_FILTER, nf_ptr,
                                    mem::size_of_val(&nf) as u32))?;
            UnixStream::from_raw_fd(fd)
        };

        let handle = {
            let devices = devices.clone();
            let should_stop = should_stop.clone();
            thread::spawn(move || {
                let mut buf = [0u8; 2048];

                let mut vd: VecDequeue<u8> = VecDeque::new();
                let mut idx = 0;

                while !should_stop.load(Ordering::Relaxed) {
                    let len = stream.read(&mut buf).unwrap();

                    let result = AdapterDecoder::decode(&buf[0..len]).unwrap();
                    if let Some((event, i)) = result {
                        idx = i;
                    }
                    // devices.lock().unwrap().push(device);
                }

                // clean up
                debug!("cleaning up device");
                let fd = stream.as_raw_fd();
                drop(stream);
                unsafe {
                    hci_close_dev(fd);
                }
            })
        };

        Ok(DeviceScanner {
            devices,
            should_stop,
            handle: Box::new(handle),
        })
    }

    pub fn devices(&self) -> Vec<Device> {
        (*self.devices.lock().unwrap()).to_vec()
    }
}

impl Drop for DeviceScanner {
    fn drop(&mut self) {
        self.should_stop.store(true, Ordering::Relaxed);
    }
}

impl Adapter {
    pub fn scanner(self, cb: Option<DeviceCallback>) -> nix::Result<DeviceScanner> {
        DeviceScanner::start(self, cb)
    }
}
