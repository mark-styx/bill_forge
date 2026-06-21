//! Security primitives re-exported from `billforge-core::security`.
//!
//! Kept as a thin alias so call sites in `api` can use `crate::security::*`
//! without coupling to the path inside the shared crate.

pub use billforge_core::security::{CipherError, TokenCipher};
