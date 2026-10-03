use std::collections::BTreeSet;

use cml::lower;
use cml::parser;
use cml::x86_freestanding::X86FreestandingBackend;

#[derive(Debug)]
struct Certificate {
    id: usize,
    kind: String,
    frame: usize,
    stack_roots: BTreeSet<usize>,
    register_roots: BTreeSet<String>,
}

fn field<'a>(parts: impl Iterator<Item = &'a str>, key: &str) -> &'a str {
    parts
        .filter_map(|part| part.split_once('='))
        .find_map(|(name, value)| (name == key).then_some(value))
        .unwrap_or_else(|| panic!("missing field {key}"))
}

fn parse_certificates(assembly: &str) -> Vec<Certificate> {
    let mut certificates: Vec<Certificate> = Vec::new();

    for raw in assembly.lines() {
        let line = raw.trim();
        if let Some(rest) = line.strip_prefix("# GC_SAFEPOINT ") {
            let parts: Vec<_> = rest.split_whitespace().collect();
            let id = field(parts.iter().copied(), "id").parse().unwrap();
            let kind = field(parts.iter().copied(), "kind").to_string();
            let frame = field(parts.iter().copied(), "frame").parse().unwrap();
            certificates.push(Certificate {
                id,
                kind,
                frame,
                stack_roots: BTreeSet::new(),
                register_roots: BTreeSet::new(),
            });
        } else if let Some(rest) = line.strip_prefix("# GC_STACK_ROOT ") {
            let parts: Vec<_> = rest.split_whitespace().collect();
            let id: usize = field(parts.iter().copied(), "id").parse().unwrap();
            let offset: usize = field(parts.iter().copied(), "offset").parse().unwrap();
            let current = certificates
                .last_mut()
                .expect("stack root must follow safepoint");
            assert_eq!(current.id, id);
            assert!(current.stack_roots.insert(offset), "duplicate stack root");
        } else if let Some(rest) = line.strip_prefix("# GC_REGISTER_ROOT ") {
            let parts: Vec<_> = rest.split_whitespace().collect();
            let id: usize = field(parts.iter().copied(), "id").parse().unwrap();
            let reg = field(parts.iter().copied(), "reg").to_string();
            let current = certificates
                .last_mut()
                .expect("register root must follow safepoint");
            assert_eq!(current.id, id);
            assert!(
                current.register_roots.insert(reg),
                "duplicate register root"
            );
        }
    }

    certificates
}

