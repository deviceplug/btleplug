mod scan;
mod parser;

use libc;
use std;
use libc::{c_char, c_void};
use std::ffi::CStr;
use nix;
use nom;
use nom::IResult;
use bytes::BufMut;

use std::io::Read;

use std::collections::{HashSet, HashMap};
use std::fmt;
use std::fmt::{Display, Debug, Formatter};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::os::unix::net::UnixStream;
use std::os::unix::io::{FromRawFd, AsRawFd};
use std::thread;
use std::thread::JoinHandle;
use std::boxed::Box;

use util::handle_error;
use manager::Callback;
use ::adapter::parser::{AdapterDecoder, Message};
use ::device::Device;


#[link(name = "bluetooth")]
extern {
    fn hci_open_dev(dev_id: i32) -> i32;

    fn hci_close_dev(dd: i32) -> i32;
}

#[derive(Debug, Copy)]
pub struct HCIDevReq {
    pub dev_id: u16,
    pub dev_opt: u32,
}

impl Clone for HCIDevReq {
    fn clone(&self) -> Self { *self }
}

#[derive(Copy, Serialize, Deserialize, Hash, Eq, PartialEq)]
#[repr(C)]
pub struct BDAddr {
    pub address: [ u8 ; 6usize ]
}

impl Clone for BDAddr {
    fn clone(&self) -> Self { *self }
}

impl Display for BDAddr {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let a = self.address;
        write!(f, "{:X}:{:X}:{:X}:{:X}:{:X}:{:X}",
               a[5], a[4], a[3], a[2], a[1], a[0])
    }
}

impl Debug for BDAddr {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        (self as &Display).fmt(f)
    }
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

