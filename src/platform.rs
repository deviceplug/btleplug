#[cfg(target_os = "linux")]
pub use crate::bluez::{adapter::Adapter, manager::Manager};
#[cfg(any(target_os = "macos", target_os = "ios"))]
pub use crate::corebluetooth::{adapter::Adapter, manager::Manager};
#[cfg(target_os = "windows")]
pub use crate::winrtble::{adapter::Adapter, manager::Manager};

use crate::api::Central;
use static_assertions::assert_impl_all;

// Ensure that the exported types implement all the expected traits.
// TODO: Add `Debug`.
assert_impl_all!(Adapter: Central, Clone, Send, Sized, Sync);
assert_impl_all!(Manager: Clone, Send, Sized, Sync);
