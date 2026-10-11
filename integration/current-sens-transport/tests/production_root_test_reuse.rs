//! Recompile ACTUAL root CML source and its ORIGINAL test suite with modern
//! SENS exact-domain crate, without hiding the unrelated old backend blockers.
//! The cml alias resolves the original root test import to this test crate.
extern crate self as cml;

#[path = "../../../src/core_profile.rs"]
pub mod core_profile;

#[path = "../../../tests/core_profile_request_test.rs"]
mod original_production_tests;
