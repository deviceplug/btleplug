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
use adapter::parser::ACLData;

pub type HandleFn = Box<Fn(u16, &[u8]) + Send>;

#[derive(Clone)]
pub struct ACLStream {
    pub address: BDAddr,
    pub handle: u16,
    fd: i32,
    should_stop: Arc<AtomicBool>,
    cmd_sender: Sender<(Vec<u8>, Option<HandleFn>, bool)>,
    resp_sender: Sender<Vec<u8>>,
}

impl ACLStream {
    pub fn new(address: BDAddr, handle: u16, fd: i32) -> ACLStream {
        info!("Creating new ACLStream for {}, {}, {}", address, handle, fd);
        let (tx, rx) = channel();
        let (resp_tx, resp_rx) = channel();
        let acl_stream = ACLStream {
            address,
            handle,
            fd,
            should_stop: Arc::new(AtomicBool::new(false)),
            cmd_sender: tx,
            resp_sender: resp_tx,
        };

        {
            let should_stop = acl_stream.should_stop.clone();
            let stream = acl_stream.clone();
            thread::spawn(move || {
                while !should_stop.load(Ordering::Relaxed) {
                    stream.handle_iteration(&rx, &resp_rx).unwrap();
                }
            });
        }

        acl_stream
    }

    fn handle_iteration(&self, receiver: &Receiver<(Vec<u8>, Option<HandleFn>, bool)>,
                        resp_rx: &Receiver<Vec<u8>>) -> nix::Result<()> {
        let (mut data, handler, command) = receiver.recv().unwrap();

        debug!("sending {:?} to {}", data, self.fd);

        handle_error(unsafe {
            write(self.fd, data.as_mut_ptr() as *mut c_void, data.len()) as i32
        })?;

        loop {
            let resp = resp_rx.recv().unwrap();

            if resp == data {
                // confirmation
                if command {
                     break;
                }
            } else {
                debug!("got resp: {:?}", resp);
                handler.map(|h| h(self.handle, &resp));
                break;
            }

        }

        Ok(())
    }

    pub fn write(&self, data: &mut [u8], handler: Option<HandleFn>) {
        // let mut packet = Protocol::acl(self.handle, ATT_CID, data);
        self.cmd_sender.send((data.to_owned(), handler, false)).unwrap();
    }

    pub fn write_cmd(&self, data: &mut [u8]) {
        // let mut packet = Protocol::acl(self.handle, ATT_CID, data);
        self.cmd_sender.send((data.to_owned(), None, true)).unwrap();
    }

    pub fn receive(&self, message: &ACLData) {
        // message.data
        // TODO: handle partial packets
        if message.data[0] == ATT_CID as u8 {
            self.resp_sender.send(message.data[2..].to_vec()).unwrap();
        }
    }
}
