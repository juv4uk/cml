use cml::ir::{Ir, PrimOp};
use cml::{lower, parser};

#[test]
fn exact_q_less_equal_does_not_fall_back_to_generic_call_or_atom_eq() {
    // Upstream authority: my-lisp contracts/exact-q-binary-contract.lisp,
    // semantic identity 1017. Its result domain is exact-Q 0/1, not t/nil
    // and not semantic 0003 atom identity.
    let expressions = parser::parse("(<= 128 191)").expect("exact-Q comparison source must parse");
    let lowered = lower::lower_program(&expressions)
        .expect("a stable Canon comparison identity must reach compiler lowering");

    let [node] = lowered.as_slice() else {
        panic!("expected one lowered expression, got {lowered:?}");
    };

    match node {
        Ir::Prim { op, args } => {
            assert_ne!(
                *op,
                PrimOp::Eq,
                "numeric <= must not be conflated with semantic 0003 atom identity"
            );
            assert_eq!(args.len(), 2, "binary exact-Q <= must keep both operands");
        }
        other => panic!(
            "semantic identity 1017 must lower to an admitted numeric comparison primitive, got {other:?}"
        ),
    }
}
