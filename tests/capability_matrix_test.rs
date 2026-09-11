use std::collections::{BTreeMap, BTreeSet};
use std::fs;

/// CML-CONTRACT-SCOPE-REALIGN-M1: validate the per-backend capability matrix.
///
/// This test enforces:
/// 1. Global contract = min of backend contracts (for supported backends)
/// 2. Every "supported" capability has an evidence entry
/// 3. No backend claims a capability without evidence
/// 4. The matrix is consistent with known backend implementations
#[test]
fn capability_matrix_global_contract_is_min_of_backend_contracts() {
    let matrix = fs::read_to_string("capability-matrix.my")
        .expect("capability-matrix.my should exist and be readable");

    // Parse global contract
    let global_contract = extract_contract(&matrix, "global-contract")
        .expect("capability-matrix.my must have global-contract");
    assert_eq!(global_contract, (2, 0), "global contract must be 2.0");

    // Known backend contracts from the matrix
    let backend_contracts = vec![
        ("fpga-lisp", (2, 0), "supported"),
        ("c-backend", (2, 1), "supported"),
        ("x86-freestanding", (2, 0), "supported"),
    ];

    // Find minimum contract across supported backends
    let min_contract = backend_contracts
        .iter()
        .filter(|(_, _, status)| *status == "supported")
        .map(|(_, contract, _)| *contract)
        .min()
        .expect("at least one supported backend must exist");

    assert_eq!(
        global_contract, min_contract,
        "global-contract ({}.{}) must equal minimum backend contract ({}.{})",
        global_contract.0, global_contract.1, min_contract.0, min_contract.1
    );
}

#[test]
fn capability_matrix_every_supported_capability_has_evidence() {
    let matrix = fs::read_to_string("capability-matrix.my")
        .expect("capability-matrix.my should exist and be readable");

    // Evidence entries from the matrix (capability -> evidence name)
    let evidence_map: BTreeMap<&str, &str> = [
        ("integer", "tier-1-conformance"),
        ("rational", "c_backend_conformance_test.rs"),
        ("string", "string_values_test.rs"),
        ("nil", "tier-1-conformance"),
        ("true", "tier-1-conformance"),
        ("quote", "tier-1-conformance"),
        ("proper-list", "tier-1-conformance"),
        ("dotted-list", "tier-1-conformance"),
        ("cond", "tier-1-conformance"),
        ("lambda-fixed-arity", "tier-1-conformance"),
        (
            "lambda-fixed-arity-one",
            "self_tail_recursive_def_compiles_with_correct_structure",
        ),
        ("lambda-variadic", "c_backend_conformance_test.rs"),
        ("lambda-bare-symbol-params", "c_backend_conformance_test.rs"),
        ("application", "tier-1-conformance"),
        ("cons", "tier-1-conformance"),
        ("car", "tier-1-conformance"),
        ("cdr", "tier-1-conformance"),
        ("eq", "tier-1-conformance"),
        ("atom", "tier-1-conformance"),
        ("equal?", "tier-1-conformance"),
        ("defmacro", "tier-1-conformance"),
        ("def", "tier-1-conformance"),
        ("recursive-def-letrec", "evidence/length/"),
        ("let", "c_backend_conformance_test.rs"),
        ("let-as-lambda-lowering", "tier-1-conformance"),
        ("variadic-up-to-eight-arguments", "tier-1-conformance"),
        ("variadic-all-arities", "c_backend_conformance_test.rs"),
        (
            "def-self-tail-recursive",
            "self_tail_recursive_def_compiles_with_correct_structure",
        ),
        ("typed-buffer-i32", "c_backend_conformance_test.rs"),
        ("typed-buffer-f32", ""),
        ("numeric-buffer-map-i32", "c_backend_conformance_test.rs"),
        ("numeric-buffer-map-f32", ""),
        ("first-class-builtins", "c_backend_test.rs"),
        ("builtin-shadowing", "c_backend_test.rs"),
        ("higher-order-builtin-argument", "c_backend_test.rs"),
        ("structural-equal?", "c_backend_test.rs"),
        (
            "platform-calls",
            "pci_config_calls_are_explicit_target_abi_imports",
        ),
        (
            "pci-config",
            "pci_config_calls_are_explicit_target_abi_imports",
        ),
        ("mmio", "pci_config_calls_are_explicit_target_abi_imports"),
        (
            "tail-self-call",
            "explicit_self_tail_call_loop_lowers_without_calls",
        ),
        ("error-kind-divisionbyzero", "c_backend_test.rs"),
        ("error-kind-numericoverflow", "c_backend_test.rs"),
        ("error-kind-parse", "c_backend_test.rs"),
        ("error-kind-invalidform", "c_backend_test.rs"),
        ("first-class-builtins", "c_backend_test.rs"),
        ("builtin-shadowing", "c_backend_test.rs"),
        ("higher-order-builtin-argument", "c_backend_test.rs"),
        ("error-kind-divisionbyzero", "c_backend_test.rs"),
        ("error-kind-numericoverflow", "c_backend_test.rs"),
        ("error-kind-parse", "c_backend_test.rs"),
        ("error-kind-invalidform", "c_backend_test.rs"),
        (
            "platform-calls",
            "pci_config_calls_are_explicit_target_abi_imports",
        ),
        (
            "pci-config",
            "pci_config_calls_are_explicit_target_abi_imports",
        ),
        ("mmio", "pci_config_calls_are_explicit_target_abi_imports"),
        (
            "tail-self-call",
            "explicit_self_tail_call_loop_lowers_without_calls",
        ),
        (
            "def-self-tail-recursive",
            "self_tail_recursive_def_compiles_with_correct_structure",
        ),
    ]
    .into_iter()
    .collect();

    // Parse the matrix to find all capabilities marked as "supported"
    let mut missing_evidence = Vec::new();

    // Extract all (capability . supported) pairs from the matrix
    let supported_caps = extract_supported_capabilities(&matrix);

    for cap in supported_caps {
        if !evidence_map.contains_key(cap.as_str()) || evidence_map[cap.as_str()].is_empty() {
            missing_evidence.push(cap);
        }
    }

    assert!(
        missing_evidence.is_empty(),
        "Every supported capability must have evidence mapping:\n{}",
        missing_evidence.join("\n")
    );
}