#[test]
fn pack_rest_safepoints_emit_exact_shrinking_root_locations() {
    let expressions = parser::parse("((lambda (a b . rest) rest) 10 20 30 40 50)").unwrap();
    let program = lower::lower_program(&expressions).unwrap();
    let assembly = X86FreestandingBackend::new()
        .compile_program(&program)
        .expect("all-rest lambda must compile");

    let certificates = parse_certificates(&assembly);
    assert_eq!(
        certificates.len(),
        3,
        "three rest values require three allocating wsm_cons safepoints"
    );

    let stack_counts: Vec<_> = certificates.iter().map(|c| c.stack_roots.len()).collect();
    assert_eq!(
        stack_counts,
        vec![6, 5, 4],
        "two preserved fixed arguments stay rooted while pending rest roots shrink"
    );

    // Derive the fixed-argument locations from their actual use *after* the
    // final packing allocation. This avoids a hand-written stack-offset
    // whitelist and proves the certificate follows compiler layout.
    let after_last_cons = assembly
        .rsplit_once("call wsm_cons")
        .expect("fixture must allocate the rest list")
        .1;
    let before_lambda_call = after_last_cons
        .split("call .Llambda_")
        .next()
        .expect("direct lambda call must follow rest packing");
    let final_arg_offsets: Vec<usize> = before_lambda_call
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            let rest = line.strip_prefix("movq ")?;
            let (source, target) = rest.split_once(", ")?;
            if !matches!(target, "%rsi" | "%rdx" | "%rcx" | "%r8" | "%r9") {
                return None;
            }
            let offset = source.strip_suffix("(%rsp)")?;
            offset.parse().ok()
        })
        .collect();
    assert_eq!(
        final_arg_offsets.len(),
        3,
        "fixed a/b plus packed rest must feed the final lambda call"
    );
    let fixed_offsets = &final_arg_offsets[..2];
    for fixed in fixed_offsets {
        assert!(
            certificates
                .iter()
                .all(|cert| cert.stack_roots.contains(fixed)),
            "fixed argument location {fixed} is live after every packing allocation"
        );
    }
    assert!(
        !certificates
            .last()
            .expect("three safepoints")
            .stack_roots
            .contains(&final_arg_offsets[2]),
        "final packed-rest result does not exist until after the last allocation"
    );

    let frames: BTreeSet<_> = certificates.iter().map(|c| c.frame).collect();
    assert_eq!(
        frames.len(),
        1,
        "all certificates belong to one native frame"
    );
    let frame = certificates[0].frame;
    assert_eq!(frame % 16, 0, "freestanding spill frame remains aligned");

    for (expected_id, cert) in certificates.iter().enumerate() {
        assert_eq!(cert.id, expected_id);
        assert_eq!(cert.kind, "pack-rest-bounded");
        assert_eq!(
            cert.register_roots,
            BTreeSet::from(["%rdx".to_string(), "%rsi".to_string()]),
            "current car/cdr registers are rewriteable live locations"
        );
        assert!(
            !cert.register_roots.contains("%r12"),
            "RuntimeContext is mechanism metadata, not a WSM root"
        );
        for offset in &cert.stack_roots {
            assert_eq!(offset % 8, 0, "root location must name a Word slot");
            assert!(
                *offset < frame,
                "return address/frame padding/out-of-frame data must not be roots"
            );
        }
    }

    for pair in certificates.windows(2) {
        let before = &pair[0].stack_roots;
        let after = &pair[1].stack_roots;
        let removed: Vec<_> = before.difference(after).copied().collect();
        let added: Vec<_> = after.difference(before).copied().collect();

        // Preserved fixed roots remain. One processed car and the previous
        // cdr slot die; one newly allocated cdr/result slot becomes live.
        // Net root count shrinks by one.
        assert_eq!(removed.len(), 2, "dead locations must disappear precisely");
        assert_eq!(added.len(), 1, "new packed-list cdr location must appear");
    }

    assert!(
        !assembly.contains("# GC_REGISTER_ROOT id=0 reg=%r12"),
        "context pointer is never certified as a language root"
    );
}

#[test]
fn non_allocating_fixed_lambda_emits_no_gc_certificate() {
    let expressions = parser::parse("((lambda (x) x) 7)").unwrap();
    let program = lower::lower_program(&expressions).unwrap();
    let assembly = X86FreestandingBackend::new()
        .compile_program(&program)
        .expect("fixed non-allocating lambda must compile");

    assert!(
        parse_certificates(&assembly).is_empty(),
        "research root metadata is emitted only at admitted allocating safepoints"
    );
}

#[test]
fn nested_variadic_call_with_preexisting_spill_emits_no_partial_certificate() {
    let expressions =
        parser::parse("(cons (quote KEEP) ((lambda (a . rest) rest) 1 2 3))").unwrap();
    let program = lower::lower_program(&expressions).unwrap();
    let assembly = X86FreestandingBackend::new()
        .compile_program(&program)
        .expect("nested variadic call must compile");

    assert!(
        assembly.matches("call wsm_cons").count() >= 3,
        "fixture must contain nested rest-packing allocations plus outer cons"
    );
    let certificates = parse_certificates(&assembly);
    assert!(
        certificates
            .iter()
            .all(|cert| cert.kind != "pack-rest-bounded"),
        "#415 bounded pack-rest metadata must stay suppressed when an older outer spill exists"
    );

    let structured: Vec<_> = certificates
        .iter()
        .filter(|cert| cert.kind == "runtime-call-structured")
        .collect();
    assert_eq!(
        structured.len(),
        1,
        "#417 may certify the outer top-level cons after the complex nested variadic call returns"
    );
    assert_eq!(
        structured[0].stack_roots.len(),
        2,
        "outer cons has exactly KEEP plus the completed nested result"
    );
}

