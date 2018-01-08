use libc::{write, c_void};
use bytes::{BytesMut, BufMut, LittleEndian};
use nix;

use ::constants::*;
use ::util::handle_error;

pub struct Protocol {
}

impl Protocol {
    pub fn hci(command: u16, data: &[u8]) -> BytesMut {
        let mut buf = BytesMut::with_capacity(4 + data.len());

        // header
        buf.put_u8(HCI_COMMAND_PKT);
        buf.put_u16::<LittleEndian>(command);

        // len
        buf.put_u8(data.len() as u8);

        // data
        buf.put(data);
        buf
    }

    pub fn write(fd: i32, message: &mut [u8]) -> nix::Result<()> {
        debug!("writing({}) {:?}", fd, message);
        let ptr = message.as_mut_ptr();
        handle_error(unsafe {
            write(fd, ptr as *mut _ as *mut c_void, message.len()) as i32
        })?;
        Ok(())
    }
}
