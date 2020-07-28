// btleplug Source Code File
//
// Copyright 2020 Nonpolynomial Labs LLC. All rights reserved.
//
// Licensed under the BSD 3-Clause license. See LICENSE file in the project root
// for full license information.
//
// Some portions of this file are taken and/or modified from Rumble
// (https://github.com/mwylde/rumble), using a dual MIT/Apache License under the
// following copyright:
//
// Copyright (c) 2014 The Rust Project Developers

mod acl_stream;
mod peripheral;

use libc;
use nom;
use bytes::{BytesMut, BufMut};
use dashmap::DashMap;

use std::{
    self,
    ffi::CStr,
    collections::HashSet,
    sync::{Arc, mpsc::Receiver, atomic::{AtomicBool, Ordering}},
    thread,
};

use crate::Result;
use crate::api::{CentralEvent, BDAddr, Central, AdapterManager};

use crate::bluez::{
    util::handle_error,
    protocol::hci,
    adapter::peripheral::Peripheral,
    constants::*,
    ioctl,
};


#[derive(Copy, Debug)]
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

#[derive(Copy, Debug)]
#[repr(C)]
pub struct HCIDevInfo {
    pub dev_id : u16,
    pub name : [libc::c_char; 8],
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
            name: [0 as libc::c_char; 8],
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
struct SockaddrHCI {
    hci_family: libc::sa_family_t,
    hci_dev: u16,
    hci_channel: u16,
}

impl Clone for SockaddrHCI {
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
            if flags & (1 << (i & 31)) as u32 != 0 {
                set.insert(f.clone());
            }
        }

        set
    }
}

/// The [`Central`](../../api/trait.Central.html) implementation for BlueZ.
#[derive(Clone, Debug)]
pub struct ConnectedAdapter {
    pub adapter: Adapter,
    adapter_fd: i32,
    should_stop: Arc<AtomicBool>,
    pub scan_enabled: Arc<AtomicBool>,
    pub active: Arc<AtomicBool>,
    pub filter_duplicates: Arc<AtomicBool>,
    handle_map: Arc<DashMap<u16, BDAddr>>,
    manager: AdapterManager<Peripheral>
}

impl ConnectedAdapter {
    pub fn new(adapter: &Adapter) -> Result<ConnectedAdapter> {
        let adapter_fd = handle_error(unsafe {
            libc::socket(libc::AF_BLUETOOTH, libc::SOCK_RAW | libc::SOCK_CLOEXEC, 1)
        })?;

        let addr = SockaddrHCI {
            hci_family: libc::AF_BLUETOOTH as u16,
            hci_dev: adapter.dev_id,
            hci_channel: 0,
        };

        handle_error(unsafe {
            libc::bind(adapter_fd, &addr as *const SockaddrHCI as *const libc::sockaddr,
                       std::mem::size_of::<SockaddrHCI>() as u32)
        })?;

        let should_stop = Arc::new(AtomicBool::new(false));

        let connected = ConnectedAdapter {
            adapter: adapter.clone(),
            adapter_fd,
            active: Arc::new(AtomicBool::new(true)),
            filter_duplicates: Arc::new(AtomicBool::new(false)),
            should_stop,
            scan_enabled: Arc::new(AtomicBool::new(false)),
            handle_map: Arc::new(DashMap::new()),
            manager: AdapterManager::new(),
        };

        connected.add_raw_socket_reader(adapter_fd);

        connected.set_socket_filter()?;

        Ok(connected)
    }

    fn set_socket_filter(&self) -> Result<()> {
        let mut filter = BytesMut::with_capacity(14);
        let type_mask = (1 << HCI_COMMAND_PKT) | (1 << HCI_EVENT_PKT) as u8 | (1 << HCI_ACLDATA_PKT) as u8;
        let event_mask1 = (1 << EVT_DISCONN_COMPLETE) | (1 << EVT_ENCRYPT_CHANGE) |
            (1 << EVT_CMD_COMPLETE) | (1 << EVT_CMD_STATUS);
        let event_mask2 = 1 << (EVT_LE_META_EVENT - 32);
        let opcode = 0;

        filter.put_u32_le(type_mask as u32);
        filter.put_u32_le(event_mask1 as u32);
        filter.put_u32_le(event_mask2 as u32);
        filter.put_u16_le(opcode);

        handle_error(unsafe {
            libc::setsockopt(self.adapter_fd, SOL_HCI, HCI_FILTER,
                             filter.as_mut_ptr() as *mut _ as *mut libc::c_void,
                             filter.len() as u32)
        })?;
        Ok(())
    }