impl HCIDevStats {
    fn default() -> HCIDevStats {
        HCIDevStats {
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
        }
    }
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

impl HCIDevInfo {
    pub fn default() -> HCIDevInfo {
        HCIDevInfo {
            dev_id: 0,
            name: [0i8; 8],
            bdaddr: BDAddr { address: [0u8; 6] },
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
            stat: HCIDevStats::default()
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub enum AdapterType {
    BrEdr,
    Amp,
    Unknown
}

impl AdapterType {
    fn parse(typ: u8) -> AdapterType {
        match typ {
            0 => AdapterType::BrEdr,
            1 => AdapterType::Amp,
            _ => AdapterType::Unknown,
        }
    }
}

#[derive(Hash, Eq, PartialEq, Debug, Copy, Clone)]
pub enum AdapterState {
    Up, Init, Running, Raw, PScan, IScan, Inquiry, Auth, Encrypt
}

impl AdapterState {
    fn parse(flags: u32) -> HashSet<AdapterState> {
        use self::AdapterState::*;

        let states = [Up, Init, Running, Raw, PScan, IScan, Inquiry, Auth, Encrypt];

        let mut set = HashSet::new();
        for (i, f) in states.iter().enumerate() {
            if flags & (1 << (i & 31)) != 0 {
                set.insert(f.clone());
            }
        }

        set
    }
}

#[derive(Clone)]
pub struct ConnectedAdapter {
    pub adapter: Adapter,
    stream: Arc<Mutex<UnixStream>>,
    should_stop: Arc<AtomicBool>,
    callbacks: Arc<Mutex<Vec<Callback>>>,
    pub discovered: Arc<Mutex<HashMap<BDAddr, Device>>>
}

impl ConnectedAdapter {
    pub fn new(adapter: &Adapter, callbacks: Vec<Callback>) -> nix::Result<ConnectedAdapter> {
        let mut stream = Arc::new(Mutex::new(unsafe {
            UnixStream::from_raw_fd(handle_error(hci_open_dev(adapter.dev_id as i32))?)
        }));

        let should_stop = Arc::new(AtomicBool::new(false));

        let connected = ConnectedAdapter {
            adapter: adapter.clone(),
            stream,
            should_stop,
            callbacks: Arc::new(Mutex::new(callbacks)),
            discovered: Arc::new(Mutex::new(HashMap::new()))
        };

        {
            let should_stop = connected.should_stop.clone();
            let mut stream = connected.stream.clone();
            let connected = connected.clone();
            thread::spawn(move || {
                let mut buf = [0u8; 2048];
                let mut cur: Vec<u8> = vec![];

                let mut idx = 0;

                while !should_stop.load(Ordering::Relaxed) {
                    let len = {
                        stream.lock().unwrap().read(&mut buf).unwrap()
                    };

                    cur.put_slice(&buf[0..len]);

                    let mut new_cur: Option<Vec<u8>> = Some(vec![]);
                    {
                        let result = {
                            AdapterDecoder::decode(&cur)
                        };

                        match result {
                            IResult::Done(left, result) => {
                                ConnectedAdapter::handle(&connected, result);
                                if !left.is_empty() {
                                    new_cur = Some(left.to_owned());
                                };
                            }
                            IResult::Incomplete(needed) => {
                                new_cur = None;
                            },
                            IResult::Error(err) => {
                                error!("parse error {}", err);
                            }
                        }
                    };

                    cur = new_cur.unwrap_or(cur);
                }
            })
        };

        Ok(connected)
    }

    fn handle(&self, message: Message) {
        debug!("got message {:#?}", message);

        let mut discovered = self.discovered.lock().unwrap();
        match message {
            Message::LEAdvertisingReport(info) => {
                use ::adapter::parser::LEAdvertisingData::*;

                let mut device = discovered.entry(info.bdaddr)
                    .or_insert_with(|| Device::new(info.bdaddr));

                device.discovery_count += 1;

                if info.evt_type == 4 {
                    // discover event
                    device.has_scan_response = true;
                } else {
                    // TODO: reset service data
                }

                for datum in info.data {
                    match datum {
                        LocalName(name) => {
                            device.local_name = Some(name);
                        }
                        TxPowerLevel(power) => {
                            device.tx_power_level = Some(power);
                        }
                        ManufacturerSpecific(data) => {
                            device.manufacturer_data = Some(data);
                        }
                        _ => {
                            // skip for now
                        }
                    }
                }
            }
        }
    }
}

impl Drop for ConnectedAdapter {
    fn drop(&mut self) {
        self.should_stop.store(true, Ordering::Relaxed);
        // clean up
        debug!("cleaning up device");
        let fd = self.stream.lock().unwrap().as_raw_fd();
        unsafe {
            hci_close_dev(fd);
        }

    }
}

#[derive(Debug, Clone)]
pub struct Adapter {
    pub name: String,
    pub dev_id: u16,
    pub addr: BDAddr,
    pub typ: AdapterType,
    pub states: HashSet<AdapterState>,
}

// #define HCIGETDEVINFO	_IOR('H', 211, int)
static HCI_GET_DEV_MAGIC: usize = (2u32 << 0i32 + 8i32 + 8i32 + 14i32 |
    (b'H' as (i32) << 0i32 + 8i32) as (u32) | (211i32 << 0i32) as (u32)) as (usize) |
    4 /* (sizeof(i32)) */ << 0i32 + 8i32 + 8i32;

impl Adapter {
    pub fn from_device_info(di: &HCIDevInfo) -> Adapter {
        Adapter {
            name: String::from(unsafe { CStr::from_ptr(di.name.as_ptr()).to_str().unwrap() }),
            dev_id: 0,
            addr: di.bdaddr,
            typ: AdapterType::parse((di.type_ & 0x30) >> 4),
            states: AdapterState::parse(di.flags),
        }
    }

    pub fn from_dev_id(ctl: i32, dev_id: u16) -> nix::Result<Adapter> {
        let mut di = HCIDevInfo::default();
        di.dev_id = dev_id;

        unsafe {
            handle_error(libc::ioctl(ctl, HCI_GET_DEV_MAGIC as libc::c_ulong,
                                     &mut di as (*mut HCIDevInfo) as (*mut c_void)))?;
        }

        Ok(Adapter::from_device_info(&di))
    }

    pub fn is_up(&self) -> bool {
        self.states.contains(&AdapterState::Up)
    }

    pub fn connect(&self, callbacks: Vec<Callback>) -> nix::Result<ConnectedAdapter> {
        ConnectedAdapter::new(self, callbacks)
    }
}
