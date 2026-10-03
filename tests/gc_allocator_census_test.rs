const SOURCE: &str = include_str!("../src/x86_freestanding.rs");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Policy {
    ConditionalCertified,
    UnknownNeedsRootProof,
}

#[derive(Clone, Copy, Debug)]
struct AllocatorOwner {
    name: &'static str,
    direct_cons: usize,
    direct_closure_new: usize,
    policy: Policy,
    note: &'static str,
}

const OWNERS: &[AllocatorOwner] = &[
    AllocatorOwner {
        name: "compile_program",
        direct_cons: 0,
        direct_closure_new: 1,
        policy: Policy::UnknownNeedsRootProof,
        note: "startup named-closure allocation; globals/root owner not yet proved",
    },
    AllocatorOwner {
        name: "emit_runtime_call_with_structured_args",
        direct_cons: 0,
        direct_closure_new: 0,
        policy: Policy::ConditionalCertified,
        note: "runtime-call-structured when bounded completeness is proved; otherwise not a safepoint",
    },
    AllocatorOwner {
        name: "emit_pack_rest_list",
        direct_cons: 1,
        direct_closure_new: 0,
        policy: Policy::ConditionalCertified,
        note: "pack-rest-bounded when caller proves complete preserved roots; otherwise not a safepoint",
    },
    AllocatorOwner {
        name: "emit_fixed_arity_closure_value",
        direct_cons: 1,
        direct_closure_new: 1,
        policy: Policy::UnknownNeedsRootProof,
        note: "capture-list construction plus closure descriptor allocation",
    },
    AllocatorOwner {
        name: "emit_quoted",
        direct_cons: 2,
        direct_closure_new: 0,
        policy: Policy::UnknownNeedsRootProof,
        note: "proper/dotted quoted-list construction",
    },
    AllocatorOwner {
        name: "emit_primitive_list",
        direct_cons: 1,
        direct_closure_new: 0,
        policy: Policy::UnknownNeedsRootProof,
        note: "SID8 LIST right-to-left construction",
    },
];

fn count(haystack: &str, needle: &str) -> usize {
    haystack.match_indices(needle).count()
}

fn leading_spaces(line: &str) -> usize {
    line.as_bytes().iter().take_while(|&&b| b == b' ').count()
}

fn is_fn_header(trimmed: &str, name: &str) -> bool {
    let tails = [
        format!("fn {name}("),
        format!("pub fn {name}("),
        format!("pub(crate) fn {name}("),
    ];
    tails.iter().any(|prefix| trimmed.starts_with(prefix))
}

fn any_fn_header(trimmed: &str) -> bool {
    trimmed.starts_with("fn ")
        || trimmed.starts_with("pub fn ")
        || trimmed.starts_with("pub(crate) fn ")
}

fn owner_source<'a>(source: &'a str, name: &str) -> &'a str {
    let mut offset = 0usize;
    let mut start = None;
    let mut indent = 0usize;

    for line in source.split_inclusive('\n') {
        let trimmed = line.trim_start();
        if start.is_none() && is_fn_header(trimmed, name) {
            start = Some(offset);
            indent = leading_spaces(line);
        } else if let Some(begin) = start {
            if leading_spaces(line) == indent && any_fn_header(trimmed) {
                return &source[begin..offset];
            }
        }
        offset += line.len();
    }

    let begin = start.unwrap_or_else(|| panic!("allocator owner function {name} not found"));
    &source[begin..]
}

#[test]
fn every_direct_allocator_emission_belongs_to_the_declared_owner_census() {
    let expected_cons: usize = OWNERS.iter().map(|owner| owner.direct_cons).sum();
    let expected_closure_new: usize = OWNERS
        .iter()
        .map(|owner| owner.direct_closure_new)
        .sum();

    assert_eq!(
        count(SOURCE, "call wsm_cons"),
        expected_cons,
        "new or removed direct wsm_cons emission requires an explicit #419 census update"
    );
    assert_eq!(
        count(SOURCE, "call wsm_closure_new"),
        expected_closure_new,
        "new or removed direct wsm_closure_new emission requires an explicit #419 census update"
    );

    for owner in OWNERS {
        let body = owner_source(SOURCE, owner.name);
        assert_eq!(
            count(body, "call wsm_cons"),
            owner.direct_cons,
            "{} changed its direct wsm_cons ownership; update #419 deliberately",
            owner.name
        );
        assert_eq!(
            count(body, "call wsm_closure_new"),
            owner.direct_closure_new,
            "{} changed its direct wsm_closure_new ownership; update #419 deliberately",
            owner.name
        );
    }
}

#[test]
fn certified_owner_families_keep_their_fail_closed_certificate_guards() {
    let structured = owner_source(SOURCE, "emit_runtime_call_with_structured_args");
    assert!(structured.contains(r#"runtime == "wsm_cons""#));
    assert!(structured.contains("gc_structured_context_complete"));
    assert!(structured.contains(r#""runtime-call-structured""#));
    assert_eq!(
        count(structured, "call {runtime}"),
        1,
        "shared runtime helper should own one dynamic call emission"
    );

    let pack_rest = owner_source(SOURCE, "emit_pack_rest_list");
    assert!(pack_rest.contains("if let Some(preserved_slots)"));
    assert!(pack_rest.contains(r#""pack-rest-bounded""#));
    assert_eq!(count(pack_rest, "call wsm_cons"), 1);
}

#[test]
fn unknown_allocator_owners_are_not_silently_treated_as_certified() {
    for owner in OWNERS
        .iter()
        .filter(|owner| owner.policy == Policy::UnknownNeedsRootProof)
    {
        let body = owner_source(SOURCE, owner.name);
        assert!(
            !body.contains(r#""runtime-call-structured""#)
                && !body.contains(r#""pack-rest-bounded""#),
            "{} is UNKNOWN in #419 and must not inherit another owner's certificate kind",
            owner.name
        );
    }
}

#[test]
fn census_is_small_explicit_and_reviewable() {
    assert_eq!(OWNERS.len(), 6, "allocator frontier changed; update #419");
    assert_eq!(
        OWNERS
            .iter()
            .filter(|owner| owner.policy == Policy::ConditionalCertified)
            .count(),
        2
    );
    assert_eq!(
        OWNERS
            .iter()
            .filter(|owner| owner.policy == Policy::UnknownNeedsRootProof)
            .count(),
        4
    );

    for owner in OWNERS {
        assert!(!owner.note.is_empty());
        eprintln!(
            "{}: {:?}; direct_cons={}; direct_closure_new={}; {}",
            owner.name,
            owner.policy,
            owner.direct_cons,
            owner.direct_closure_new,
            owner.note
        );
    }
}
