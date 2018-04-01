use ::Result;

use api;
use api::{Characteristic, HandleFn, Properties, BDAddr, Host};
use std::mem::size_of;
use std::collections::BTreeSet;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::Ordering;

use libc;

use bluez::adapter::acl_stream::ACLStream;
use bluez::adapter::ConnectedAdapter;
use bluez::util::handle_error;
use bluez::constants::*;
use api::Event;
use api::EventHandler;

#[derive(Copy, Debug)]
#[repr(C)]
pub struct SockaddrL2 {
    l2_family: libc::sa_family_t,
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

#[derive(Clone)]
struct Peripheral {
    c_adapter: ConnectedAdapter,
    address: BDAddr,
    properties: Arc<Mutex<Properties>>,
    characteristics: Arc<Mutex<BTreeSet<Characteristic>>>,
    stream: Arc<Mutex<Option<ACLStream>>>,
    event_handler: Arc<Mutex<Option<EventHandler>>>,
}

impl Peripheral {
    fn new(c_adapter: ConnectedAdapter, address: BDAddr, properties: Properties) {
        let p = Peripheral {
            c_adapter, address,
            properties: Arc::new(Mutex::new(properties)),
            characteristics: Arc::new(Mutex::new(BTreeSet::new())),
            stream: Arc::new(Mutex::new(Option::None)),
            event_handler: Arc::new(Mutex::new(Option::None)),
        };

        let p_clone = p.clone();
        *p.event_handler.lock().unwrap() = Some(Box::new(move|event| {
            p_clone.handle_system_event(event);
        }));

        c_adapter.on_event(*p.event_handler.lock().unwrap());

        p
    }

    pub fn handle_system_event(&self, event: Event) {

    }
}

impl api::Peripheral for Peripheral {
    fn properties(&self) -> Properties {
        let l = self.properties.lock().unwrap();
        l.clone()
    }

    fn characteristics(&self) -> BTreeSet<Characteristic> {
        let l = self.characteristics.lock().unwrap();
        l.clone()
    }

    fn is_connected(&self) -> bool {
        let l = self.stream.lock().unwrap();
        l.is_some()
    }

    fn connect(&self) -> Result<()> {
        // take lock on stream
        let mut stream = self.stream.lock().unwrap();

        if stream.is_some() {
            // we're already connected, just return
            return Ok(());
        }

        // create the socket on which we'll communicate with the device
        let fd = handle_error(unsafe {
            libc::socket(libc::AF_BLUETOOTH, libc::SOCK_SEQPACKET, 0)
        })?;

        let local_addr = SockaddrL2 {
            l2_family: libc::AF_BLUETOOTH as libc::sa_family_t,
            l2_psm: 0,
            l2_bdaddr: self.c_adapter.adapter.addr,
            l2_cid: ATT_CID,
            l2_bdaddr_type: self.c_adapter.adapter.typ.num() as u32,
        };

        // bind to the socket
        handle_error(unsafe {
            libc::bind(fd, &local_addr as *const SockaddrL2 as *const libc::sockaddr,
                       size_of::<SockaddrL2>() as u32)
        })?;

        // configure it as a bluetooth socket
        let mut opt = [1u8, 0];
        handle_error(unsafe {
            libc::setsockopt(fd, libc::SOL_BLUETOOTH, 4, opt.as_mut_ptr() as *mut libc::c_void, 2)
        })?;

        let addr = SockaddrL2 {
            l2_family: libc::AF_BLUETOOTH as u16,
            l2_psm: 0,
            l2_bdaddr: self.address,
            l2_cid: ATT_CID,
            l2_bdaddr_type: 1,
        };

        // connect to the device
        handle_error(unsafe {
            libc::connect(fd, &addr as *const SockaddrL2 as *const libc::sockaddr,
                          size_of::<SockaddrL2>() as u32)
        })?;

        let mut opts = L2CapOptions::default();

        let mut len = size_of::<L2CapOptions>() as u32;
        handle_error(unsafe {
            libc::getsockopt(fd, SOL_L2CAP, L2CAP_OPTIONS,
                             &mut opts as *mut _ as *mut libc::c_void,
                             &mut len)
        })?;

        // restart scanning if we were already, as connecting to a device seems to kill it
        if self.c_adapter.scan_enabled.load(Ordering::Relaxed) {
            self.c_adapter.start_scan()?;
        }

        // create the acl stream that will communicate with the device


        Ok(())
    }


    fn disconnect(&self) -> Result<()> {
        unimplemented!()
    }

    fn discover_characteristics(&self) {
        unimplemented!()
    }

    fn discover_characteristics_in_range(&self, start: u16, end: u16) {
        unimplemented!()
    }

    fn command(&self, characteristic: &Characteristic, data: &[u8]) {
        unimplemented!()
    }

    fn request(&self, characteristic: &Characteristic, data: &[u8], handler: Option<HandleFn>) {
        unimplemented!()
    }

    fn subscribe(&self, characteristic: &Characteristic) {
        unimplemented!()
    }

    fn unsubscribe(&self, characteristic: &Characteristic) {
        unimplemented!()
    }
}
