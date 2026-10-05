use std::collections::HashSet;

use cml::compiler_mechanism::{
    RichCompilerMechanismRef, select_rich_compiler_mechanism,
};
use sens::syntax::{Expr, ExprKind};
use sens::{compiler_lowering_role_from_sens, lower_program, parse, CompilerLoweringRole};

const NUCLEUS: &str = include_str!("../external/sens/lib/compiler-nucleus.lisp");

fn collect_verified_bindings(
    expr: &Expr,
    roles: &mut HashSet<CompilerLoweringRole>,
    mechanisms: &mut HashSet<RichCompilerMechanismRef>,
) {
    match &expr.kind {
        ExprKind::DomainCall(identity, args) => {
            let role = compiler_lowering_role_from_sens(*identity)
                .unwrap_or_else(|error| {
                    panic!("pinned SENS role law failed for {identity}: {error:?}")
                })
                .unwrap_or_else(|| {
                    panic!(
                        "current compiler nucleus contains an exact-domain call without an admitted lowering role: {identity}"
                    )
                });
            let mechanism = select_rich_compiler_mechanism(role);
            roles.insert(role);
            mechanisms.insert(mechanism);

            for arg in args.iter() {
                collect_verified_bindings(arg, roles, mechanisms);
            }
        }
        ExprKind::List(items) => {
            for item in items.iter() {
                collect_verified_bindings(item, roles, mechanisms);
            }
        }
        ExprKind::Pair(head, tail) => {
            collect_verified_bindings(head, roles, mechanisms);
            collect_verified_bindings(tail, roles, mechanisms);
        }
        ExprKind::Sid(sid) => panic!("legacy Sid entered current selfhost source closure: {sid}"),
        ExprKind::Call(sid, _) => {
            panic!("legacy Call entered current selfhost source closure: {sid}")
        }
        ExprKind::Number(_, _)
        | ExprKind::Rational(_)
        | ExprKind::BinaryNumber(_)
        | ExprKind::NumericBuffer(_)
        | ExprKind::DomainIdentity(_)
        | ExprKind::String(_)
        | ExprKind::Symbol(_)
        | ExprKind::Local { .. } => {}
    }
}

#[test]
fn real_pinned_compiler_nucleus_reaches_all_nine_rich_mechanisms_via_sens_roles() {
    let parsed = parse(NUCLEUS).expect("pinned current compiler nucleus parses");
    let lowered = lower_program(&parsed);

    let mut roles = HashSet::new();
    let mut mechanisms = HashSet::new();
    for expr in &lowered {
        collect_verified_bindings(expr, &mut roles, &mut mechanisms);
    }

    assert_eq!(
        roles.len(),
        9,
        "real current nucleus must exercise the complete nine-role SENS closure: {roles:?}"
    );
    assert_eq!(
        mechanisms.len(),
        9,
        "every verified SENS role must bind to one distinct CML mechanism family: {mechanisms:?}"
    );

    for expected in [
        RichCompilerMechanismRef::Quote,
        RichCompilerMechanismRef::AtomPredicateD1,
        RichCompilerMechanismRef::SelectorTail,
        RichCompilerMechanismRef::SelectorHead,
        RichCompilerMechanismRef::AtomEqualityD1,
        RichCompilerMechanismRef::ConditionalD1,
        RichCompilerMechanismRef::PairConstruct,
        RichCompilerMechanismRef::Lambda,
        RichCompilerMechanismRef::Define,
    ] {
        assert!(
            mechanisms.contains(&expected),
            "real current nucleus did not reach expected CML mechanism {expected:?}"
        );
    }
}

#[test]
fn current_role_binding_source_has_no_host_role_or_identity_switch() {
    let mechanism_source = include_str!("../src/compiler_mechanism.rs");
    assert!(!mechanism_source.contains("DomainIdentity"));
    assert!(!mechanism_source.contains("packed_bits"));
    assert!(!mechanism_source.contains("Sid8"));
    assert!(!mechanism_source.contains("Sens8"));
    assert!(mechanism_source.contains("CompilerLoweringRole"));
}
