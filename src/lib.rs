// #![allow(warnings)]

extern crate libc;

#[macro_use]
extern crate log;
extern crate env_logger;

#[macro_use]
extern crate nix;

extern crate bincode;

#[macro_use]
extern crate serde_derive;
extern crate serde;

extern crate bytes;
#[macro_use]
extern crate enum_primitive;
extern crate num;

#[macro_use]
extern crate nom;

pub mod adapter;
pub mod manager;
pub mod device;
mod util;