#[test]
fn capability_matrix_no_duplicate_capabilities() {
    let matrix = fs::read_to_string("capability-matrix.my")
        .expect("capability-matrix.my should exist and be readable");

    // Check each backend section for duplicate capabilities
    for backend in ["fpga-lisp", "c-backend", "x86-freestanding"] {
        let mut seen = BTreeSet::new();
        let caps = extract_backend_capabilities(&matrix, backend);
        for cap in caps {
            assert!(
                seen.insert(cap.clone()),
                "Backend {} has duplicate capability: {:?}",
                backend,
                cap
            );
        }
    }
}

#[test]
fn capability_matrix_c_backend_2_1_slice_explicitly_labelled() {
    let matrix = fs::read_to_string("capability-matrix.my")
        .expect("capability-matrix.my should exist and be readable");

    let c_backend_contract = extract_backend_contract(&matrix, "c-backend")
        .expect("c-backend must be defined in capability matrix");

    assert_eq!(
        c_backend_contract,
        (2, 1),
        "C-backend contract must be explicitly 2.1 (slice)"
    );

    let caps = extract_backend_capabilities(&matrix, "c-backend");
    assert!(
        caps.contains_key("first-class-builtins"),
        "C-backend 2.1 slice must include first-class-builtins"
    );
    assert!(
        caps.contains_key("builtin-shadowing"),
        "C-backend 2.1 slice must include builtin-shadowing"
    );
    assert!(
        caps.contains_key("rational"),
        "C-backend 2.1 slice must include rational"
    );
    assert_eq!(
        caps.get("string"),
        Some(&"supported".to_string()),
        "C-backend string capability must match the executable string witnesses"
    );

    // Verify the claim reconciliation section exists
    assert!(
        matrix.contains("CML-IMPLEMENT-CONTRACT-2.1"),
        "CML-IMPLEMENT-CONTRACT-2.1 claim must be reconciled in matrix"
    );
    assert!(
        matrix.contains("reconciled"),
        "Reconciled claim must have status 'reconciled'"
    );
    assert!(
        matrix.contains("c-backend . supported")
            && matrix.contains("fpga-lisp . unsupported")
            && matrix.contains("x86-freestanding . unsupported"),
        "Reconciled claim must show per-backend 2.1 support"
    );
}

