use std::collections::BTreeSet;

use cml::lower;
use cml::parser;
use cml::x86_freestanding::X86FreestandingBackend;

#[derive(Debug)]
struct Certificate {
    id: usize,
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
            let kind = field(parts.iter().copied(), "kind");
            let frame = field(parts.iter().copied(), "frame").parse().unwrap();
            assert_eq!(kind, "pack-rest-bounded");
            certificates.push(Certificate {
                id,
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
