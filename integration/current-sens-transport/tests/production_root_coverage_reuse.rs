//! Run ORIGINAL root CML coverage tests against the actual root production
//! src/coverage.rs, but compile with modern SENS while other old backend
//! modules remain blocked. The alias is only a test-import path; no alternate
//! implementation, SID alias, or hidden production module is introduced.
extern crate self as cml;

#[path = "../../../src/coverage.rs"]
pub mod coverage;

#[path = "../../../tests/coverage_ledger_test.rs"]
mod original_current_domain_coverage;

#[path = "../../../tests/coverage_x86_canon_reconciliation_test.rs"]
mod preserved_x86_core_witness_audit;

#[path = "../../../tests/coverage_x86_fixnum_limit_test.rs"]
mod preserved_x86_fixnum_witness_audit;
