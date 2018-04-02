use ::Result;

use api;
use api::{Characteristic, CharPropFlags, HandleFn, Properties, BDAddr, Host};
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
use ::Error;
use bluez::protocol::hci;
use api::AddressType;
use std::sync::mpsc;
use std::sync::mpsc::Sender;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::channel;
use std::time::Duration;
use bytes::{BytesMut, BufMut};
use bytes::LittleEndian;
use bluez::protocol::att;

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
pub struct Peripheral {
    c_adapter: ConnectedAdapter,
    address: BDAddr,
    properties: Arc<Mutex<Properties>>,
    characteristics: Arc<Mutex<BTreeSet<Characteristic>>>,
    stream: Arc<Mutex<Option<ACLStream>>>,
    connection_tx: Arc<Mutex<Sender<u16>>>,
    connection_rx: Arc<Mutex<Receiver<u16>>>,
}

impl Peripheral {
    pub fn new(c_adapter: ConnectedAdapter, address: BDAddr) -> Peripheral {
        let (connection_tx, connection_rx) = channel();
        Peripheral {
            c_adapter, address,
            properties: Arc::new(Mutex::new(Properties::default())),
            characteristics: Arc::new(Mutex::new(BTreeSet::new())),
            stream: Arc::new(Mutex::new(Option::None)),
            connection_tx: Arc::new(Mutex::new(connection_tx)),
            connection_rx: Arc::new(Mutex::new(connection_rx)),
        }
    }

    pub fn handle_device_message(&self, message: &hci::Message) {
        match message {
            &hci::Message::LEAdvertisingReport(ref info) => {
                assert_eq!(self.address, info.bdaddr, "received message for wrong device");
                use bluez::protocol::hci::LEAdvertisingData::*;

                let mut properties = self.properties.lock().unwrap();

                properties.discovery_count += 1;
                properties.address_type = if info.bdaddr_type == 1 {
                    AddressType::Random
                } else {
                    AddressType::Public
                };

                if info.evt_type == 4 {
                    // discover event
                    properties.has_scan_response = true;
                } else {
                    // TODO: reset service data
                }

                for datum in info.data.iter() {
                    match datum {
                        &LocalName(ref name) => {
                            properties.local_name = Some(name.clone());
                        }
                        &TxPowerLevel(ref power) => {
                            properties.tx_power_level = Some(power.clone());
                        }
                        &ManufacturerSpecific(ref data) => {
                            properties.manufacturer_data = Some(data.clone());
                        }
                        _ => {
                            // skip for now
                        }
                    }
                }
            }
            &hci::Message::LEConnComplete(ref info) => {
                assert_eq!(self.address, info.bdaddr, "received message for wrong device");

                self.connection_tx.lock().unwrap().send(info.handle.clone()).unwrap();
            }
            &hci::Message::ACLDataPacket(ref data) => {
                let handle = data.handle.clone();
                self.stream.lock().unwrap().iter().for_each(|stream| {
                    if stream.handle == handle {
                        stream.receive(data);
                    }
                });
            },
            _ => {
                // ignore
            }
        }
    }

    fn request_by_handle(&self, handle: u16, data: &[u8], handler: Option<HandleFn>) -> Result<()> {
        let l = self.stream.lock().unwrap();
        let stream = l.as_ref().ok_or(Error::NotConnected)?;
        let mut buf = BytesMut::with_capacity(3 + data.len());
        buf.put_u8(ATT_OP_WRITE_REQ);
        buf.put_u16::<LittleEndian>(handle);
        buf.put(data);
        stream.write(&mut *buf, handler);
        Ok(())
    }

    fn write_acl_packet(&self, data: &mut [u8], handler: Option<HandleFn>) -> Result<()> {
        let l = self.stream.lock().unwrap();
        let stream = l.as_ref().ok_or(Error::NotConnected)?;
        stream.write(data, handler);
        Ok(())
    }

    fn notify(&self, characteristic: &Characteristic, enable: bool) -> Result<()> {
        info!("setting notify for {}/{:?} to {}", self.address, characteristic.uuid, enable);
        let l = self.stream.lock().unwrap();
        let stream = l.as_ref().ok_or(Error::NotConnected)?;

        let mut buf = BytesMut::with_capacity(7);
        buf.put_u8(ATT_OP_READ_BY_TYPE_REQ);
        buf.put_u16::<LittleEndian>(characteristic.start_handle);
        buf.put_u16::<LittleEndian>(characteristic.end_handle);
        buf.put_u16::<LittleEndian>(GATT_CLIENT_CHARAC_CFG_UUID);
        let self_copy = self.clone();
        let char_copy = characteristic.clone();

        stream.write(&mut *buf, Some(Box::new(move |_, data| {
            match att::notify_response(data).to_result() {
                Ok(resp) => {
                    debug!("got notify response: {:?}", resp);

                    let use_notify = char_copy.properties.contains(CharPropFlags::NOTIFY);
                    let use_indicate = char_copy.properties.contains(CharPropFlags::INDICATE);

                    let mut value = resp.value;

                    if enable {
                        if use_notify {
                            value |= 0x0001;
                        } else if use_indicate {
                            value |= 0x0002;
                        }
                    } else {
                        if use_notify {
                            value &= 0xFFFE;
                        } else if use_indicate {
                            value &= 0xFFFD;
                        }
                    }

                    let mut value_buf = BytesMut::with_capacity(2);
                    value_buf.put_u16::<LittleEndian>(value);
                    self_copy.request_by_handle(resp.handle,
                                                &*value_buf, Some(Box::new(|_, data| {
                            if data.len() > 0 && data[0] == ATT_OP_WRITE_RESP {
                                debug!("Got response from notify: {:?}", data);
                            } else {
                                warn!("Unexpected notify response: {:?}", data);
                            }
                        }))).unwrap();
                }
                Err(err) => {
                    error!("failed to parse notify response: {:?}", err);
                }
            };
        })));

        Ok(())
    }

}

