//! Analytics Data Transfer Objects
//!
//! Re-exports from `models` for backward compatibility. The unsafe
//! `create_router` and Path-based handler wrappers have been removed;
//! all analytics endpoints are now served from `api::routes::analytics`
//! under the authenticated `/api/v1/analytics` namespace.

pub use crate::models::*;
