#[cfg(target_os = "linux")]
pub use crate::bluez::{adapter::Adapter, manager::Manager};
#[cfg(any(target_os = "macos", target_os = "ios"))]
pub use crate::corebluetooth::{adapter::Adapter, manager::Manager};
#[cfg(target_os = "windows")]
pub use crate::winrtble::{adapter::Adapter, manager::Manager};
