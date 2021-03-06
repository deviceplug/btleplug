#[cfg(target_os = "linux")]
pub use crate::bluez::{
    adapter::{peripheral::Peripheral, Adapter},
    manager::Manager,
};
#[cfg(any(target_os = "macos", target_os = "ios"))]
pub use crate::corebluetooth::{adapter::Adapter, manager::Manager, peripheral::Peripheral};
#[cfg(target_os = "windows")]
pub use crate::winrtble::{adapter::Adapter, manager::Manager, peripheral::Peripheral};

use crate::api::{self, Central};
use static_assertions::assert_impl_all;

// Ensure that the exported types implement all the expected traits.
// TODO: Add `Debug`.
assert_impl_all!(Adapter: Central, Clone, Send, Sized, Sync);
assert_impl_all!(Manager: Clone, Send, Sized, Sync);
assert_impl_all!(Peripheral: api::Peripheral, Clone, Send, Sized, Sync);
