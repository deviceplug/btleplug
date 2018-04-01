use nix;
use nix::Errno;

use ::Result;
use Error;

fn errno_to_error(errno: Errno) -> Error {
    match errno {
        nix::Errno::EPERM => Error::PermissionDenied,
        nix::Errno::ENODEV => Error::DeviceNotFound,
        nix::Errno::ENOTCONN => Error::NotConnected,
        _ => Error::Other(errno.to_string())
    }
}

impl From<nix::Error> for Error {
    fn from(e: nix::Error) -> Self {
        match e {
            nix::Error::Sys(errno) => {
                errno_to_error(errno)
            },
            _ => {
                Error::Other(e.to_string())
            }
        }
    }
}

pub fn handle_error(v: i32) -> Result<i32> {
    if v < 0 {
        Err(errno_to_error(Errno::last()))
    } else {
        Ok(v)
    }
}
