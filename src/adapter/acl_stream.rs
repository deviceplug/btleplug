use std::thread;
use std::boxed::Box;
use std::sync::Arc;
use std::time::Duration;

use libc::*;

use nix;

use adapter::BDAddr;
use ::constants::*;
use ::util::handle_error;

use std::fmt;
use std::fmt::{Debug, Formatter};
use std::sync::mpsc::{channel, Sender, Receiver};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::collections::HashMap;
use ::protocol::hci::ACLData;

use self::StreamMessage::*;

pub type HandleFn = Box<Fn(u16, &[u8]) + Send>;

enum StreamMessage {
    Command(Vec<u8>),
    Request(Vec<u8>, Option<HandleFn>),
    Data(Vec<u8>),
}

impl Debug for StreamMessage {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            &Command(ref data) => write!(f, "Command({:?})", data),
            &Request(ref data, ref cb) => write!(f, "Request({:?}, cb: {})", data, cb.is_some()),
            &Data(ref data) => write!(f, "Data({:?})", data),
        }
    }
}

#[derive(Clone)]
pub struct ACLStream {
    pub address: BDAddr,
    pub handle: u16,
    fd: i32,
    should_stop: Arc<AtomicBool>,
    sender: Sender<StreamMessage>,
    subscriptions: Arc<Mutex<HashMap<u16, Vec<HandleFn>>>>
}

impl ACLStream {
    pub fn new(address: BDAddr, handle: u16, fd: i32) -> ACLStream {
        info!("Creating new ACLStream for {}, {}, {}", address, handle, fd);
        let (tx, rx) = channel();
        let acl_stream = ACLStream {
            address,
            handle,
            fd,
            should_stop: Arc::new(AtomicBool::new(false)),
            sender: tx,
            subscriptions: Arc::new(Mutex::new(HashMap::new())),
        };

        {
            let should_stop = acl_stream.should_stop.clone();
            let stream = acl_stream.clone();
            thread::spawn(move || {
                let mut msg = rx.recv().unwrap();
                while !should_stop.load(Ordering::Relaxed) {
                    match stream.handle_iteration(&mut msg, &rx) {
                        Ok(_) => msg = rx.recv().unwrap(),
                        Err(nix::Error::Sys(nix::errno::ENOTCONN)) => {
                            // retry message
                            thread::sleep(Duration::from_millis(50));
                            continue;
                        }
                        Err(e) => {
                            panic!("Unhandled error {}", e);
                        }
                    }
                }

                if let Err(err) = handle_error(unsafe { close(fd) }) {
                    warn!("Failed to close socket {}: {}", fd, err);
                };
            });
        }

        acl_stream
    }

    fn write_socket(&self, value: &mut [u8], command: bool,
                    receiver: &Receiver<StreamMessage>) -> nix::Result<Vec<u8>> {
        handle_error(unsafe {
            write(self.fd, value.as_mut_ptr() as *mut c_void, value.len()) as i32
        })?;

        let mut skipped = vec![];
        loop {
            let message = receiver.recv().unwrap();
            debug!("waiting for confirmation... {:?}", message);
            if let Data(rec) = message {
                if rec != value {
                    skipped.into_iter().for_each(|m|
                        self.sender.send(m).unwrap());
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
                        receiver: &Receiver<StreamMessage>) -> nix::Result<()> {
        match *msg {
            Command(ref mut value) => {
                debug!("sending command {:?} to {}", value, self.fd);

                self.write_socket(value, true, receiver)?;
            },
            Request(ref mut value, ref handler) => {
                debug!("sending request {:?} to {}", value, self.fd);

                let result = self.write_socket(value, false, receiver)?;
                handler.iter().for_each(|h| h(self.handle, &result));
            },
            Data(ref value) => {
                debug!("Received data {:?}", value);
                if !value.is_empty() {
                    match value[0] {
                        ATT_OP_EXCHANGE_MTU_REQ => {
                            // write back that we don't support it?
                            self.write_cmd(&mut [0x01, 0x02, 0x00, 0x00, 0x06]);
                        }
                        ATT_OP_VALUE_NOTIFICATION => {

                        }
                        _ => {
                            // skip
                        }
                    }
                }
            }
        }

        Ok(())
    }

    pub fn write(&self, data: &mut [u8], handler: Option<HandleFn>) {
        // let mut packet = Protocol::acl(self.handle, ATT_CID, data);
        self.sender.send(Request(data.to_owned(), handler)).unwrap();
    }

    pub fn write_cmd(&self, data: &mut [u8]) {
        // let mut packet = Protocol::acl(self.handle, ATT_CID, data);
        self.sender.send(Command(data.to_owned())).unwrap();
    }

    pub fn receive(&self, message: &ACLData) {
        // message.data
        // TODO: handle partial packets
        if message.cid == ATT_CID {
            self.sender.send(Data(message.data.to_vec())).unwrap();
        }
    }
}

impl Drop for ACLStream {
    fn drop(&mut self) {
        self.should_stop.clone().store(true, Ordering::Relaxed);
    }
}
