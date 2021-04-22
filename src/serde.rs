//! De-/Serialization with alternative formats.
//!
//! The various modules in here are intended to be used with `serde`'s [`with` annotation] to de-/serialize as something other than the default format.
//!
//! [`with` annotation]: https://serde.rs/attributes.html#field-attributes

/// Different de-/serialization formats for [`crate::api::BDAddr`].
pub mod bdaddr {
    pub use crate::api::bdaddr::serde::*;
}
