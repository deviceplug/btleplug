use nix;
use nix::Errno;

pub fn handle_error(v: i32) -> nix::Result<i32> {
    if v < 0 {
        Err(nix::Error::Sys(Errno::last()))
    } else {
        Ok(v)
    }
}
