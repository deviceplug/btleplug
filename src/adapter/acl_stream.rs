extern crate core;

use std;
use std::thread;
use std::boxed::Box;
use std::sync::Arc;

use libc::*;

use nix;

use adapter::BDAddr;
use ::util::handle_error;

use std::sync::mpsc::{channel, Sender, Receiver};
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Copy, Debug, Default)]
#[repr(packed)]
struct MgmtHdr {
    opcode: u16,
    index: u16,
    len: u16,
}
impl Clone for MgmtHdr {
    fn clone(&self) -> Self { *self }
}

pub type HandleFn = Box<Fn(u16, &[u8]) + Send>;

#[derive(Clone)]
pub struct ACLStream {
    pub address: BDAddr,
    pub handle: u16,
    fd: i32,
    should_stop: Arc<AtomicBool>,
    sender: Sender<(Vec<u8>, Option<HandleFn>, bool)>
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

    fn handle_iteration(&self, receiver: &Receiver<(Vec<u8>, Option<HandleFn>, bool)>) -> nix::Result<()> {
        let (mut data, handler, command) = receiver.recv().unwrap();

        debug!("sending {:?} to {}", data, self.fd);

        handle_error(unsafe {
            write(self.fd, data.as_mut_ptr() as *mut c_void, data.len()) as i32
        })?;

        if !command {
            match self.read_message() {
                //            Ok(Message::ACLDataPacket {cid, data, ..}) => {
                //                handler.map(|h| h(cid, &data));
                //            },
                Ok(data) => {
                    handler.map(|h| h(self.handle, &data));
                }
                Err(err) => {
                    match err {
                        nix::Error::Sys(nix::errno::ENOTCONN) => {
                            // skip
                        }
                        nix::Error::Sys(nix::errno::EBADF) => {
                            info!("fd {} closed, stopping read", self.fd);
                            // todo: stop
                        }
                        _ => {
                            warn!("fd {} error: {:?}", self.fd, err)
                        }
                    };
                    thread::sleep(std::time::Duration::from_millis(100));
                }
            };
        }
        Ok(())
    }

    fn read_message(&self) -> nix::Result<Vec<u8>> {
//        let mut control = [0u8; 64];
        let mut buf = [0u8; 2048];

//        let mut header = MgmtHdr::default();
//        let mut iov = [
//            iovec {
//                iov_base:  &mut header as *mut _ as *mut c_void,
//                iov_len: size_of::<MgmtHdr>(),
//            },
//            iovec {
//                iov_base: buf.as_mut_ptr() as *mut c_void,
//                iov_len: buf.len()
//            },
//        ];
//
//        let mut msg = msghdr {
//            msg_iov: iov.as_mut_ptr(),
//            msg_iovlen: iov.len(),
//            msg_control: control.as_mut_ptr() as *mut c_void,
//            msg_controllen: control.len(),
//            msg_name: std::ptr::null_mut() as *mut c_void,
//            msg_namelen: 0,
//            msg_flags: 0,
//        };

        let len = handle_error(unsafe {
            read(self.fd, buf.as_mut_ptr() as *mut c_void, buf.len()) as i32 })?;


        if len < 0 {
            //continue;
            return Err(nix::Error::from(nix::errno::ENODATA));
        }

        // TODO: monitor/control.c does some stuff with timevals and creds that should
        // probably be duplicated

        let data = &buf[0..len as usize];
        Ok(data.to_owned())

        // debug!("header: {:?}", header);

//        match Decoder::decode(data) {
//            IResult::Done(left, result) => {
//                info!("> {:?}", result);
//                if !left.is_empty() {
//                    error!("unexpected left-over data: {:?}", left);
//                }
//                return Ok(result);
//            }
//            IResult::Incomplete(_) => {
//                error!("unexpected incomplete {:?}", data);
//            }
//            IResult::Error(err) => {
//                error!("parse error {}\nfrom: {:?}", err, data);
//            }
//        };
//
//        Err(nix::Error::from(nix::errno::EBADMSG))
    }
//
//    fn add_seq_socket_reader(should_stop: fd: i32) {
//        let should_stop = self.should_stop.clone();
//        let connected = self.clone();
//
//        thread::spawn(move || {
//            while !should_stop.load(Ordering::Relaxed) {
//        });
//
//    }


    pub fn write(&self, data: &mut [u8], handler: Option<HandleFn>) {
        // let mut packet = Protocol::acl(self.handle, ATT_CID, data);
        self.sender.send((data.to_owned(), handler, false)).unwrap();
    }

    pub fn write_cmd(&self, data: &mut [u8]) {
        // let mut packet = Protocol::acl(self.handle, ATT_CID, data);
        self.sender.send((data.to_owned(), None, true)).unwrap();
    }

//    pub fn write_att(&self, data: &mut [u8]) {
//        let packet = Protocol::acl(self.handle, ATT_CID, data);
//        self.sender.send((packet.to_vec(), None)).unwrap();
//    }

//    pub fn receive(&self, message: Message) {
//        // TODO: handle partial packets
//        match message {
//            Message::ACLDataPacket { cid, data, .. } => {
//                let handler = self.cur_handler.lock().unwrap();
//                handler.map(|f| {
//                    f(cid, &data);
//                });
//
//            }
//            _ => {
//                panic!("should onl ysend ACLDataPackets here")
//            }
//        }
//    }
}
