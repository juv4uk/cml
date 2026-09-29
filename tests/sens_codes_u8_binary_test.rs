// Fix: lambda body can only be 1 expr; use nested or separate calls
use cml::c_backend::CBackend;
use cml::lower;
use cml::macros::MacroExpander;
use cml::parser;
use std::collections::HashSet;
use std::fs;

fn read_canonical_sens_sids() -> (HashSet<u8>, usize) {
    let path = "external/sens/lib/generated/function-table.lisp";
    let content = fs::read_to_string(path)
        .expect("CRITICAL: canonical function table not found. Run `git -c protocol.file.allow=always submodule update --init external/sens && cd external/sens && git checkout a69f4dd1246f668a4bfefa3bf11863a1def24af3`");

    let mut sids = HashSet::new();
    let mut count = 0;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("(0") {
            if let Some(sid_str) = trimmed.split_whitespace().next() {
                let sid_str = sid_str.trim_start_matches('(');
                if sid_str.len() == 8 && sid_str.chars().all(|c| c == '0' || c == '1') {
                    if let Ok(sid_val) = u8::from_str_radix(sid_str, 2) {
                        sids.insert(sid_val);
                        count += 1;
                    }
                }
            }
        }
    }

    assert_eq!(
        count, 128,
        "Canonical SENS function table has {} entries (expected 128 on pinned a69f4dd)",
        count
    );
    assert_eq!(sids.len(), 128, "All 128 SIDs must be unique");
    (sids, count)
}

#[test]
fn sens_identity_is_u8_emitted_as_binary_literal() {
    let exprs = parser::parse("(+ 1 2)").expect("parse");
    let exprs = MacroExpander::new().process(&exprs).expect("macro expand");
    let program = lower::lower_program(&exprs).expect("lower");
    let mut backend = CBackend::new();
    let c_source = backend.compile_program(&program).expect("compile");

    assert!(
        c_source.contains("mk_sid_callable(0b00001100)"),
        "plus SID must be emitted as 0b binary literal (u8)"
    );
    assert!(
        c_source.contains("uint8_t sid"),
        "SID parameter type in runtime must be uint8_t"
    );
}

#[test]
fn emitted_sids_are_exactly_8_bit_binary() {
    let exprs = parser::parse(
        "
        (define plus-closure (lambda (a b) (+ a b)))
        (plus-closure 1 2)
    ",
    )
    .expect("parse");
    let exprs = MacroExpander::new().process(&exprs).expect("macro expand");
    let program = lower::lower_program(&exprs).expect("lower");
    let mut backend = CBackend::new();
    let c_source = backend.compile_program(&program).expect("compile");

    let re = regex::Regex::new(r"mk_sid_callable\(0b([01]{8})\)").unwrap();
    let mut found = 0;
    for cap in re.captures_iter(&c_source) {
        let bits = &cap[1];
        assert_eq!(bits.len(), 8, "SID literal must be exactly 8 bits: {bits}");
        let val = u8::from_str_radix(bits, 2).expect(&format!("valid 8-bit binary: {bits}"));
        assert!(val <= 255, "SID {bits} must fit in u8");
        found += 1;
    }
    assert!(found > 0, "At least one SID must be emitted in binary form");
}

#[test]
fn emitted_sids_are_subset_of_canonical_table() {
    let (canonical, _) = read_canonical_sens_sids();

    // Use separate lambdas/lets to avoid single-body limit
    let exprs = parser::parse(
        "
        (define test1 (lambda (x y) (cons x y)))
        (define test2 (lambda (x) (atom? x)))
        (define test3 (lambda (x y) (eq? x y)))
        (define test4 (lambda (x y) (+ x y)))
        (define test5 (lambda (x y) (- x y)))
        (define test6 (lambda (x y) (* x y)))
        (define test7 (lambda (x y) (/ x y)))
    ",
    )
    .expect("parse");
    let exprs = MacroExpander::new().process(&exprs).expect("macro expand");
    let program = lower::lower_program(&exprs).expect("lower");
    let mut backend = CBackend::new();
    let c_source = backend.compile_program(&program).expect("compile");

    let re = regex::Regex::new(r"mk_sid_callable\(0b([01]{8})\)").unwrap();
    let mut emitted = HashSet::new();
    for cap in re.captures_iter(&c_source) {
        let bits = &cap[1];
        let sid_val = u8::from_str_radix(bits, 2).expect("valid 8-bit");
        emitted.insert(sid_val);
    }

    for sid in &emitted {
        assert!(
            canonical.contains(sid),
            "Emitted SID 0b{:08b} not found in canonical 128-table",
            sid
        );
    }

    println!(
        "  Emitted SIDs: {}; all present in canonical table ({}/128)",
        emitted.len(),
        emitted.len()
    );
}

#[test]
fn runtime_dispatch_sids_are_canonical_and_coverage_reported_honestly() {
    let (canonical, total) = read_canonical_sens_sids();

    let exprs = parser::parse("(+ 1 2)").expect("parse");
    let exprs = MacroExpander::new().process(&exprs).expect("macro expand");
    let program = lower::lower_program(&exprs).expect("lower");
    let mut backend = CBackend::new();
    let c_source = backend.compile_program(&program).expect("compile");

    let re = regex::Regex::new(r"case 0b([01]{8}):").unwrap();
    let mut dispatch_sids = HashSet::new();
    for cap in re.captures_iter(&c_source) {
        let bits = &cap[1];
        let sid_val = u8::from_str_radix(bits, 2).expect("valid 8-bit in dispatch");
        dispatch_sids.insert(sid_val);
    }

    for sid in &dispatch_sids {
        assert!(
            canonical.contains(sid),
            "Dispatch SID 0b{:08b} not canonical",
            sid
        );
    }

    let covered = dispatch_sids.len();
    println!(
        "  Runtime dispatch covers {}/{} canonical SIDs (honest subset)",
        covered, total
    );
    assert!(covered > 0, "At least one SID must be dispatched");
    assert!(
        covered < total,
        "Coverage is honestly a subset; currently {} of {}",
        covered,
        total
    );
}

#[test]
fn no_english_authority_in_sens_identity() {
    let exprs = parser::parse("(atom? (quote test))").expect("parse");
    let exprs = MacroExpander::new().process(&exprs).expect("macro expand");
    let program = lower::lower_program(&exprs).expect("lower");
    let mut backend = CBackend::new();
    let c_source = backend.compile_program(&program).expect("compile");

    assert!(
        c_source.contains("mk_sid_callable(0b00000010)"),
        "atom? must compile to its 8-bit SID, not to an English string"
    );

    assert!(
        !c_source.contains("mk_sid_callable(\"atom?\""),
        "SID must not be emitted as an English string"
    );
    assert!(
        !c_source.contains("mk_sens(\"atom?\""),
        "Legacy mk_sens with string must not appear"
    );
}
