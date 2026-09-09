//! CML-CONTRACT-SCOPE-REALIGN-M1 + CML-AUTO-CHECK-CONTRACT-VERSION-CLAIM
//!
//! Fail-closed guard: the global language contract claim in compatibility.my
//! must stay (2 0) until an explicit, evidence-backed claim upgrade. Partial
//! higher-contract features are allowed only when their status tokens contain
//! "partial" (or are explicitly subset-supported like reader 4.0) and never
//! silently imply a raised global claim.

use std::fs;
use std::path::PathBuf;

fn compatibility_path() -> PathBuf {
    // tests run with CARGO_MANIFEST_DIR = crate root
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("compatibility.my")
}

fn load_compat() -> String {
    fs::read_to_string(compatibility_path()).expect("compatibility.my must exist")
}

/// Extract the first `(contract . (MAJOR MINOR))` under the language section.
fn parse_global_contract(text: &str) -> (u32, u32) {
    // Prefer the claim-authority global-contract if present, else language contract.
    if let Some(idx) = text.find("(global-contract . (") {
        let rest = &text[idx + "(global-contract . (".len()..];
        return parse_pair(rest);
    }
    // First bare (contract . (N M)) after (language .
    let lang = text
        .find("(language .")
        .expect("compatibility.my must contain (language .");
    let after = &text[lang..];
    let marker = "(contract . (";
    let cidx = after
        .find(marker)
        .expect("language section must contain (contract . (N M))");
    parse_pair(&after[cidx + marker.len()..])
}

fn parse_pair(rest: &str) -> (u32, u32) {
    let end = rest.find(')').expect("closing paren for contract pair");
    let nums: Vec<u32> = rest[..end]
        .split_whitespace()
        .filter_map(|s| s.parse().ok())
        .collect();
    assert!(
        nums.len() >= 2,
        "expected (MAJOR MINOR) contract pair, got {:?}",
        &rest[..end]
    );
    (nums[0], nums[1])
}

fn parse_observed_upstream(text: &str) -> (u32, u32) {
    let marker = "(observed-upstream-contract . (";
    let idx = text
        .find(marker)
        .expect("observed-upstream-contract must be recorded");
    parse_pair(&text[idx + marker.len()..])
}

#[test]
fn global_language_contract_claim_is_2_0() {
    let text = load_compat();
    let (major, minor) = parse_global_contract(&text);
    assert_eq!(
        (major, minor),
        (2, 0),
        "global claim must remain (2 0) until an explicit upgrade with multi-backend evidence"
    );
}

#[test]
fn observed_upstream_is_at_least_claimed() {
    let text = load_compat();
    let claimed = parse_global_contract(&text);
    let observed = parse_observed_upstream(&text);
    assert!(
        observed >= claimed,
        "observed-upstream-contract {observed:?} must be >= claimed {claimed:?}"
    );
}

#[test]
fn higher_contract_gap_entries_are_not_full_claims() {
    let text = load_compat();
    // Any contract-N.0-* status that is not the global claim must not say
    // bare "supported" for 3.0/5.0/6.0 unless explicitly subset-scoped.
    // 4.0 reader is allowed as supported (subset).
    for (label, forbidden_full) in [
        ("contract-3.0-gap", true),
        ("contract-5.0-decimal-separator", true),
        ("contract-6.0-canon-reservation", true),
    ] {
        let idx = text
            .find(label)
            .unwrap_or_else(|| panic!("missing gap entry {label}"));
        // Look at a window after the label for status
        let window = &text[idx..idx.saturating_add(400).min(text.len())];
        let status_idx = window
            .find("(status . ")
            .unwrap_or_else(|| panic!("{label} missing status"));
        let status_rest = &window[status_idx + "(status . ".len()..];
        let end = status_rest
            .find(')')
            .unwrap_or_else(|| panic!("{label} status unclosed"));
        let status = status_rest[..end].trim();
        if forbidden_full {
            assert!(
                status.contains("partial")
                    || status == "unsupported"
                    || status.contains("subset"),
                "{label} status `{status}` must remain partial/unsupported/subset — \
                 a bare full claim would silently raise authority past global 2.0"
            );
        }
    }
}

#[test]
fn claim_authority_block_is_present() {
    let text = load_compat();
    assert!(
        text.contains("claim-authority"),
        "compatibility.my must declare claim-authority (CML-CONTRACT-SCOPE-REALIGN-M1)"
    );
    assert!(
        text.contains("partial-features-do-not-raise-claim"),
        "claim-authority must state partial-features-do-not-raise-claim"
    );
    assert!(
        text.contains("tests/contract_claim_authority_test.rs"),
        "claim-authority must point at this enforcing test"
    );
}
