extern crate libc;

#[macro_use]
extern crate log;

#[macro_use]
extern crate nix;

extern crate bytes;
#[macro_use] extern crate enum_primitive;
extern crate num;

#[macro_use]
extern crate nom;

#[macro_use]
extern crate bitflags;

extern crate failure;
#[macro_use]
extern crate failure_derive;

use std::result;
use std::time::Duration;

pub mod bluez;
pub mod api;

#[derive(Debug, Fail, Clone)]
pub enum Error {
    #[fail(display = "Permission denied")]
    PermissionDenied,

    #[fail(display = "Device not found")]
    DeviceNotFound,

    #[fail(display = "Not connected")]
    NotConnected,

    #[fail(display = "The operation is not supported: {}", _0)]
    NotSupported(String),

    #[fail(display = "Timed out after {:?}", _0)]
    TimedOut(Duration),

    #[fail(display = "{}", _0)]
    Other(String)
}

// Rumble Result type
pub type Result<T> = result::Result<T, Error>;
