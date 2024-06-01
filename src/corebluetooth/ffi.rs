#![allow(non_camel_case_types)]
use std::os::raw::{c_char, c_void};

pub type dispatch_object_s = c_void;
pub type dispatch_queue_t = *mut dispatch_object_s;
pub type dispatch_queue_attr_t = *const dispatch_object_s;

pub const DISPATCH_QUEUE_SERIAL: dispatch_queue_attr_t = 0 as dispatch_queue_attr_t;

extern "C" {
    pub fn dispatch_queue_create(
        label: *const c_char,
        attr: dispatch_queue_attr_t,
    ) -> dispatch_queue_t;
}

// TODO: Do we need to link to AppKit here?
#[cfg_attr(target_os = "macos", link(name = "AppKit", kind = "framework"))]
extern "C" {}
