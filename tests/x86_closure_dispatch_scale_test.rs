//! CML-X86-CLOSURE-DISPATCH-SCALABILITY: makes the linear-per-call-site
//! dispatch cost documented on `emit_single_argument_closure_call` in
//! src/x86_freestanding.rs measurable, instead of asserted only in a
//! comment. Not a regression test for a bug -- the current behavior is
//! correct and fail-closed -- a benchmark-shaped test that must be re-run
//! (and its formula re-derived) before any dispatch redesign is justified.

use cml::lower;
use cml::parser;
use cml::x86_freestanding::X86FreestandingBackend;

/// `n` independent escaping closures, each materialized and immediately
/// called once, compiled together as one program.
fn closure_call_source(n: usize) -> String {
    let mut src = String::new();
    for i in 0..n {
        src.push_str(&format!(
            "(((lambda (x) (lambda (y) (cons x y))) (quote A{i})) (quote B{i})) "
        ));
    }
    src
}

fn total_dispatch_comparisons(n: usize) -> usize {
    let expressions = parser::parse(&closure_call_source(n)).unwrap();
    let program = lower::lower_program(&expressions).unwrap();
    let assembly = X86FreestandingBackend::new()
        .compile_program(&program)
        .expect("bounded escaping closures should compile");
    assembly.matches("cmpl $").count()
}

#[test]
fn escaping_closure_dispatch_cost_grows_quadratically_with_closure_count() {
    // emit_single_argument_closure_call dispatches a call via a linear
    // cmpl/jne chain against every closure compiled into the unit *so far*,
    // not just ones reachable from that call site. With N closures compiled
    // in sequence, the K-th call site's chain covers all K closures known by
    // then, so the *total* comparisons emitted across the whole program are
    // the triangular number N*(N+1)/2 -- quadratic in N, not linear.
    let small = total_dispatch_comparisons(3);
    let large = total_dispatch_comparisons(30);

    assert_eq!(
        small,
        3 * 4 / 2,
        "3 closures should yield the triangular count 6"
    );
    assert_eq!(
        large,
        30 * 31 / 2,
        "30 closures should yield the triangular count 465"
    );
    assert!(
        large > small * 20,
        "dispatch cost should grow much faster than the 10x increase in closure \
         count (found {small} -> {large}); if this ever stops holding, the \
         O(n) claim on emit_single_argument_closure_call in \
         src/x86_freestanding.rs needs re-verifying, not just re-asserting"
    );
}