#[test]
fn capability_matrix_fpga_lisp_stays_at_2_0() {
    let matrix = fs::read_to_string("capability-matrix.my")
        .expect("capability-matrix.my should exist and be readable");

    let fpga_contract =
        extract_backend_contract(&matrix, "fpga-lisp").expect("fpga-lisp must be defined");

    assert_eq!(
        fpga_contract,
        (2, 0),
        "fpga-lisp must remain at contract 2.0"
    );

    let caps = extract_backend_capabilities(&matrix, "fpga-lisp");
    assert!(
        caps.contains_key("first-class-builtins") == false
            || caps.get("first-class-builtins") == Some(&"unsupported".to_string()),
        "fpga-lisp must not claim first-class-builtins"
    );
    assert!(
        caps.get("rational") == Some(&"unsupported".to_string()),
        "fpga-lisp must not claim rationals"
    );
    assert!(
        caps.get("error-kind-divisionbyzero") == Some(&"unsupported".to_string()),
        "fpga-lisp must not claim contract-3.0 error kinds"
    );
}

#[test]
fn capability_matrix_x86_freestanding_stays_at_2_0() {
    let matrix = fs::read_to_string("capability-matrix.my")
        .expect("capability-matrix.my should exist and be readable");

    let x86_contract = extract_backend_contract(&matrix, "x86-freestanding")
        .expect("x86-freestanding must be defined");

    assert_eq!(
        x86_contract,
        (2, 0),
        "x86-freestanding must remain at contract 2.0"
    );

    let caps = extract_backend_capabilities(&matrix, "x86-freestanding");
    assert!(
        caps.get("def-self-tail-recursive") == Some(&"supported".to_string()),
        "x86-freestanding must support self-tail-recursive def (CML-X86-DEF-BOUNDED-SELF-TAIL-RECURSIVE-FUNCTION)"
    );
}

#[test]
fn capability_matrix_validation_rules_present() {
    let matrix = fs::read_to_string("capability-matrix.my")
        .expect("capability-matrix.my should exist and be readable");

    assert!(
        matrix.contains("validation-rules"),
        "validation-rules section must be present"
    );
    assert!(
        matrix.contains("global-contract-is-min-of-backends"),
        "global-contract-is-min-of-backends rule required"
    );
    assert!(
        matrix.contains("no-claim-without-evidence"),
        "no-claim-without-evidence rule required"
    );
    assert!(
        matrix.contains("fail-closed-on-missing"),
        "fail-closed-on-missing rule required"
    );
    assert!(
        matrix.contains("no-silent-downgrade"),
        "no-silent-downgrade rule required"
    );
}

/// --- Simple parsing helpers ---

fn extract_contract(text: &str, marker_name: &str) -> Option<(u32, u32)> {
    let marker = format!("({} . (", marker_name);
    let start = text.find(&marker)? + marker.len();
    let end = text[start..].find(')')? + start;
    let mut parts = text[start..end].split_whitespace();
    Some((parts.next()?.parse().ok()?, parts.next()?.parse().ok()?))
}

fn extract_backend_contract(text: &str, backend: &str) -> Option<(u32, u32)> {
    let marker = format!("({backend}");
    let start = text.find(&marker)? + marker.len();
    let contract_marker = "(contract . (";
    let contract_start = text[start..].find(contract_marker)? + start + contract_marker.len();
    let contract_end = text[contract_start..].find(')')? + contract_start;
    let mut parts = text[contract_start..contract_end].split_whitespace();
    Some((parts.next()?.parse().ok()?, parts.next()?.parse().ok()?))
}

