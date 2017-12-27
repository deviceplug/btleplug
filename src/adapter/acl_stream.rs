use std::sync::Mutex;
use std::sync::atomic::AtomicPtr;

use libc::{write, c_void};

use nix;

use adapter::BDAddr;
use adapter::protocol::Protocol;
use adapter::parser::{Message, Characteristic, CharacteristicUUID};
use ::util::handle_error;
use ::constants::*;


type HandleFn = fn(cid: u16, data: &[u8]) -> ();

pub struct ACLStream {
    pub address: BDAddr,
    pub handle: u16,
    pub fd: i32,
    pub cur_handler: Mutex<Option<HandleFn>>,
}

impl ACLStream {
    pub fn write(&self, data: &mut [u8], handler: Option<HandleFn>) -> nix::Result<()> {
        // let mut packet = Protocol::acl(self.handle, ATT_CID, data);

        debug!("writing {:?}", data);

        if self.cur_handler.lock() {

        }

        handle_error(unsafe {
            write(self.fd, data.as_mut_ptr() as *mut c_void, data.len()) as i32
        })?;
        Ok(())
    }

    pub fn receive(&self, message: Message) {
        // TODO: handle partial packets
        match message {
            Message::ACLDataPacket { cid, data, .. } => {
                let handler = self.cur_handler.lock().unwrap();
                handler.map(|f| {
                    f(cid, &data);
                });

            }
            _ => {
                panic!("should onl ysend ACLDataPackets here")
            }
        }
    }
}
