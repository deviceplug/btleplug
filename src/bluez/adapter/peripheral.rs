use ::Result;

use api::{Characteristic, CharPropFlags, Callback, PeripheralProperties, BDAddr, Central,
          Peripheral as ApiPeripheral};
use std::mem::size_of;
use std::collections::BTreeSet;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::Ordering;

use libc;

use bluez::adapter::acl_stream::{ACLStream};
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
use bluez::protocol::att;
use std::fmt::Debug;
use std::fmt::Formatter;
use std::fmt;
use std::sync::RwLock;
use std::collections::VecDeque;
use bluez::protocol::hci::ACLData;
use std::sync::Condvar;
use api::RequestCallback;
use api::CommandCallback;
use api::UUID;
use api::UUID::B16;
use api::NotificationHandler;
use std::fmt::Display;

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
    properties: Arc<Mutex<PeripheralProperties>>,
    characteristics: Arc<Mutex<BTreeSet<Characteristic>>>,
    stream: Arc<RwLock<Option<ACLStream>>>,
    connection_tx: Arc<Mutex<Sender<u16>>>,
    connection_rx: Arc<Mutex<Receiver<u16>>>,
    message_queue: Arc<Mutex<VecDeque<ACLData>>>,
}

impl Display for Peripheral {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let connected = if self.is_connected() { " connected" } else { "" };
        let properties = self.properties.lock().unwrap();
        write!(f, "{} {}{}", self.address, properties.local_name.clone()
            .unwrap_or("(unknown)".to_string()), connected)
    }
}

impl Debug for Peripheral {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let connected = if self.is_connected() { " connected" } else { "" };
        let properties = self.properties.lock().unwrap();
        let characteristics = self.characteristics.lock().unwrap();
        write!(f, "{} properties: {:?}, characteristics: {:?} {}", self.address, *properties,
               *characteristics, connected)
    }
}

impl Peripheral {
    pub fn new(c_adapter: ConnectedAdapter, address: BDAddr) -> Peripheral {
        let (connection_tx, connection_rx) = channel();
        Peripheral {
            c_adapter, address,
            properties: Arc::new(Mutex::new(PeripheralProperties::default())),
            characteristics: Arc::new(Mutex::new(BTreeSet::new())),
            stream: Arc::new(RwLock::new(Option::None)),
            connection_tx: Arc::new(Mutex::new(connection_tx)),
            connection_rx: Arc::new(Mutex::new(connection_rx)),
            message_queue: Arc::new(Mutex::new(VecDeque::new())),
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

                properties.address = info.bdaddr;

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

                debug!("got le conn complete {:?}", info);
                self.connection_tx.lock().unwrap().send(info.handle.clone()).unwrap();
            }
            &hci::Message::ACLDataPacket(ref data) => {
                let handle = data.handle.clone();
                match self.stream.try_read() {
                    Ok(stream) => {
                        stream.iter().for_each(|stream| {
                            if stream.handle == handle {
                                debug!("got data packet for {}: {:?}", self.address, data);
                                stream.receive(data);
                            }
                        });
                    }
                    Err(_e) => {
                        // we can't access the stream right now because we're still connecting, so
                        // we'll push the message onto a queue for now
                        let mut queue = self.message_queue.lock().unwrap();
                        queue.push_back(data.clone());
                    }
                }
            },
            &hci::Message::DisconnectComplete {..} => {
                // destroy our stream
                debug!("removing stream for {} due to disconnect", self.address);
                let mut stream = self.stream.write().unwrap();
                *stream = None;
                // TODO clean up our sockets
            },
            msg => {
                debug!("ignored message {:?}", msg);
            }
        }
    }

    fn request_raw_async(&self, data: &mut[u8], handler: Option<RequestCallback>) {
        let l = self.stream.read().unwrap();
        match l.as_ref().ok_or(Error::NotConnected) {
            Ok(stream) => {
                stream.write(&mut *data, handler);
            }
            Err(err) => {
                if let Some(h) = handler {
                    h(Err(err));
                }
            }
        }
    }

    fn request_raw(&self, data: &mut [u8]) -> Result<Vec<u8>> {
        Peripheral::wait_until_done(|done: RequestCallback| {
            // TODO this copy can be avoided
            let mut data = data.to_vec();
            self.request_raw_async(&mut data, Some(done));
        })
    }

    fn request_by_handle(&self, handle: u16, data: &[u8], handler: Option<RequestCallback>) {
        let mut buf = BytesMut::with_capacity(3 + data.len());
        buf.put_u8(ATT_OP_WRITE_REQ);
        buf.put_u16_le(handle);
        buf.put(data);
        self.request_raw_async(&mut buf, handler);
    }

    fn notify(&self, characteristic: &Characteristic, enable: bool) -> Result<()> {
        info!("setting notify for {}/{:?} to {}", self.address, characteristic.uuid, enable);
        let mut buf = att::read_by_type_req(
            characteristic.start_handle, characteristic.end_handle, B16(GATT_CLIENT_CHARAC_CFG_UUID));

        let data = self.request_raw(&mut buf)?;

        match att::notify_response(&data) {
            Ok(resp) => {
                let use_notify = characteristic.properties.contains(CharPropFlags::NOTIFY);
                let use_indicate = characteristic.properties.contains(CharPropFlags::INDICATE);

                let mut value = resp.1.value;

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
                value_buf.put_u16_le(value);
                let data = Peripheral::wait_until_done(|done: RequestCallback| {
                    self.request_by_handle(resp.1.handle, &*value_buf, Some(done))
                })?;

                if data.len() > 0 && data[0] == ATT_OP_WRITE_RESP {
                    debug!("Got response from notify: {:?}", data);
                    return Ok(());
                } else {
                    warn!("Unexpected notify response: {:?}", data);
                    return Err(Error::Other("Failed to set notify".to_string()));
                }
            }
            Err(err) => {
                debug!("failed to parse notify response: {:?}", err);
                return Err(Error::Other("failed to get characteristic state".to_string()));
            }
        };
    }

    fn wait_until_done<F, T: Clone + Send + 'static>(operation: F) -> Result<T> where F: for<'a> Fn(Callback<T>) {
        let pair = Arc::new((Mutex::new(None), Condvar::new()));
        let pair2 = pair.clone();
        let on_finish = Box::new(move|result: Result<T>| {
            let &(ref lock, ref cvar) = &*pair2;
            let mut done = lock.lock().unwrap();
            *done = Some(result.clone());
            cvar.notify_one();
        });

        operation(on_finish);

        // wait until we're done
        let &(ref lock, ref cvar) = &*pair;

        let mut done = lock.lock().unwrap();
        while (*done).is_none() {
            done = cvar.wait(done).unwrap();
        }

        // TODO: this copy is avoidable
        (*done).clone().unwrap()
    }
}

