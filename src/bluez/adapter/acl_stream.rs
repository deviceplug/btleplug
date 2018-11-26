use std::thread;
use std::sync::Arc;
use std::time::Duration;

use libc;

use ::Result;

use bluez::constants::*;
use bluez::util::handle_error;

use std::fmt;
use std::fmt::{Debug, Formatter};
use std::sync::mpsc::{channel, Sender, Receiver};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use bluez::protocol::hci::ACLData;
use bluez::protocol::att;

use self::StreamMessage::*;
use api::BDAddr;
use Error;
use api::CommandCallback;
use api::RequestCallback;
use bluez::adapter::Adapter;
use bytes::BytesMut;
use bytes::BufMut;
use api::NotificationHandler;

enum StreamMessage  {
    Command(Vec<u8>, Option<CommandCallback>),
    Request(Vec<u8>, Option<RequestCallback>),
    Data(Vec<u8>),
}

impl Debug for StreamMessage {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            &Command(ref data, ref _cb) => write!(f, "Command({:?})", data),
            &Request(ref data, ref cb) => write!(f, "Request({:?}, cb: {})", data, cb.is_some()),
            &Data(ref data) => write!(f, "Data({:?})", data),
        }
    }
}

#[derive(Clone)]
pub struct ACLStream {
    adapter: Adapter,
    pub address: BDAddr,
    pub handle: u16,
    fd: i32,
    should_stop: Arc<AtomicBool>,
    sender: Arc<Mutex<Sender<StreamMessage>>>,
    notification_handlers: Arc<Mutex<Vec<NotificationHandler>>>,
}

impl ACLStream {
    pub fn new(adapter: Adapter, address: BDAddr, handle: u16, fd: i32) -> ACLStream {
        info!("Creating new ACLStream for {}, {}, {}", address, handle, fd);
        let (tx, rx) = channel();
        let acl_stream = ACLStream {
            adapter,
            address,
            handle,
            fd,
            should_stop: Arc::new(AtomicBool::new(false)),
            sender: Arc::new(Mutex::new(tx)),
            notification_handlers: Arc::new(Mutex::new(vec![])),
        };

        {
            let should_stop = acl_stream.should_stop.clone();
            let stream = acl_stream.clone();
            thread::spawn(move || {
                let mut msg = rx.recv().unwrap();
                while !should_stop.load(Ordering::Relaxed) {
                    match stream.handle_iteration(&mut msg, &rx) {
                        Ok(_) => msg = rx.recv().unwrap(),
                        Err(Error::NotConnected) => {
                            // retry message
                            thread::sleep(Duration::from_millis(50));
                            continue;
                        }
                        Err(e) => {
                            error!("Unhandled error {}", e);
                        }
                    }
                }

                if let Err(err) = handle_error(unsafe { libc::close(fd) }) {
                    warn!("Failed to close socket {}: {}", fd, err);
                };
            });
        }

        acl_stream
    }

    fn write_socket(&self, value: &mut [u8], command: bool,
                    receiver: &Receiver<StreamMessage>) -> Result<Vec<u8>> {
        debug!("writing {:?}", value);
        handle_error(unsafe {
            libc::write(self.fd, value.as_mut_ptr() as *mut libc::c_void, value.len()) as i32
        })?;

        let mut skipped = vec![];
        loop {
            let message = receiver.recv().unwrap();
            debug!("waiting for confirmation... {:?}", message);
            if let Data(rec) = message {
                if rec != value {
                    skipped.into_iter().for_each(|m|
                        self.send(m));
                    return Ok(rec);
                } else if command {
                    return Ok(vec![]);
                }
            } else {
                skipped.push(message);
            }
        }
    }

    fn handle_iteration(&self, msg: &mut StreamMessage,
                        receiver: &Receiver<StreamMessage>) -> Result<()> {
        match *msg {
            Command(ref mut value, ref handler) => {
                debug!("sending command {:?} to {}", value, self.fd);

                let result = self.write_socket(value, true, receiver)
                    .map(|_v| ());
                if let &Some(ref f) = handler {
                    f(result);
                }
            },
            Request(ref mut value, ref handler) => {
                debug!("sending request {:?} to {}", value, self.fd);

                let result = self.write_socket(value, false, receiver);
                if let &Some(ref f) = handler {
                    f(result);
                }
            },
            Data(ref value) => {
                debug!("Received data {:?}", value);
            }
        }

        Ok(())
    }

    fn send(&self, message: StreamMessage) {
        let l = self.sender.lock().unwrap();
        l.send(message).unwrap();
    }

    pub fn write(&self, data: &mut [u8], handler: Option<RequestCallback>) {
        // let mut packet = Protocol::acl(self.handle, ATT_CID, data);
        self.send(Request(data.to_owned(), handler));
    }

    pub fn write_cmd(&self, data: &mut [u8], on_done: Option<CommandCallback>) {
        self.send(Command(data.to_owned(), on_done));
    }

    pub fn on_notification(&self, handler: NotificationHandler) {
        let mut list = self.notification_handlers.lock().unwrap();
        list.push(handler);
    }

    pub fn receive(&self, message: &ACLData) {
        debug!("receive message: {:?}", message);
        // message.data
        // TODO: handle partial packets
        if message.cid == ATT_CID {
            let value = message.data.to_vec();
            if !value.is_empty() {
                match value[0] {
                    ATT_OP_EXCHANGE_MTU_REQ => {
                        let request = att::mtu_request(&value).unwrap().1;
                        // is the client MTU smaller than ours?
                        if request.client_rx_mtu <= self.adapter.info.acl_mtu {
                            debug!("sending MTU: {}", self.adapter.info.acl_mtu);
                            // it is, send confirmation
                            let mut buf = BytesMut::with_capacity(3);
                            buf.put_u8(ATT_OP_EXCHANGE_MTU_RESP);
                            buf.put_u16_le(self.adapter.info.acl_mtu);
                            self.write_cmd(&mut buf, None);
                        } else {
                            // TODO: reduce our MTU to client's
                            error!("client's MTU is larger than ours");
                            self.write_cmd(&mut [0x01, 0x02, 0x00, 0x00, 0x06], None);
                        }
                    }
                    ATT_OP_VALUE_NOTIFICATION => {
                        debug!("value notification: {:?}", value);
                        match att::value_notification(&value) {
                            Ok(notification) => {
                                let handlers = self.notification_handlers.lock().unwrap();
                                handlers.iter().for_each(|h| h(notification.1.clone()));
                            }
                            Err(err) => {
                                error!("failed to parse notification: {:?}", err);
                            }
                        }
                    }
                    _ => {
                        self.send(Data(value));
                    }
                }
            }
        }
    }
}

impl Drop for ACLStream {
    fn drop(&mut self) {
        self.should_stop.clone().store(true, Ordering::Relaxed);
    }
}