#[test]
fn nested_platform_call_carries_earlier_outer_spill_as_live_root() {
    let expressions = parser::parse("(cons (quote KEEP) (cons (quote B) (quote C)))").unwrap();
    let program = lower::lower_program(&expressions).unwrap();
    let assembly = X86FreestandingBackend::new()
        .compile_program(&program)
        .expect("nested cons fixture must compile");

    let certificates: Vec<_> = parse_certificates(&assembly)
        .into_iter()
        .filter(|cert| cert.kind == "runtime-call-structured")
        .collect();

    assert_eq!(
        certificates.len(),
        2,
        "inner and outer wsm_cons calls should be certified in the bounded top-level frame"
    );
    assert_eq!(
        certificates[0].stack_roots.len(),
        3,
        "inner allocation needs outer KEEP plus its two current operands"
    );
    assert_eq!(
        certificates[1].stack_roots.len(),
        2,
        "outer allocation needs KEEP plus the completed inner result"
    );

    let shared: BTreeSet<_> = certificates[0]
        .stack_roots
        .intersection(&certificates[1].stack_roots)
        .copied()
        .collect();
    assert_eq!(
        shared.len(),
        1,
        "exactly the earlier KEEP spill must survive both allocation points"
    );

    for cert in &certificates {
        assert_eq!(
            cert.register_roots,
            BTreeSet::from(["%rdx".to_string(), "%rsi".to_string()])
        );
        assert!(!cert.register_roots.contains("%r12"));
    }
}

#[test]
fn nested_platform_call_without_earlier_spill_has_no_phantom_root() {
    let expressions = parser::parse("(cons (cons (quote B) (quote C)) (quote KEEP))").unwrap();
    let program = lower::lower_program(&expressions).unwrap();
    let assembly = X86FreestandingBackend::new()
        .compile_program(&program)
        .expect("dead-twin nested cons fixture must compile");

    let certificates: Vec<_> = parse_certificates(&assembly)
        .into_iter()
        .filter(|cert| cert.kind == "runtime-call-structured")
        .collect();

    assert_eq!(certificates.len(), 2);
    assert_eq!(
        certificates[0].stack_roots.len(),
        2,
        "inner allocation has only its own two operands when no earlier outer spill exists"
    );
    assert_eq!(certificates[1].stack_roots.len(), 2);
    assert!(
        certificates[0]
            .stack_roots
            .is_disjoint(&certificates[1].stack_roots),
        "the completed inner result is a new slot; no dead/phantom earlier root may persist"
    );
}

#[test]
fn complex_nested_form_fails_closed_instead_of_emitting_partial_structured_map() {
    let expressions =
        parser::parse("(cons (quote KEEP) ((lambda () (cons (quote B) (quote C)))))").unwrap();
    let program = lower::lower_program(&expressions).unwrap();
    let assembly = X86FreestandingBackend::new()
        .compile_program(&program)
        .expect("complex nested allocation fixture must compile");

    assert!(
        assembly.matches("call wsm_cons").count() >= 2,
        "fixture must allocate inside the lambda and again in the outer cons"
    );

    let certificates: Vec<_> = parse_certificates(&assembly)
        .into_iter()
        .filter(|cert| cert.kind == "runtime-call-structured")
        .collect();

    assert_eq!(
        certificates.len(),
        1,
        "the bounded protocol must suppress the inner lambda-frame certificate and only certify the outer top-level call"
    );
    assert_eq!(certificates[0].stack_roots.len(), 2);
}