    fn add_raw_socket_reader(&self, fd: i32) {
        let should_stop = self.should_stop.clone();
        let connected = self.clone();

        thread::spawn(move || {
            let mut buf = [0u8; 2048];
            let mut cur: Vec<u8> = vec![];

            while !should_stop.load(Ordering::Relaxed) {
                // debug!("reading");
                let len = handle_error(unsafe {
                    libc::read(fd, buf.as_mut_ptr() as *mut _ as *mut libc::c_void, buf.len()) as i32
                }).unwrap_or(0) as usize;
                if len == 0 {
                    continue;
                }

                cur.put_slice(&buf[0..len]);

                let mut new_cur: Option<Vec<u8>> = Some(vec![]);
                {
                    let result = {
                        hci::message(&cur)
                    };

                    match result {
                        Ok((left, result)) => {
                            ConnectedAdapter::handle(&connected, result);
                            if !left.is_empty() {
                                new_cur = Some(left.to_owned());
                            };
                        }
                        Err(nom::Err::Incomplete(_)) => {
                            new_cur = None;
                        },
                        Err(nom::Err::Error(err)) | Err(nom::Err::Failure(err)) => {
                            error!("parse error {:?}\nfrom: {:?}", err, cur);
                        }
                    }
                };

                cur = new_cur.unwrap_or(cur);
            }
        });
    }

    fn emit(&self, event: CentralEvent) {
        debug!("emitted {:?}", event);
        self.manager.emit(event);
    }

    fn handle(&self, message: hci::Message) {
        debug!("got message {:?}", message);

        match message {
            hci::Message::LEAdvertisingReport(info) => {
                let address = info.bdaddr.clone();
                let info_clone = info.clone();
                // FIXME this is confusing, it appears we are getting an *owned* Peripheral here
                // but in fact because most of its members are wrapped in an Arc it ends up
                // actually sharing most of its state with the original object...
                let peripheral = self.manager.peripheral(info.bdaddr)
                    .unwrap_or_else(|| Peripheral::new(self.clone(), info.bdaddr));
                peripheral.handle_device_message(&hci::Message::LEAdvertisingReport(info));
                if !self.manager.has_peripheral(&address) {
                    warn!("{:?}", info_clone);
                    self.manager.add_peripheral(address, peripheral);
                    self.emit(CentralEvent::DeviceDiscovered(address.clone()))
                } else {
                    self.manager.update_peripheral(address, peripheral);
                    self.emit(CentralEvent::DeviceUpdated(address.clone()))
                }
            }
            hci::Message::LEConnComplete(info) => {
                info!("connected to {:?}", info);
                let address = info.bdaddr.clone();
                let handle = info.handle.clone();
                match self.peripheral(address) {
                    Some(peripheral) => {
                        peripheral.handle_device_message(&hci::Message::LEConnComplete(info))
                    }
                    // todo: there's probably a better way to handle this case
                    None => warn!("Got connection for unknown device {}", info.bdaddr)
                }

                self.handle_map.insert(handle, address);

                self.emit(CentralEvent::DeviceConnected(address));
            }
            hci::Message::ACLDataPacket(data) => {
                let message = hci::Message::ACLDataPacket(data);

                // TODO this is a bit risky from a deadlock perspective (note mutexes are not
                // reentrant in rust!)
                let peripherals = self.manager.peripherals();

                for peripheral in peripherals {
                    // we don't know the handler => device mapping, so send to all and let them filter
                    peripheral.handle_device_message(&message);
                }
            },
            hci::Message::DisconnectComplete { handle, .. } => {
                match self.handle_map.remove(&handle) {
                    Some(addr) => {
                        match self.peripheral(addr.1) {
                            Some(peripheral) => peripheral.handle_device_message(&message),
                            None => warn!("got disconnect for unknown device {}", addr.1),
                        };
                        self.emit(CentralEvent::DeviceDisconnected(addr.1));
                    }
                    None => {
                        warn!("got disconnect for unknown handle {}", handle);
                    }
                }
            }
            _ => {
                // skip
            }
        }
    }