impl api::Peripheral for Peripheral {
    fn address(&self) -> BDAddr {
        self.address.clone()
    }

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

        // wait until we get the connection notice
        match self.connection_rx.lock().unwrap().recv_timeout(Duration::from_secs(1)) {
            Ok(handle) => {
                // create the acl stream that will communicate with the device
                *stream = Some(ACLStream::new(self.address, handle, fd));
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                return Err(Error::TimedOut(1000));
            }
            err => {
                // unexpected error
                err.unwrap();
            }
        };

        Ok(())
    }


    fn disconnect(&self) -> Result<()> {
        let handle = {
            let l = self.stream.lock().unwrap();
            if l.is_none() {
                // we're already disconnected
                return Ok(());
            }
            l.as_ref().unwrap().handle
        };

        let mut data = BytesMut::with_capacity(3);
        data.put_u16::<LittleEndian>(handle);
        data.put_u8(HCI_OE_USER_ENDED_CONNECTION);
        let mut buf = hci::hci_command(DISCONNECT_CMD, &*data);
        self.c_adapter.write(&mut *buf)
    }

    fn discover_characteristics(&self) -> Result<()> {
        self.discover_characteristics_in_range(0x0001, 0xFFFF)
    }

    fn discover_characteristics_in_range(&self, start: u16, end: u16) -> Result<()> {
        let mut buf = BytesMut::with_capacity(7);
        buf.put_u8(ATT_OP_READ_BY_TYPE_REQ);
        buf.put_u16::<LittleEndian>(start);
        buf.put_u16::<LittleEndian>(end);
        buf.put_u16::<LittleEndian>(GATT_CHARAC_UUID);

        let self_copy = self.clone();
        let handler = Box::new(move |_: u16, data: &[u8]| {
            match att::characteristics(data).to_result() {
                Ok(chars) => {
                    debug!("Chars: {:#?}", chars);
                    let mut cur_chars = self_copy.characteristics.lock().unwrap();
                    let mut next = None;
                    let mut char_set = cur_chars.clone();
                    chars.into_iter().for_each(|mut c| {
                        c.end_handle = end;
                        next = Some(c.start_handle);
                        char_set.insert(c);
                    });

                    // fix the end handles
                    let mut prev = 0xffff;
                    *cur_chars = char_set.into_iter().rev().map(|mut c| {
                        c.end_handle = prev;
                        prev = c.start_handle - 1;
                        c
                    }).collect();

                    next.map(|next| {
                        if next < end {
                            self_copy.discover_characteristics_in_range(next + 1, end).unwrap();
                        }
                    });
                }
                Err(err) => {
                    error!("failed to parse chars: {:?}", err);
                }
            };
        });

        self.write_acl_packet(&mut *buf, Some(handler))
    }

    fn command(&self, characteristic: &Characteristic, data: &[u8]) -> Result<()> {
//        let pair = Arc::new((Mutex::new(None), Condvar::new()));
//        let pair2 = pair.clone();
//        let done = Box::new(move|error| {
//            info!("Done: {:?}", error);
//            let &(ref lock, ref cvar) = &*pair2;
//            let mut done = lock.lock().unwrap();
//            *done = Some(error);
//            cvar.notify_one();
//        });

        {
            let l = self.stream.lock().unwrap();
            let stream = l.as_ref().ok_or(Error::NotConnected)?;
            let mut buf = BytesMut::with_capacity(3 + data.len());
            buf.put_u8(ATT_OP_WRITE_CMD);
            buf.put_u16::<LittleEndian>(characteristic.value_handle);
            buf.put(data);

            stream.write_cmd(&mut *buf, None); //Some(done));
        }

        // wait until we're done
//        let &(ref lock, ref cvar) = &*pair;
//        let mut done = lock.lock().unwrap();
//        while (*done).is_none() {
//            done = cvar.wait(done).unwrap();
//        }

        Ok(())
    }


    fn request(&self, characteristic: &Characteristic, data: &[u8], handler: Option<HandleFn>) -> Result<()> {
        self.request_by_handle(characteristic.value_handle, data, handler)
    }

    fn subscribe(&self, characteristic: &Characteristic) -> Result<()> {
        self.notify(characteristic, true)
    }

    fn unsubscribe(&self, characteristic: &Characteristic) -> Result<()> {
        self.notify(characteristic, false)
    }
}