#[test]
fn captured_closure_emits_shrinking_capture_roots_and_exact_environment_root() {
    let expressions = parser::parse(
        "(((lambda (a b) (lambda (x) (cons a (cons b (cons x (quote ())))))) \
          (quote A) (quote B)) \
         (quote C))",
    )
    .unwrap();
    let program = lower::lower_program(&expressions).unwrap();
    let assembly = X86FreestandingBackend::new()
        .compile_program(&program)
        .expect("two-capture escaping closure must compile");

    let closure: Vec<_> = parse_certificates(&assembly)
        .into_iter()
        .filter(|cert| {
            matches!(
                cert.kind.as_str(),
                "closure-capture-cons" | "closure-new-bounded"
            )
        })
        .collect();

    assert_eq!(
        closure.len(),
        3,
        "two captured values require two cons safepoints plus closure allocation"
    );
    assert_eq!(
        closure.iter().map(|c| c.kind.as_str()).collect::<Vec<_>>(),
        vec![
            "closure-capture-cons",
            "closure-capture-cons",
            "closure-new-bounded",
        ]
    );

    assert_eq!(
        closure
            .iter()
            .map(|c| c.stack_roots.len())
            .collect::<Vec<_>>(),
        vec![3, 2, 0],
        "future capture sources shrink while each old tail is replaced"
    );
    assert_eq!(
        closure[0].register_roots,
        BTreeSet::from(["%rdx".to_string(), "%rsi".to_string()])
    );
    assert_eq!(
        closure[1].register_roots,
        BTreeSet::from(["%rdx".to_string(), "%rsi".to_string()])
    );
    assert_eq!(
        closure[2].register_roots,
        BTreeSet::from(["%rdx".to_string()]),
        "completed environment list is the only closure-allocation register root"
    );
    assert!(
        closure
            .iter()
            .all(|cert| !cert.register_roots.contains("%r12")),
        "RuntimeContext must never become a WSM root"
    );

    let first_removed: Vec<_> = closure[0]
        .stack_roots
        .difference(&closure[1].stack_roots)
        .copied()
        .collect();
    let first_added: Vec<_> = closure[1]
        .stack_roots
        .difference(&closure[0].stack_roots)
        .copied()
        .collect();
    assert_eq!(
        first_removed.len(),
        2,
        "one consumed capture source and the previous tail die after the first cons"
    );
    assert_eq!(
        first_added.len(),
        1,
        "the newly materialized environment tail becomes the next live tail"
    );

    let frames: BTreeSet<_> = closure.iter().map(|cert| cert.frame).collect();
    assert_eq!(
        frames.len(),
        1,
        "one closure construction lives in one native frame"
    );
    let certified_frame = closure[0].frame;
    assert_eq!(certified_frame % 8, 0);
    for cert in &closure[..2] {
        assert!(
            cert.stack_roots
                .iter()
                .all(|offset| *offset < certified_frame),
            "every capture root must lie inside the certified current frame"
        );
    }

    let cert_marker = assembly
        .find("kind=closure-capture-cons")
        .expect("fixture must emit bounded closure root metadata");
    let nearest_frame_line = assembly[..cert_marker]
        .lines()
        .rev()
        .find(|line| line.trim().starts_with("subq $"))
        .expect("closure certificate must be inside an explicit native frame")
        .trim();
    let emitted_frame: usize = nearest_frame_line
        .strip_prefix("subq $")
        .and_then(|rest| rest.strip_suffix(", %rsp"))
        .expect("native frame line shape")
        .parse()
        .expect("numeric native frame size");
    assert_eq!(
        certified_frame, emitted_frame,
        "research root metadata must name the actual currently emitted native frame"
    );
}

#[test]
fn captured_closure_fails_closed_when_an_outer_structured_spill_is_live() {
    let expressions = parser::parse(
        "(cons (quote KEEP) \
           ((lambda (a b) (lambda (x) (cons a (cons b x)))) \
            (quote A) (quote B)))",
    )
    .unwrap();
    let program = lower::lower_program(&expressions).unwrap();
    let assembly = X86FreestandingBackend::new()
        .compile_program(&program)
        .expect("nested closure fixture must compile");

    let certificates = parse_certificates(&assembly);
    assert!(
        certificates.iter().all(|cert| {
            cert.kind != "closure-capture-cons" && cert.kind != "closure-new-bounded"
        }),
        "closure metadata must fail closed while an older caller-frame structured spill is live"
    );
    assert!(
        certificates
            .iter()
            .any(|cert| cert.kind == "runtime-call-structured"),
        "the outer cons may still be certified after the nested closure expression completes"
    );
}

