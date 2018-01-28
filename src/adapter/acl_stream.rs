extern crate core;

use std::thread;
use std::boxed::Box;
use std::sync::Arc;

use libc::*;

use nix;

use adapter::BDAddr;
use ::constants::*;
use ::util::handle_error;

use std::sync::mpsc::{channel, Sender, Receiver};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::collections::HashMap;
use adapter::parser::ACLData;

use self::StreamMessage::*;

pub type HandleFn = Box<Fn(u16, &[u8]) + Send>;

enum StreamMessage {
    Command(Vec<u8>),
    Request(Vec<u8>, Option<HandleFn>),
    Data(Vec<u8>),
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
                while !should_stop.load(Ordering::Relaxed) {
                    stream.handle_iteration(&rx).unwrap();
                }
            });
        }

        acl_stream
    }

    fn write_socket(&self, value: &mut [u8],
                    receiver: &Receiver<StreamMessage>) -> nix::Result<Vec<u8>> {
        handle_error(unsafe {
            write(self.fd, value.as_mut_ptr() as *mut c_void, value.len()) as i32
        })?;

        loop {
            let message = receiver.recv().unwrap();
            if let Data(rec) = message {
                if rec != value {
                    return Ok(rec);
                }
            } else {
                self.sender.send(message).unwrap();
            }
        }
    }

    fn handle_iteration(&self, receiver: &Receiver<StreamMessage>) -> nix::Result<()> {
        match receiver.recv().unwrap() {
            Command(mut value) => {
                debug!("sending command {:?} to {}", value, self.fd);

                let result = self.write_socket(&mut value, receiver)?;
                if result != [ATT_OP_WRITE_RESP] {
                    warn!("unexpected response to command: {:?}", result)
                }
            },
            Request(mut value, handler) => {
                debug!("sending request {:?} to {}", value, self.fd);

                let result = self.write_socket(&mut value, receiver)?;
                handler.map(|h| h(self.handle, &result));
            },
            Data(value) => {
                debug!("Received data {:?}", value);
                // skip
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