impl ApiPeripheral for Peripheral {
    fn address(&self) -> BDAddr {
        self.address.clone()
    }

    fn properties(&self) -> PeripheralProperties {
        let l = self.properties.lock().unwrap();
        l.clone()
    }

    fn characteristics(&self) -> BTreeSet<Characteristic> {
        let l = self.characteristics.lock().unwrap();
        l.clone()
    }

    fn is_connected(&self) -> bool {
        let l = self.stream.try_read();
        return l.is_ok() && l.unwrap().is_some();
    }

    fn connect(&self) -> Result<()> {
        // take lock on stream
        let mut stream = self.stream.write().unwrap();

        if stream.is_some() {
            // we're already connected, just return
            return Ok(());
        }

        // create the socket on which we'll communicate with the device
        let fd = handle_error(unsafe {
            libc::socket(libc::AF_BLUETOOTH, libc::SOCK_SEQPACKET, 0)
        })?;
        debug!("created socket {} to communicate with device", fd);

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
        debug!("bound to socket {}", fd);

        // configure it as a bluetooth socket
        let mut opt = [1u8, 0];
        handle_error(unsafe {
            libc::setsockopt(fd, libc::SOL_BLUETOOTH, 4, opt.as_mut_ptr() as *mut libc::c_void, 2)
        })?;
        debug!("configured socket {}", fd);

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
        debug!("connected to device {} over socket {}", self.address, fd);

        // restart scanning if we were already, as connecting to a device seems to kill it
        if self.c_adapter.scan_enabled.load(Ordering::Relaxed) {
            self.c_adapter.start_scan()?;
            debug!("restarted scanning");
        }

        // wait until we get the connection notice
        let timeout = Duration::from_secs(20);
        match self.connection_rx.lock().unwrap().recv_timeout(timeout) {
            Ok(handle) => {
                // create the acl stream that will communicate with the device
                let s = ACLStream::new(self.c_adapter.adapter.clone(),
                                       self.address, handle, fd);

                // replay missed messages
                let mut queue = self.message_queue.lock().unwrap();
                while !queue.is_empty() {
                    let msg = queue.pop_back().unwrap();
                    if s.handle == msg.handle {
                        s.receive(&msg);
                    }
                }

                *stream = Some(s);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                return Err(Error::TimedOut(timeout.clone()));
            }
            err => {
                // unexpected error
                err.unwrap();
            }
        };

        Ok(())
    }

