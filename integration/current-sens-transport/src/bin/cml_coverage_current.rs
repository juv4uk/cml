//! Execute ACTUAL root CML coverage CLI source and root coverage module,
//! compiled with the exact current SENS candidate. No duplicated formatter,
//! old byte-ID aliases, or release-grade native compiler claims.
extern crate self as cml;

#[path = "../../../../src/coverage.rs"]
pub mod coverage;

include!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../src/bin/cml-coverage.rs"));