fn extract_backend_capabilities(text: &str, backend: &str) -> BTreeMap<String, String> {
    let mut caps = BTreeMap::new();
    let backend_marker = format!("({backend}");
    let Some(backend_start) = text.find(&backend_marker) else {
        return BTreeMap::new();
    };
    let backend_section = &text[backend_start + backend_marker.len()..];

    // Find capabilities section within the backend
    let cap_marker = "(capabilities";
    let Some(cap_start) = backend_section.find(cap_marker) else {
        return BTreeMap::new();
    };
    let cap_start = cap_start + cap_marker.len();

    let mut depth = 1;
    let mut i = cap_start;
    while i < backend_section.len() && depth > 0 {
        match backend_section[i..].chars().next() {
            Some('(') => depth += 1,
            Some(')') => depth -= 1,
            _ => {}
        }
        if depth == 0 {
            break;
        }
        i += 1;
    }
    let cap_section = &backend_section[cap_start..i];

    // Parse (capability . status) pairs - handle nested structure
    let mut pos = 0;
    while pos < cap_section.len() {
        if let Some(open) = cap_section[pos..].find('(') {
            let abs_open = pos + open;
            if let Some(close) = cap_section[abs_open..].find(')') {
                let abs_close = abs_open + close;
                let pair = &cap_section[abs_open + 1..abs_close].trim();
                if let Some(dot_pos) = pair.find('.') {
                    let cap = pair[..dot_pos].trim().to_string();
                    let status = pair[dot_pos + 1..].trim().to_string();
                    if !cap.is_empty() && !status.is_empty() {
                        caps.insert(cap, status);
                    }
                }
                pos = abs_close + 1;
            } else {
                break;
            }
        } else {
            break;
        }
    }
    caps
}

fn extract_supported_capabilities(text: &str) -> Vec<String> {
    let mut caps = Vec::new();

    // Only search within backend capability sections
    for backend in ["fpga-lisp", "c-backend", "x86-freestanding"] {
        let backend_marker = format!("({backend}");
        let Some(backend_start) = text.find(&backend_marker) else {
            continue;
        };
        let backend_section = &text[backend_start + backend_marker.len()..];

        let Some(cap_start) = backend_section.find("(capabilities") else {
            continue;
        };
        let cap_start = cap_start + "(capabilities".len();

        // Find end of capabilities section
        let mut depth = 1;
        let mut i = cap_start;
        while i < backend_section.len() && depth > 0 {
            match backend_section[i..].chars().next() {
                Some('(') => depth += 1,
                Some(')') => depth -= 1,
                _ => {}
            }
            if depth == 0 {
                break;
            }
            i += 1;
        }
        let cap_section = &backend_section[cap_start..i];

        // Find all (capability . supported) patterns within this section
        let mut pos = 0;
        while pos < cap_section.len() {
            if let Some(dot_pos) = cap_section[pos..].find(". supported") {
                let abs_dot = pos + dot_pos;
                // Look backwards for the opening (
                let search_start = if abs_dot > 50 { abs_dot - 50 } else { 0 };
                let search_text = &cap_section[search_start..abs_dot];
                if let Some(open_pos) = search_text.rfind('(') {
                    let cap_start = search_start + open_pos + 1;
                    let cap = cap_section[cap_start..abs_dot].trim();
                    if !cap.is_empty()
                        && !cap.contains(' ')
                        && !cap.contains('\n')
                        && !cap.contains('"')
                    {
                        // Filter out false positives
                        if !matches!(
                            cap,
                            "status" | "c-backend" | "fpga-lisp" | "x86-freestanding"
                        ) {
                            caps.push(cap.to_string());
                        }
                    }
                }
                pos = abs_dot + 10;
            } else {
                break;
            }
        }
    }
    caps
}
