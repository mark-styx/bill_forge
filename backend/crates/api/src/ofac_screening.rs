//! OFAC screening re-export shim.
//!
//! The screener implementation was lifted into the shared `billforge_vendor_mgmt`
//! crate so the VendorRiskRescan worker (which cannot depend on the API crate)
//! can call `OfacScreener::screen_vendor` on its tenant pools. This module
//! re-exports the screener so existing API call sites that use
//! `crate::ofac_screening::OfacScreener` keep compiling unchanged.

pub use billforge_vendor_mgmt::ofac_screening::{
    OfacMatch, OfacScreenOutcome, OfacScreener, SanctionsEntry,
};
