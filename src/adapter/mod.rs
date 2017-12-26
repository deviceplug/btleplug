mod scan;
mod parser;
mod protocol;

use libc;
use std;
use libc::*;
use std::ffi::CStr;
use nix;
use nom::IResult;
use bytes::{BytesMut, BufMut, LittleEndian};

use std::io::{Read, Write};

use std::collections::{HashSet, HashMap};
use std::fmt;
use std::fmt::{Display, Debug, Formatter};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::os::unix::net::UnixStream;
use std::os::unix::io::FromRawFd;
use std::thread;
use std::mem::size_of;

use util::handle_error;
use manager::Callback;
use ::adapter::parser::{AdapterDecoder, Message};
use ::device::Device;
use ::constants::*;


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

#[derive(Copy, Debug)]
#[repr(C)]
pub struct SockaddrHCI {
    hci_family: sa_family_t,
    hci_dev: u16,
    hci_channel: u16,
}

impl Clone for SockaddrHCI {
    fn clone(&self) -> Self { *self }
}


#[derive(Copy, Debug)]
#[repr(C)]
pub struct SockaddrL2 {
    l2_family: sa_family_t,
    l2_psm: u16,
    l2_bdaddr: BDAddr,
    l2_cid: u16,
    l2_bdaddr_type: u32,
}
impl Clone for SockaddrL2 {
    fn clone(&self) -> Self { *self }
}

const L2CAP_OPTIONS: i32 = 0x01;
const SOL_L2CAP: i32 = 6;

#[derive(Copy, Debug, Default)]
#[repr(C)]
struct L2CapOptions {
    omtu: u16,
    imtu: u16,
    flush_to: u16,
    mode: u8,
    fcs : u8,
    max_tx: u8,
    txwin_size: u16,
}
impl Clone for L2CapOptions {
    fn clone(&self) -> Self { *self }
}

#[derive(Debug, Copy, Clone)]
pub enum AdapterType {
    BrEdr,
    Amp,
    Unknown(u8)
}

impl AdapterType {
    fn parse(typ: u8) -> AdapterType {
        match typ {
            0 => AdapterType::BrEdr,
            1 => AdapterType::Amp,
            x => AdapterType::Unknown(x),
        }
    }

