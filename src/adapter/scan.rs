use std::io::Read;
use std::mem;
use std::os::unix::io::{FromRawFd, AsRawFd};
use std::os::unix::net::UnixStream;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::thread::JoinHandle;
use std::boxed::Box;

use bincode::deserialize;

use libc::{setsockopt, c_void};

use nix;

use ::util::handle_error;
use ::adapter::{BDAddr, Adapter};
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
                while !should_stop.load(Ordering::Relaxed) {
                    let mut buf = [0u8; 260];
                    let mut idx = 1usize + HCI_EVENT_HDR_SIZE as usize;
                    let len = stream.read(&mut buf).unwrap();

                    let sub_event = buf[idx];
                    idx += 1;

                    if sub_event != 2 {
                        // TODO: what to do about this?
                        break;
                    }

                    idx += 1;
                    let info: LeAdvertisingInfo = deserialize(&buf[idx..len]).unwrap();
                    idx += mem::size_of_val(&info);

                    let data: Vec<u8> = buf[idx..idx + info.length as usize].to_vec();
                    let name = parse_name(data);

                    let device = Device {
                        addr: info.bdaddr,
                        name
                    };

                    if let Some(cb) = callback {
                        cb(device.clone());
                    }

                    devices.lock().unwrap().push(device);
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