    fn disconnect(&self) -> Result<()> {
        let mut l = self.stream.write().unwrap();

        if l.is_none() {
            // we're already disconnected
            return Ok(());
        }

        let handle = l.as_ref().unwrap().handle;

        let mut data = BytesMut::with_capacity(3);
        data.put_u16_le(handle);
        data.put_u8(HCI_OE_USER_ENDED_CONNECTION);
        let mut buf = hci::hci_command(DISCONNECT_CMD, &*data);
        self.c_adapter.write(&mut *buf)?;

        *l = None;
        Ok(())
    }

    fn discover_characteristics(&self) -> Result<Vec<Characteristic>> {
        self.discover_characteristics_in_range(0x0001, 0xFFFF)
    }

    fn discover_characteristics_in_range(&self, start: u16, end: u16) -> Result<Vec<Characteristic>> {
        let mut results = vec![];
        let mut start = start;
        loop {
            debug!("discovering chars in range [{}, {}]", start, end);

            let mut buf = att::read_by_type_req(start, end, B16(GATT_CHARAC_UUID));
            let data = self.request_raw(&mut buf)?;

            match att::characteristics(&data) {
                Ok(result) => {
                    match result.1 {
                        Ok(chars) => {
                            debug!("Chars: {:#?}", chars);

                            // TODO this copy can be removed
                            results.extend(chars.clone());

                            if let Some(ref last) = chars.iter().last() {
                                if last.start_handle < end - 1 {
                                    start = last.start_handle + 1;
                                    continue;
                                }
                            }
                            break;
                        }
                        Err(err) => {
                            // this generally means we should stop iterating
                            debug!("got error: {:?}", err);
                            break;
                        }
                    }
                }
                Err(err) => {
                    error!("failed to parse chars: {:?}", err);
                    return Err(Error::Other(format!("failed to parse characteristics response {:?}",
                                                    err)));
                }
            }
        }

        // fix the end handles (we don't get them directly from device, so we have to infer)
        for i in 0..results.len() {
            (*results.get_mut(i).unwrap()).end_handle =
                results.get(i + 1).map(|c| c.end_handle).unwrap_or(end);
        }

        // update our cache
        let mut lock = self.characteristics.lock().unwrap();
        results.iter().for_each(|c| { lock.insert(c.clone());});

        Ok(results)
    }

    fn command_async(&self, characteristic: &Characteristic, data: &[u8], handler: Option<CommandCallback>) {
        let l = self.stream.read().unwrap();
        match l.as_ref() {
            Some(stream) => {
                let mut buf = BytesMut::with_capacity(3 + data.len());
                buf.put_u8(ATT_OP_WRITE_CMD);
                buf.put_u16_le(characteristic.value_handle);
                buf.put(data);

                stream.write_cmd(&mut *buf, handler);
            }
            None => {
                handler.iter().for_each(|h| h(Err(Error::NotConnected)));
            }
        }
    }

    fn command(&self, characteristic: &Characteristic, data: &[u8]) -> Result<()> {
        Peripheral::wait_until_done(|done: CommandCallback| {
            self.command_async(characteristic, data, Some(done));
        })
    }

    fn request_async(&self, characteristic: &Characteristic, data: &[u8], handler: Option<RequestCallback>) {
        self.request_by_handle(characteristic.value_handle, data, handler);
    }

    fn request(&self, characteristic: &Characteristic, data: &[u8]) -> Result<Vec<u8>> {
        Peripheral::wait_until_done(|done: RequestCallback| {
            self.request_async(characteristic, data, Some(done));
        })
    }

    fn read_by_type_async(&self, characteristic: &Characteristic, uuid: UUID,
                          handler: Option<RequestCallback>) {
        let mut buf = att::read_by_type_req(characteristic.start_handle, characteristic.end_handle, uuid);
        self.request_raw_async(&mut buf, handler);
    }

    fn read_by_type(&self, characteristic: &Characteristic, uuid: UUID) -> Result<Vec<u8>> {
        Peripheral::wait_until_done(|done: RequestCallback| {
            self.read_by_type_async(characteristic, uuid, Some(done));
        })
    }


    fn subscribe(&self, characteristic: &Characteristic) -> Result<()> {
        self.notify(characteristic, true)
    }

    fn unsubscribe(&self, characteristic: &Characteristic) -> Result<()> {
        self.notify(characteristic, false)
    }

    fn on_notification(&self, handler: NotificationHandler) {
        // TODO handle the disconnected case better
        let l = self.stream.read().unwrap();
        match l.as_ref() {
            Some(stream) => {
                stream.on_notification(handler);
            }
            None => {
                error!("tried to subscribe to notifications, but not yet connected")
            }
        }
    }
}
