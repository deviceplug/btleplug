//! The `platform` module contains the platform-specific implementations of the various [`api`]
//! traits. Refer for the `api` module for how to use them.

#[cfg(target_os = "linux")]
pub use crate::bluez::{adapter::Adapter, manager::Manager, peripheral::Peripheral};
#[cfg(any(target_os = "macos", target_os = "ios"))]
pub use crate::corebluetooth::{adapter::Adapter, manager::Manager, peripheral::Peripheral};
#[cfg(target_os = "windows")]
pub use crate::winrtble::{adapter::Adapter, manager::Manager, peripheral::Peripheral};

use crate::api::{self, Central};
use static_assertions::assert_impl_all;
use std::fmt::Debug;

// Ensure that the exported types implement all the expected traits.
assert_impl_all!(Adapter: Central, Clone, Debug, Send, Sized, Sync);
assert_impl_all!(Manager: api::Manager, Clone, Debug, Send, Sized, Sync);
assert_impl_all!(Peripheral: api::Peripheral, Clone, Debug, Send, Sized, Sync);