#[test]
fn sid8_list_emits_exact_shrinking_pending_argument_roots() {
    let expressions =
        parser::parse("(list (quote A) (quote B) (quote C) (quote D))").unwrap();
    let program = lower::lower_program(&expressions).unwrap();
    let assembly = X86FreestandingBackend::new()
        .compile_program(&program)
        .expect("top-level LIST fixture must compile");

    let list: Vec<_> = parse_certificates(&assembly)
        .into_iter()
        .filter(|cert| cert.kind == "list-bounded")
        .collect();

    assert_eq!(list.len(), 4, "four LIST elements require four cons safepoints");
    assert_eq!(
        list.iter()
            .map(|cert| cert.stack_roots.len())
            .collect::<Vec<_>>(),
        vec![4, 3, 2, 1],
        "pending source locations shrink exactly once per consumed argument"
    );

    for cert in &list {
        assert_eq!(
            cert.register_roots,
            BTreeSet::from(["%rdx".to_string(), "%rsi".to_string()]),
            "current car and tail must be rewriteable for a moving collector"
        );
        assert!(!cert.register_roots.contains("%r12"));
        assert!(
            cert.stack_roots.iter().all(|offset| *offset < cert.frame),
            "every pending LIST source must lie inside the current native frame"
        );
    }

    for pair in list.windows(2) {
        assert_eq!(
            pair[0]
                .stack_roots
                .difference(&pair[1].stack_roots)
                .count(),
            1,
            "exactly one consumed source location dies after each cons"
        );
        assert!(
            pair[1].stack_roots.is_subset(&pair[0].stack_roots),
            "LIST does not invent replacement stack roots; the new tail lives in %rdx"
        );
    }
}

#[test]
fn nested_sid8_list_preserves_the_older_outer_structured_spill() {
    let expressions =
        parser::parse("(cons (quote KEEP) (list (quote A) (quote B) (quote C)))").unwrap();
    let program = lower::lower_program(&expressions).unwrap();
    let assembly = X86FreestandingBackend::new()
        .compile_program(&program)
        .expect("nested LIST inside outer cons must compile");

    let certificates = parse_certificates(&assembly);
    let list: Vec<_> = certificates
        .iter()
        .filter(|cert| cert.kind == "list-bounded")
        .collect();
    assert_eq!(list.len(), 3);
    assert_eq!(
        list.iter()
            .map(|cert| cert.stack_roots.len())
            .collect::<Vec<_>>(),
        vec![4, 3, 2],
        "outer KEEP stays live while three pending LIST sources shrink"
    );

    let outer = certificates
        .iter()
        .find(|cert| cert.kind == "runtime-call-structured")
        .expect("outer cons should retain its bounded structured certificate");
    assert_eq!(
        outer.stack_roots.len(),
        2,
        "outer cons needs KEEP + LIST result"
    );

    let shared: Vec<_> = list
        .last()
        .expect("last LIST safepoint")
        .stack_roots
        .intersection(&outer.stack_roots)
        .copied()
        .collect();
    assert_eq!(
        shared.len(),
        1,
        "one rewriteable older outer spill must survive every nested LIST allocation"
    );
}

#[test]
fn sid8_list_fails_closed_in_unproved_lexical_context() {
    let expressions =
        parser::parse("((lambda (x) (list x (quote A))) (quote X))").unwrap();
    let program = lower::lower_program(&expressions).unwrap();
    let assembly = X86FreestandingBackend::new()
        .compile_program(&program)
        .expect("lexical LIST fixture must compile");

    assert!(
        parse_certificates(&assembly)
            .iter()
            .all(|cert| cert.kind != "list-bounded"),
        "LIST must not publish a partial current-frame map when lexical liveness is unproved"
    );
}

