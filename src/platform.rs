//! The `platform` module contains the platform-specific implementations of the various [`api`]
//! traits. Refer for the `api` module for how to use them.

#[cfg(target_os = "linux")]
pub use crate::bluez::{
    adapter::Adapter, manager::Manager, peripheral::Peripheral, peripheral::PeripheralId,
};
#[cfg(any(target_os = "macos", target_os = "ios"))]
pub use crate::corebluetooth::{
    adapter::Adapter, manager::Manager, peripheral::Peripheral, peripheral::PeripheralId,
};
#[cfg(target_os = "windows")]
pub use crate::winrtble::{
    adapter::Adapter, manager::Manager, peripheral::Peripheral, peripheral::PeripheralId,
};

use crate::api::{self, Central};
use api::PeripheralIdent;
use static_assertions::assert_impl_all;
use std::borrow::Borrow;
use std::fmt::Debug;
use std::hash::{Hash, Hasher};

impl PeripheralIdent for PeripheralId {
    fn id(&self) -> Self {
        self.clone()
    }
    fn get_id(&self) -> &Self {
        self
    }
}

// This implements hash based on the PeripheralId of a Peripheral.
// Peripheral doesn't currently implement Hash, and you could consider that the same Peripheral
// connected to via different Adapters should hash differently
pub struct PeripheralIdKeyed {
    peripheral: Peripheral,
}

impl Borrow<PeripheralId> for PeripheralIdKeyed {
    fn borrow(&self) -> &PeripheralId {
        self.get_id()
    }
}

impl From<Peripheral> for PeripheralIdKeyed {
    fn from(peripheral: Peripheral) -> PeripheralIdKeyed {
        PeripheralIdKeyed { peripheral }
    }
}

impl PeripheralIdKeyed {
    pub fn peripheral(self) -> Peripheral {
        self.peripheral
    }
}

impl PeripheralIdent for PeripheralIdKeyed {
    fn id(&self) -> PeripheralId {
        self.peripheral.id()
    }
    fn get_id(&self) -> &PeripheralId {
        self.peripheral.get_id()
    }
}

impl Eq for PeripheralIdKeyed {}
impl PartialEq for PeripheralIdKeyed {
    fn eq(&self, other: &PeripheralIdKeyed) -> bool {
        *self.peripheral.get_id() == *other.peripheral.get_id()
    }
}

impl Hash for PeripheralIdKeyed {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.get_id().hash(state)
    }
}

// Ensure that the exported types implement all the expected traits.
assert_impl_all!(Adapter: Central, Clone, Debug, Send, Sized, Sync);
assert_impl_all!(Manager: api::Manager, Clone, Debug, Send, Sized, Sync);
assert_impl_all!(Peripheral: api::Peripheral, Clone, Debug, Send, Sized, Sync);
assert_impl_all!(PeripheralId: Clone, Debug, Send, Sized, Sync, Eq, PartialEq);
