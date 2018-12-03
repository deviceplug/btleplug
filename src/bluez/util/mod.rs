use nix;
use nix::errno::Errno;

use ::Result;
use Error;

fn errno_to_error(errno: Errno) -> Error {
    match errno {
        Errno::EPERM => Error::PermissionDenied,
        Errno::ENODEV => Error::DeviceNotFound,
        Errno::ENOTCONN => Error::NotConnected,
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
        debug!("got error {}", Errno::last());
        Err(errno_to_error(Errno::last()))
    } else {
        Ok(v)
    }
}