    fn num(&self) -> u8 {
        match *self {
            AdapterType::BrEdr => 0,
            AdapterType::Amp => 1,
            AdapterType::Unknown(x) => x,
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
    adapter_fd: i32,
    should_stop: Arc<AtomicBool>,
    callbacks: Arc<Mutex<Vec<Callback>>>,
    pub discovered: Arc<Mutex<HashMap<BDAddr, Device>>>,
    device_fds: Arc<Mutex<HashMap<BDAddr, i32>>>,
}

impl ConnectedAdapter {
    pub fn new(adapter: &Adapter, callbacks: Vec<Callback>) -> nix::Result<ConnectedAdapter> {

        let adapter_fd = handle_error(unsafe {
            socket(AF_BLUETOOTH, SOCK_RAW | SOCK_CLOEXEC /*| SOCK_NONBLOCK*/, 1)
        })?;

        let addr = SockaddrHCI {
            hci_family: AF_BLUETOOTH as u16,
            hci_dev: adapter.dev_id,
            hci_channel: 0,
        };

        handle_error(unsafe {
            bind(adapter_fd, &addr as *const SockaddrHCI as *const sockaddr,
                 std::mem::size_of::<SockaddrHCI>() as u32)
        })?;

        let should_stop = Arc::new(AtomicBool::new(false));

        let connected = ConnectedAdapter {
            adapter: adapter.clone(),
            adapter_fd,
            should_stop,
            callbacks: Arc::new(Mutex::new(callbacks)),
            discovered: Arc::new(Mutex::new(HashMap::new())),
            device_fds: Arc::new(Mutex::new(HashMap::new())),
        };

        connected.add_socket_reader(adapter_fd);

        connected.set_socket_filter()?;

        Ok(connected)
    }

    fn set_socket_filter(&self) -> nix::Result<()> {
        let mut filter = BytesMut::with_capacity(14);
        let type_mask = (1 << HCI_COMMAND_PKT) | (1 << HCI_EVENT_PKT) | (1 << HCI_ACLDATA_PKT);
        let event_mask1 = (1 << EVT_DISCONN_COMPLETE) | (1 << EVT_ENCRYPT_CHANGE) |
            (1 << EVT_CMD_COMPLETE) | (1 << EVT_CMD_STATUS);
        let event_mask2 = 1 << (EVT_LE_META_EVENT - 32);
        let opcode = 0;

        filter.put_u32::<LittleEndian>(type_mask);
        filter.put_u32::<LittleEndian>(event_mask1);
        filter.put_u32::<LittleEndian>(event_mask2);
        filter.put_u32::<LittleEndian>(opcode);

        handle_error(unsafe {
            setsockopt(self.adapter_fd, SOL_HCI, HCI_FILTER,
                       filter.as_mut_ptr() as *mut _ as *mut c_void,
                       filter.len() as u32)
        })?;
        Ok(())
    }

    fn add_socket_reader(&self, fd: i32) {
        let should_stop = self.should_stop.clone();
        let connected = self.clone();
//        let mut stream = unsafe {
//            UnixStream::from_raw_fd(fd)
//        };

        thread::spawn(move || {
            let mut buf = [0u8; 2048];
            let mut cur: Vec<u8> = vec![];

            while !should_stop.load(Ordering::Relaxed) {
                // debug!("reading");
                let len = handle_error(unsafe {
                    read(fd, buf.as_mut_ptr() as *mut _ as *mut c_void, buf.len()) as i32
                }).unwrap_or(0) as usize;
                if len == 0 {
                    continue;
                }

                cur.put_slice(&buf[0..len]);

                debug!("parsing {:?}", cur);

                let mut new_cur: Option<Vec<u8>> = Some(vec![]);
                {
                    let result = {
                        AdapterDecoder::decode(&cur)
                    };

                    match result {
                        IResult::Done(left, result) => {
                            info!("> {:?}", result);
                            ConnectedAdapter::handle(&connected, result);
                            if !left.is_empty() {
                                new_cur = Some(left.to_owned());
                            };
                        }
                        IResult::Incomplete(_) => {
                            new_cur = None;
                        },
                        IResult::Error(err) => {
                            error!("parse error {}", err);
                        }
                    }
                };

                cur = new_cur.unwrap_or(cur);
            }
        });
    }

    fn handle(&self, message: Message) {
        debug!("got message {:#?}", message);

        let mut discovered = self.discovered.lock().unwrap();
        match message {
            Message::LEAdvertisingReport(info) => {
                use ::adapter::parser::LEAdvertisingData::*;
                use ::device::AddressType;

                let device = discovered.entry(info.bdaddr)
                    .or_insert_with(||
                        Device::new(info.bdaddr,
                                    if info.bdaddr_type == 1 { AddressType::Random }
                                        else { AddressType::Public }));

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
            Message::LEConnComplete(info) => {
                println!("connected to {:?}", info);
            }
        }
    }

    pub fn connect(&self, device: &Device) -> nix::Result<()> {
        // let mut addr = device.address.clone();
        let fd = handle_error(unsafe {
            socket(AF_BLUETOOTH, SOCK_SEQPACKET, 0)
        })?;

        self.add_socket_reader(fd);

        let local_addr = SockaddrL2 {
            l2_family: AF_BLUETOOTH as sa_family_t,
            l2_psm: 0,
            l2_bdaddr: self.adapter.addr,
            l2_cid: ATT_CID,
            l2_bdaddr_type: self.adapter.typ.num() as u32,
        };

        handle_error(unsafe {
            bind(fd, &local_addr as *const SockaddrL2 as *const sockaddr,
                 std::mem::size_of::<SockaddrL2>() as u32)
        })?;

        let addr = SockaddrL2 {
            l2_family: AF_BLUETOOTH as u16,
            l2_psm: 0,
            l2_bdaddr: device.address,
            l2_cid: ATT_CID,
            l2_bdaddr_type: 1,
        };

        handle_error(unsafe {
            connect(fd, &addr as *const SockaddrL2 as *const sockaddr,
                    size_of::<SockaddrL2>() as u32)
        })?;

        let mut opts = L2CapOptions::default();

        let mut len = size_of::<L2CapOptions>() as u32;
        handle_error(unsafe {
            getsockopt(fd, SOL_L2CAP, L2CAP_OPTIONS,
                       &mut opts as *mut _ as *mut c_void,
                       &mut len)
        })?;

        info!("sock opts: {:#?}", opts);

        self.device_fds.lock().unwrap().insert(device.address, fd);
        Ok(())
    }

    pub fn discover_chars(&self, device: &Device) {
        // TODO: improve error handling
        let fd = self.device_fds.lock().unwrap().get(&device.address).unwrap().clone();

        let mut stream = unsafe { UnixStream::from_raw_fd(fd) };

        let mut buf = BytesMut::with_capacity(7);
        buf.put_u8(ATT_OP_READ_BY_GROUP_REQ);
        buf.put_u16::<LittleEndian>(1);
        buf.put_u16::<LittleEndian>(0xFFFF);
        buf.put_u16::<LittleEndian>(GATT_CHARAC_UUID);

        stream.write_all(&*buf).unwrap();
    }
}

impl Drop for ConnectedAdapter {
    fn drop(&mut self) {
        self.should_stop.store(true, Ordering::Relaxed);
        // clean up
        debug!("cleaning up device");
        unsafe {
            hci_close_dev(self.adapter_fd);
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