    fn write(&self, message: & [u8]) -> Result<()> {
        debug!("writing({}) {:?}", self.adapter_fd, message);
        let ptr = message.as_ptr();
        handle_error(unsafe {
            libc::write(self.adapter_fd, ptr as *const libc::c_void, message.len()) as i32
        })?;
        Ok(())
    }

    fn set_scan_params(&self) -> Result<()> {
        let mut data = BytesMut::with_capacity(7);
        data.put_u8(if self.active.load(Ordering::Relaxed) { 1 } else { 0 }); // scan_type = active or passive
        data.put_u16_le(0x0010); // interval ms
        data.put_u16_le(0x0010); // window ms
        data.put_u8(0); // own_type = public
        data.put_u8(0); // filter_policy = public
        let mut buf = hci::hci_command(LE_SET_SCAN_PARAMETERS_CMD, &*data);
        self.write(&mut *buf)
    }

    fn set_scan_enabled(&self, enabled: bool) -> Result<()> {
        let mut data = BytesMut::with_capacity(2);
        data.put_u8(if enabled { 1 } else { 0 }); // enabled
        data.put_u8(if self.filter_duplicates.load(Ordering::Relaxed) { 1 } else { 0 }); // filter duplicates

        self.scan_enabled.clone().store(enabled, Ordering::Relaxed);
        let mut buf = hci::hci_command(LE_SET_SCAN_ENABLE_CMD, &*data);
        self.write(&mut *buf)
    }
}

impl Central<Peripheral> for ConnectedAdapter {
    fn event_receiver(&self) -> Option<Receiver<CentralEvent>> {
        self.manager.event_receiver()
    }

    fn active(&self, enabled: bool) {
        self.active.clone().store(enabled, Ordering::Relaxed);
    }

    fn filter_duplicates(&self, enabled: bool) {
        self.filter_duplicates.clone().store(enabled, Ordering::Relaxed);
    }

    fn start_scan(&self) -> Result<()> {
        self.set_scan_params()?;
        self.set_scan_enabled(true)
    }

    fn stop_scan(&self) -> Result<()> {
        self.set_scan_enabled(false)
    }

    fn peripherals(&self) -> Vec<Peripheral> {
        self.manager.peripherals()
    }

    fn peripheral(&self, address: BDAddr) -> Option<Peripheral> {
        self.manager.peripheral(address)
    }
}

/// Adapter represents a physical bluetooth interface in your system, for example a bluetooth
/// dongle.
#[derive(Debug, Clone)]
pub struct Adapter {
    /// The name of the adapter.
    pub name: String,

    /// The device id of the adapter.
    pub dev_id: u16,

    /// The address of the adapter.
    pub addr: BDAddr,

    /// The type of the adapater.
    pub typ: AdapterType,

    /// The set of states that the adapater is in.
    pub states: HashSet<AdapterState>,

    /// Properties of the adapter.
    pub info: HCIDevInfo,
}

impl Adapter {
    pub fn from_device_info(di: &HCIDevInfo) -> Adapter {
        info!("DevInfo: {:?}", di);
        Adapter {
            name: String::from(unsafe { CStr::from_ptr(di.name.as_ptr()).to_str().unwrap() }),
            dev_id: di.dev_id,
            addr: di.bdaddr,
            typ: AdapterType::parse((di.type_ & 0x30) >> 4),
            states: AdapterState::parse(di.flags),
            info: di.clone(),
        }
    }

    pub fn from_dev_id(ctl: i32, dev_id: u16) -> Result<Adapter> {
        let mut di = HCIDevInfo::default();
        di.dev_id = dev_id;

        unsafe {
            ioctl::hci_get_dev_info(ctl, &mut di)?;
        }

        Ok(Adapter::from_device_info(&di))
    }

    pub fn is_up(&self) -> bool {
        self.states.contains(&AdapterState::Up)
    }

    pub fn connect(&self) -> Result<ConnectedAdapter> {
        ConnectedAdapter::new(self)
    }
}
