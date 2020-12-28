// btleplug Source Code File
//
// Copyright 2020 Nonpolynomial Labs LLC. All rights reserved.
//
// Licensed under the BSD 3-Clause license. See LICENSE file in the project root
// for full license information.
//
// Some portions of this file are taken and/or modified from Rumble
// (https://github.com/mwylde/rumble), using a dual MIT/Apache License under the
// following copyright:
//
// Copyright (c) 2014 The Rust Project Developers

use libc;

use crate::{
    api::{BDAddr, Characteristic, CommandCallback, NotificationHandler, RequestCallback, UUID},
    bluez::util::handle_error,
    Error, Result,
};

use std::{
    collections::BTreeSet,
    fmt::{self, Debug, Formatter},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{channel, Receiver, Sender},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};

enum StreamMessage {
    Command(Vec<u8>, Option<CommandCallback>),
    Request(Vec<u8>, Option<RequestCallback>),
    ConfirmIndication,
    Data(Vec<u8>),
}

use StreamMessage::*;

use super::Adapter;

impl Debug for StreamMessage {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            &Command(ref data, ref _cb) => write!(f, "Command({:?})", data),
            &Request(ref data, ref cb) => write!(f, "Request({:?}, cb: {})", data, cb.is_some()),
            &ConfirmIndication => write!(f, "ConfirmIndication"),
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
    // In order to get the UUID of our handle on notifications, we need to share
    // the characteristics vector with a peripheral owner. There may be a nicer
    // way to do this but I really don't care at the moment.
    characteristics: Arc<Mutex<BTreeSet<Characteristic>>>,
}

impl ACLStream {
    pub fn new(
        adapter: Adapter,
        address: BDAddr,
        characteristics: Arc<Mutex<BTreeSet<Characteristic>>>,
        handle: u16,
        fd: i32,
        notification_handlers: Arc<Mutex<Vec<NotificationHandler>>>,
    ) -> ACLStream {
        info!("Creating new ACLStream for {}, {}, {}", address, handle, fd);
        let (tx, rx) = channel();
        let acl_stream = ACLStream {
            adapter,
            address,
            handle,
            fd,
            characteristics,
            should_stop: Arc::new(AtomicBool::new(false)),
            sender: Arc::new(Mutex::new(tx)),
            notification_handlers,
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

    fn write_socket(
        &self,
        value: &mut [u8],
        command: bool,
        receiver: &Receiver<StreamMessage>,
    ) -> Result<Vec<u8>> {
        debug!("writing {:?}", value);
        handle_error(unsafe {
            libc::write(
                self.fd,
                value.as_mut_ptr() as *mut libc::c_void,
                value.len(),
            ) as i32
        })?;

        let mut skipped = vec![];
        loop {
            let message = receiver.recv().unwrap();
            debug!("waiting for confirmation... {:?}", message);
            if let Data(rec) = message {
                if rec != value {
                    skipped.into_iter().for_each(|m| self.send(m));
                    return Ok(rec);
                } else if command {
                    return Ok(vec![]);
                }
            } else {
                skipped.push(message);
            }
        }
    }

    fn handle_iteration(
        &self,
        msg: &mut StreamMessage,
        receiver: &Receiver<StreamMessage>,
    ) -> Result<()> {
        match *msg {
            Command(ref mut value, ref handler) => {
                debug!("sending command {:?} to {}", value, self.fd);

                let result = self.write_socket(value, true, receiver).map(|_v| ());
                if let &Some(ref f) = handler {
                    f(result);
                }
            }
            Request(ref mut value, ref handler) => {
                debug!("sending request {:?} to {}", value, self.fd);

                let result = self.write_socket(value, false, receiver);
                if let &Some(ref f) = handler {
                    f(result);
                }
            }
            ConfirmIndication => {
                debug!("confirming indication to {}", self.fd);

                unimplemented!()
            }
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

    fn receive_notification(&self, _value: &[u8]) -> () {
        unimplemented!()
    }

    fn get_uuid_by_handle(&self, handle: u16) -> Option<UUID> {
        for c in self.characteristics.lock().unwrap().iter() {
            if c.value_handle == handle {
                return Some(c.uuid);
            }
        }
        None
    }
}

impl Drop for ACLStream {
    fn drop(&mut self) {
        self.should_stop.clone().store(true, Ordering::Relaxed);
    }
}
