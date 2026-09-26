//! cml#14: generate the Canon surface tables, operations table, and
//! machine-readable metadata from the real, vendored my-lisp
//! `lib/surface/semantic-registry.wsm` instead of hand-transcribed duplicates.
//!
//! Build-time generator validates:
//! - All admitted Canon operations exist in semantic-registry.wsm.
//! - Non-empty surface coverage for every admitted operation.
//! - No surface collisions between distinct operations (fail-closed).
//! - Generates `canon_spellings.rs` for `src/canon.rs`.
//! - Generates `contracts/cml-operations.my` machine-readable table.

use std::collections::{BTreeSet, HashMap};
use std::env;
use std::fs;
use std::path::PathBuf;

/// IDs whose surfaces feed `is_reserved_canon_surface` (Canon 0+7).
const TARGET_IDS: &[&str] = &[
    "00000001", "00000010", "00000011", "00000100", "00000101", "00000110", "00000111",
];

/// Retired semantic IDs that must NEVER be active or recycled (e.g. 1153 for former RDTSC attribution).
const RETIRED_SEMANTIC_IDS: &[&str] = &["1153"];

/// Special-form dispatch sets: form name prefix -> registry IDs.
const DISPATCH_FORMS: &[(&str, &[&str])] = &[
    ("QUOTE", &["00000001"]),
    ("COND", &["00000111"]),
    ("LAMBDA", &["00001000"]),
    ("DEFINE", &["00001001", "00001011"]),
    ("DEFMACRO", &["00001010"]),
];

/// Canonical target builtin names for first-class values.
const BUILTIN_PROJECTIONS: &[(&str, &str)] = &[
    ("00000010", "ATOM"),
    ("00000011", "EQ"),
    ("00000100", "CONS"),
    ("00000101", "CAR"),
    ("00000110", "CDR"),
    ("00001100", "+"),
    ("00001101", "-"),
    ("00001110", "*"),
    ("00001111", "/"),
    ("00010011", "mod"),
    ("00010100", "QUOTIENT"),
    ("00011010", "<"),
    ("00011011", ">"),
    ("00011100", "="),
    ("00011101", "<="),
    ("00011110", ">="),
    ("00100010", "EQUAL?"),
    ("01011001", "NUMERIC-BUFFER-MAP"),
];

struct OperationSpec {
    canonical_name: &'static str,
    formal_action: &'static str,
    cml_ir_projection: &'static str,
    backend_projections: &'static [(&'static str, &'static str)],
    status: &'static str,
    authority_owner: &'static str,
    provenance_witness: &'static str,
}

const OPERATIONS: &[OperationSpec] = &[
    OperationSpec {
        canonical_name: "quote",
        formal_action: "syntax:quote",
        cml_ir_projection: "Ir::Quote",
        backend_projections: &[
            ("fpga-lisp", "LOADSYM/QUOTE"),
            ("c", "TAG_QUOTE/mk_pair"),
            ("x86_freestanding", "literal_encoding"),
        ],
        status: "supported",
        authority_owner: "my-lisp:language-core cml:compiler-middle-end",
        provenance_witness: "lib/surface/semantic-registry.wsm tests/canon_dispatch_test.rs",
    },
    OperationSpec {
        canonical_name: "atom",
        formal_action: "primitive:atom",
        cml_ir_projection: "Ir::App(Sid(00000010))",
        backend_projections: &[
            ("fpga-lisp", "OP_ATOM"),
            ("c", "prim_atom"),
            ("x86_freestanding", "wsm_atom"),
        ],
        status: "supported",
        authority_owner: "my-lisp:language-core cml:compiler-middle-end",
        provenance_witness: "lib/surface/semantic-registry.wsm tests/registry_driven_canon_callables_test.rs",
    },
    OperationSpec {
        canonical_name: "eq",
        formal_action: "primitive:eq",
        cml_ir_projection: "Ir::App(Sid(00000011))",
        backend_projections: &[
            ("fpga-lisp", "OP_EQ"),
            ("c", "prim_eq"),
            ("x86_freestanding", "wsm_eq"),
        ],
        status: "supported",
        authority_owner: "my-lisp:language-core cml:compiler-middle-end",
        provenance_witness: "lib/surface/semantic-registry.wsm tests/registry_driven_canon_callables_test.rs",
    },
    OperationSpec {
        canonical_name: "cons",
        formal_action: "primitive:cons",
        cml_ir_projection: "Ir::App(Sid(00000100))",
        backend_projections: &[
            ("fpga-lisp", "OP_CONS"),
            ("c", "mk_cons"),
            ("x86_freestanding", "wsm_cons"),
        ],
        status: "supported",
        authority_owner: "my-lisp:language-core cml:compiler-middle-end",
        provenance_witness: "lib/surface/semantic-registry.wsm tests/registry_driven_canon_callables_test.rs",
    },
    OperationSpec {
        canonical_name: "car",
        formal_action: "primitive:car",
        cml_ir_projection: "Ir::App(Sid(00000101))",
        backend_projections: &[
            ("fpga-lisp", "OP_CAR"),
            ("c", "v_car"),
            ("x86_freestanding", "wsm_car"),
        ],
        status: "supported",
        authority_owner: "my-lisp:language-core cml:compiler-middle-end",
        provenance_witness: "lib/surface/semantic-registry.wsm tests/registry_driven_canon_callables_test.rs",
    },
    OperationSpec {
        canonical_name: "cdr",
        formal_action: "primitive:cdr",
        cml_ir_projection: "Ir::App(Sid(00000110))",
        backend_projections: &[
            ("fpga-lisp", "OP_CDR"),
            ("c", "v_cdr"),
            ("x86_freestanding", "wsm_cdr"),
        ],
        status: "supported",
        authority_owner: "my-lisp:language-core cml:compiler-middle-end",
        provenance_witness: "lib/surface/semantic-registry.wsm tests/registry_driven_canon_callables_test.rs",
    },
    OperationSpec {
        canonical_name: "cond",
        formal_action: "syntax:cond",
        cml_ir_projection: "Ir::Cond",
        backend_projections: &[
            ("fpga-lisp", "CJMP/JMP"),
            ("c", "if-else-chain"),
            ("x86_freestanding", "testq/jz"),
        ],
        status: "supported",
        authority_owner: "my-lisp:language-core cml:compiler-middle-end",
        provenance_witness: "lib/surface/semantic-registry.wsm tests/canon_dispatch_test.rs",
    },
    OperationSpec {
        canonical_name: "lambda",
        formal_action: "syntax:lambda",
        cml_ir_projection: "Ir::Lambda",
        backend_projections: &[
            ("fpga-lisp", "closure/alist-env"),
            ("c", "env_tuple_closure"),
            ("x86_freestanding", "closure_alloc/direct_call"),
        ],
        status: "supported",
        authority_owner: "my-lisp:language-core cml:compiler-middle-end",
        provenance_witness: "lib/surface/semantic-registry.wsm tests/canon_dispatch_test.rs",
    },
    OperationSpec {
        canonical_name: "define",
        formal_action: "syntax:define",
        cml_ir_projection: "Ir::Def",
        backend_projections: &[
            ("fpga-lisp", "top-level-binding"),
            ("c", "top_level_def"),
            ("x86_freestanding", "top_level_symbol"),
        ],
        status: "supported",
        authority_owner: "my-lisp:language-core cml:compiler-middle-end",
        provenance_witness: "lib/surface/semantic-registry.wsm tests/canon_dispatch_test.rs",
    },
    OperationSpec {
        canonical_name: "def",
        formal_action: "compatibility:def",
        cml_ir_projection: "Ir::Def",
        backend_projections: &[
            ("fpga-lisp", "top-level-binding"),
            ("c", "top_level_def"),
            ("x86_freestanding", "top_level_symbol"),
        ],
        status: "compatibility",
        authority_owner: "my-lisp:language-core cml:compiler-middle-end",
        provenance_witness: "lib/surface/semantic-registry.wsm tests/canon_dispatch_test.rs",
    },
    OperationSpec {
        canonical_name: "defmacro",
        formal_action: "syntax:defmacro",
        cml_ir_projection: "macro_expansion",
        backend_projections: &[
            ("fpga-lisp", "frontend_macro_expand"),
            ("c", "frontend_macro_expand"),
            ("x86_freestanding", "frontend_macro_expand"),
        ],
        status: "supported",
        authority_owner: "my-lisp:language-core cml:compiler-middle-end",
        provenance_witness: "lib/surface/semantic-registry.wsm tests/macro_pipeline_test.rs",
    },
    OperationSpec {
        canonical_name: "+",
        formal_action: "primitive:add",
        cml_ir_projection: "Ir::App(Sid(00001100))",
        backend_projections: &[
            ("fpga-lisp", "OP_ADD"),
            ("c", "prim_add"),
            ("x86_freestanding", "addq"),
        ],
        status: "supported",
        authority_owner: "my-lisp:language-core cml:compiler-middle-end",
        provenance_witness: "lib/surface/semantic-registry.wsm tests/registry_driven_canon_callables_test.rs",
    },
    OperationSpec {
        canonical_name: "-",
        formal_action: "primitive:sub",
        cml_ir_projection: "Ir::App(Sid(00001101))",
        backend_projections: &[
            ("fpga-lisp", "OP_SUB"),
            ("c", "prim_sub"),
            ("x86_freestanding", "subq"),
        ],
        status: "supported",
        authority_owner: "my-lisp:language-core cml:compiler-middle-end",
        provenance_witness: "lib/surface/semantic-registry.wsm tests/registry_driven_canon_callables_test.rs",
    },
    OperationSpec {
        canonical_name: "*",
        formal_action: "primitive:exact-q-mul",
        cml_ir_projection: "Ir::App(Sid(00001110))",
        backend_projections: &[
            ("fpga-lisp", "unsupported"),
            ("c", "unsupported"),
            ("x86_freestanding", "unsupported"),
        ],
        status: "partial",
        authority_owner: "my-lisp:language-core cml:compiler-middle-end",
        provenance_witness: "lib/surface/semantic-registry.wsm tests/exact_q_mul_admission_test.rs",
    },
    OperationSpec {
        canonical_name: "mod",
        formal_action: "primitive:exact-q-mod",
        cml_ir_projection: "Ir::App(Sid(00010011))",
        backend_projections: &[
            ("fpga-lisp", "unsupported"),
            ("c", "unsupported"),
            ("x86_freestanding", "unsupported"),
        ],
        status: "partial",
        authority_owner: "my-lisp:language-core cml:compiler-middle-end",
        provenance_witness: "lib/surface/semantic-registry.wsm tests/exact_q_mod_admission_test.rs",
    },
    OperationSpec {
        canonical_name: "/",
        formal_action: "primitive:exact-q-div",
        cml_ir_projection: "Ir::App(Sid(00001111))",
        backend_projections: &[
            ("fpga-lisp", "unsupported"),
            ("c", "unsupported"),
            ("x86_freestanding", "unsupported"),
        ],
        status: "partial",
        authority_owner: "my-lisp:exact-q-binary cml:compiler-middle-end",
        provenance_witness: "my-lisp/contracts/exact-q-binary-contract.lisp tests/exact_q_div_admission_test.rs",
    },
    OperationSpec {
        canonical_name: "quotient",
        formal_action: "primitive:exact-q-quotient",
        cml_ir_projection: "Ir::App(Sid(00010100))",
        backend_projections: &[
            ("fpga-lisp", "unsupported"),
            ("c", "unsupported"),
            ("x86_freestanding", "unsupported"),
        ],
        status: "partial",
        authority_owner: "my-lisp:exact-q-binary cml:compiler-middle-end",
        provenance_witness: "my-lisp/contracts/exact-q-binary-contract.lisp tests/exact_q_quotient_admission_test.rs",
    },
    OperationSpec {
        canonical_name: "<",
        formal_action: "primitive:exact-q-less-than",
        cml_ir_projection: "Ir::App(Sid(00011010))",
        backend_projections: &[
            ("fpga-lisp", "unsupported"),
            ("c", "v_exact_q_lt/builtin_exact_q_lt"),
            ("x86_freestanding", "unsupported"),
        ],
        status: "partial",
        authority_owner: "my-lisp:exact-q-binary cml:compiler-middle-end",
        provenance_witness: "my-lisp/contracts/exact-q-binary-contract.lisp tests/exact_q_compare_lowering_test.rs tests/exact_q_lt_encoder_consumer_test.rs",
    },
    OperationSpec {
        canonical_name: ">",
        formal_action: "primitive:exact-q-greater-than",
        cml_ir_projection: "Ir::App(Sid(00011011))",
        backend_projections: &[
            ("fpga-lisp", "unsupported"),
            ("c", "unsupported"),
            ("x86_freestanding", "unsupported"),
        ],
        status: "partial",
        authority_owner: "my-lisp:exact-q-binary cml:compiler-middle-end",
        provenance_witness: "my-lisp/contracts/exact-q-binary-contract.lisp tests/exact_q_compare_lowering_test.rs",
    },
    OperationSpec {
        canonical_name: "=",
        formal_action: "primitive:exact-q-equal",
        cml_ir_projection: "Ir::App(Sid(00011100))",
        backend_projections: &[
            ("fpga-lisp", "unsupported"),
            ("c", "unsupported"),
            ("x86_freestanding", "unsupported"),
        ],
        status: "partial",
        authority_owner: "my-lisp:exact-q-binary cml:compiler-middle-end",
        provenance_witness: "my-lisp/contracts/exact-q-binary-contract.lisp tests/exact_q_compare_lowering_test.rs",
    },
    OperationSpec {
        canonical_name: "<=",
        formal_action: "primitive:exact-q-less-equal",
        cml_ir_projection: "Ir::App(Sid(00011101))",
        backend_projections: &[
            ("fpga-lisp", "unsupported"),
            ("c", "unsupported"),
            ("x86_freestanding", "bounded-fixnum/numeric-0-or-1"),
        ],
        status: "partial",
        authority_owner: "my-lisp:exact-q-binary cml:compiler-middle-end",
        provenance_witness: "my-lisp/contracts/exact-q-binary-contract.lisp tests/exact_q_compare_lowering_test.rs",
    },
    OperationSpec {
        canonical_name: ">=",
        formal_action: "primitive:exact-q-greater-equal",
        cml_ir_projection: "Ir::App(Sid(00011110))",
        backend_projections: &[
            ("fpga-lisp", "unsupported"),
            ("c", "unsupported"),
            ("x86_freestanding", "bounded-fixnum/numeric-0-or-1"),
        ],
        status: "partial",
        authority_owner: "my-lisp:exact-q-binary cml:compiler-middle-end",
        provenance_witness: "my-lisp/contracts/exact-q-binary-contract.lisp tests/exact_q_compare_lowering_test.rs",
    },
    OperationSpec {
        canonical_name: "equal?",
        formal_action: "primitive:equalp",
        cml_ir_projection: "Ir::App(Sid(00100010))",
        backend_projections: &[
            ("fpga-lisp", "cml_equal"),
            ("c", "prim_equalp"),
            ("x86_freestanding", "unsupported"),
        ],
        status: "supported",
        authority_owner: "my-lisp:language-core cml:compiler-middle-end",
        provenance_witness: "lib/surface/semantic-registry.wsm tests/registry_driven_canon_callables_test.rs",
    },
    OperationSpec {
        canonical_name: "numeric-buffer-map",
        formal_action: "library:numeric-buffer-map",
        cml_ir_projection: "Ir::App(Sid(01011001))",
        backend_projections: &[
            ("fpga-lisp", "unsupported"),
            ("c", "cml_numeric_buffer_map"),
            ("x86_freestanding", "unsupported"),
        ],
        status: "supported",
        authority_owner: "my-lisp:language-core cml:compiler-middle-end",
        provenance_witness: "lib/surface/semantic-registry.wsm tests/c_backend_test.rs",
    },
];

#[derive(Debug, Clone)]
enum Sexp {
    Atom(String),
    List(Vec<Sexp>),
}

fn parse_all(source: &str) -> Vec<Sexp> {
    let mut tokens = tokenize(source);
    let mut forms = Vec::new();
    while !tokens.is_empty() {
        forms.push(parse_one(&mut tokens));
    }
    forms
}

fn tokenize(source: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    for line in source.lines() {
        let line = line.split(';').next().unwrap_or("");
        for ch in line.chars() {
            match ch {
                '(' | ')' => {
                    if !current.is_empty() {
                        tokens.push(std::mem::take(&mut current));
                    }
                    tokens.push(ch.to_string());
                }
                c if c.is_whitespace() => {
                    if !current.is_empty() {
                        tokens.push(std::mem::take(&mut current));
                    }
                }
                c => current.push(c),
            }
        }
        if !current.is_empty() {
            tokens.push(std::mem::take(&mut current));
        }
    }
    tokens.reverse();
    tokens
}

fn parse_one(tokens: &mut Vec<String>) -> Sexp {
    let token = tokens
        .pop()
        .expect("unexpected end of semantic-registry.wsm while reading a form");
    if token == "(" {
        let mut items = Vec::new();
        loop {
            match tokens.last().map(String::as_str) {
                Some(")") => {
                    tokens.pop();
                    break;
                }
                None => panic!("unclosed '(' in semantic-registry.wsm"),
                _ => items.push(parse_one(tokens)),
            }
        }
        Sexp::List(items)
    } else if token == ")" {
        panic!("unexpected ')' in semantic-registry.wsm");
    } else {
        Sexp::Atom(token)
    }
}

fn atom(sexp: &Sexp) -> &str {
    match sexp {
        Sexp::Atom(s) => s,
        Sexp::List(_) => panic!("expected an atom, found a list in semantic-registry.wsm"),
    }
}

fn list(sexp: &Sexp) -> &[Sexp] {
    match sexp {
        Sexp::List(items) => items,
        Sexp::Atom(_) => panic!("expected a list, found an atom in semantic-registry.wsm"),
    }
}

/// Opaque semantic ID: an 8-bit binary token rendered as eight ASCII digits.
fn is_semantic_id(token: &str) -> bool {
    token.len() == 8 && token.chars().all(|c| c == '0' || c == '1')
}

/// #106: collect the complete build-source semantic denominator from the
/// same upstream registry parse already used to generate CML's admitted
/// operation table. IDs remain opaque numeric strings; this does not assign
/// meaning or backend support to rows that CML has not admitted.
fn collect_build_source_semantic_ids(root: &[Sexp]) -> Vec<u8> {
    let mut ids = Vec::with_capacity(root.len());
    let mut seen = BTreeSet::new();

    for row in root {
        let Sexp::List(items) = row else {
            panic!("cml#106: semantic-registry row must be a list");
        };
        let Some(Sexp::Atom(id)) = items.first() else {
            panic!("cml#106: semantic-registry row must begin with an opaque semantic ID");
        };
        if !is_semantic_id(id) {
            panic!("cml#106: semantic ID must be an 8-bit binary token: {id:?}");
        }
        let numeric = u8::from_str_radix(id, 2)
            .unwrap_or_else(|_| panic!("cml#106: semantic ID must fit in u8: {id:?}"));
        if !seen.insert(numeric) {
            panic!("cml#106: duplicate semantic ID {id}");
        }
        ids.push(numeric);
    }

    ids
}

/// Deterministic drift fingerprint for the exact build-source registry bytes.
/// This is evidence metadata only, not a semantic identity or security hash.
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn collect_surfaces_detailed(root: &[Sexp], id: &str) -> Vec<(String, String)> {
    if RETIRED_SEMANTIC_IDS.contains(&id) {
        panic!(
            "cml: semantic ID {id} is retired/forbidden by my-lisp authority and cannot be collected as an active surface"
        );
    }
    let entry = match root.iter().find(
        |form| matches!(form, Sexp::List(items) if !items.is_empty() && atom(&items[0]) == id),
    ) {
        Some(e) => e,
        None => {
            panic!("cml#14: semantic-registry has no entry for Canon id {id}");
        }
    };
    let fields = &list(entry)[1..];
    let mut surfaces = Vec::new();
    for field in fields {
        let parts = list(field);
        if parts.len() < 2 {
            continue;
        }
        let lang = atom(&parts[0]);
        // Current registry marks absent surfaces with ().
        if matches!(&parts[1], Sexp::List(items) if items.is_empty()) {
            continue;
        }
        let word = atom(&parts[1]);
        let status = if parts.len() >= 3 {
            atom(&parts[2])
        } else {
            "stable"
        };
        if word == "—" || lang == "compat" || (status != "stable" && status != "compatibility-only")
        {
            continue;
        }
        surfaces.push((lang.to_string(), word.to_string()));
    }
    if surfaces.is_empty() {
        panic!("cml#14: Canon id {id} has zero real surfaces in semantic-registry.wsm");
    }
    surfaces
}

/// Resolve each admitted operation's canonical name to the exact 8-bit SID
/// owned by the upstream registry. The canonical name matches either the `en`
/// surface or, when `en` is absent, the `sym` surface of the row.
fn resolve_operation_semantic_ids(root: &[Sexp]) -> HashMap<String, u8> {
    let mut map = HashMap::new();
    for row in root {
        let Sexp::List(items) = row else { continue };
        let (first, fields) = match items.split_first() {
            Some(pair) => pair,
            None => continue,
        };
        let Sexp::Atom(id_text) = first else { continue };
        if !is_semantic_id(id_text) {
            continue;
        }
        let id = u8::from_str_radix(id_text, 2).unwrap();

        let mut en_name: Option<String> = None;
        let mut sym_name: Option<String> = None;
        for field in fields {
            let Sexp::List(parts) = field else { continue };
            if parts.len() < 2 {
                continue;
            }
            let Sexp::Atom(marker) = &parts[0] else {
                continue;
            };
            let value = &parts[1];
            if matches!(value, Sexp::List(items) if items.is_empty()) {
                continue;
            }
            if marker == "en" {
                if let Sexp::Atom(word) = value {
                    en_name = Some(word.clone());
                }
            } else if marker == "sym" {
                if let Sexp::Atom(word) = value {
                    sym_name = Some(word.clone());
                }
            }
        }
        if let Some(name) = en_name.or(sym_name) {
            map.insert(name, id);
        }
    }
    map
}

fn collect_surfaces(root: &[Sexp], ids: &[&str]) -> (Vec<String>, Vec<String>) {
    let mut upper_surfaces: Vec<String> = Vec::new();
    let mut exact_surfaces: Vec<String> = Vec::new();

    for id in ids {
        let surfaces = collect_surfaces_detailed(root, id);
        for (lang, word) in surfaces {
            match lang.as_str() {
                "en" | "sym" => upper_surfaces.push(word.to_uppercase()),
                "uk" | "ук" | "ukr" | "укр" | "sa" => exact_surfaces.push(word),
                other => panic!("cml#14: unknown surface language {other:?} for id {id}"),
            }
        }
    }

    upper_surfaces.sort();
    upper_surfaces.dedup();
    exact_surfaces.sort();
    exact_surfaces.dedup();
    (upper_surfaces, exact_surfaces)
}

fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR must be set");
    // #84 build-source channel: semantic build inputs come from the
    // external/my-lisp gitlink, never from the observed-current sibling checkout.
    let mut registry_path = PathBuf::from(&manifest_dir)
        .join("external")
        .join("my-lisp")
        .join("lib")
        .join("surface")
        .join("semantic-registry.lisp");
    if !registry_path.exists() {
        registry_path = PathBuf::from(&manifest_dir)
            .join("external")
            .join("my-lisp")
            .join("lib")
            .join("surface")
            .join("semantic-registry.wsm");
    }
    println!("cargo:rerun-if-changed={}", registry_path.display());

    let source = fs::read_to_string(&registry_path).unwrap_or_else(|e| {
        panic!(
            "cml#14: could not read the real semantic-registry at {} ({e}). \
             This build depends on the external/my-lisp submodule being \
             checked out (`git submodule update --init`).",
            registry_path.display()
        )
    });

    let forms = parse_all(&source);
    let root: &[Sexp] = forms
        .iter()
        .find_map(|form| match form {
            // Legacy schema: (sr/1 (id ...) ...)
            Sexp::List(items)
                if !items.is_empty() && matches!(&items[0], Sexp::Atom(a) if a == "sr/1") =>
            {
                Some(&items[1..])
            }
            // Current schema: rows start directly with an opaque 8-bit semantic ID.
            Sexp::List(items)
                if !items.is_empty()
                    && matches!(&items[0], Sexp::List(first)
                        if matches!(&first.first(), Some(Sexp::Atom(id))
                            if is_semantic_id(id))) =>
            {
                Some(items.as_slice())
            }
            _ => None,
        })
        .unwrap_or_else(|| {
            panic!(
                "cml#14: no top-level (sr/1 ...) or headerless row list found in semantic-registry"
            )
        });

    let build_source_semantic_ids = collect_build_source_semantic_ids(root);
    let build_source_semantic_id_set: BTreeSet<u8> =
        build_source_semantic_ids.iter().copied().collect();
    let name_to_sid = resolve_operation_semantic_ids(root);

    let mut operation_ids: Vec<(usize, u8)> = Vec::with_capacity(OPERATIONS.len());
    for (index, operation) in OPERATIONS.iter().enumerate() {
        let id = *name_to_sid
            .get(operation.canonical_name)
            .unwrap_or_else(|| {
                panic!(
                    "cml#106: admitted operation {} has no SID in the build-source registry",
                    operation.canonical_name
                );
            });
        if !build_source_semantic_id_set.contains(&id) {
            panic!(
                "cml#106: admitted operation {} references semantic ID {:08b} absent from the build-source registry",
                operation.canonical_name, id
            );
        }
        operation_ids.push((index, id));
    }
    let build_source_registry_digest = fnv1a64(source.as_bytes());

    // Fail-closed collision detection across all admitted operations
    let mut seen_upper: HashMap<String, String> = HashMap::new();
    let mut seen_exact: HashMap<String, String> = HashMap::new();
    for (index, id) in &operation_ids {
        let op = &OPERATIONS[*index];
        let sid_text = format!("{:08b}", id);
        let surfaces = collect_surfaces_detailed(root, &sid_text);
        for (lang, word) in &surfaces {
            if lang == "en" || lang == "sym" {
                let folded = word.to_uppercase();
                if let Some(prev) = seen_upper.insert(folded.clone(), sid_text.clone()) {
                    if prev != sid_text {
                        panic!(
                            "cml#14: collision on upper surface {folded:?} between {prev} and {}",
                            sid_text
                        );
                    }
                }
            } else if let Some(prev) = seen_exact.insert(word.clone(), sid_text.clone()) {
                if prev != sid_text {
                    panic!(
                        "cml#14: collision on exact surface {word:?} between {prev} and {}",
                        sid_text
                    );
                }
            }
        }
    }

    let (upper_surfaces, exact_surfaces) = collect_surfaces(root, TARGET_IDS);

    let mut generated = String::new();
    generated.push_str("// @generated by build.rs from my-lisp/lib/surface/semantic-registry.wsm (cml#14). Do not edit by hand.\n\n");

    generated.push_str("use my_lisp::Sid8;\n\n");
    generated.push_str("#[derive(Debug, Clone, Copy, PartialEq, Eq)]\n");
    generated.push_str("pub struct CanonOperation {\n");
    generated.push_str("    pub canonical_name: &'static str,\n");
    generated.push_str("    pub semantic_id: Sid8,\n");
    generated.push_str("    pub formal_action: &'static str,\n");
    generated.push_str("    pub surfaces: &'static [(&'static str, &'static str)],\n");
    generated.push_str("    pub cml_ir_projection: &'static str,\n");
    generated.push_str("    pub backend_projections: &'static [(&'static str, &'static str)],\n");
    generated.push_str("    pub status: &'static str,\n");
    generated.push_str("    pub authority_owner: &'static str,\n");
    generated.push_str("    pub provenance_witness: &'static str,\n");
    generated.push_str("}\n\n");

    generated.push_str("pub const CANON_OPERATIONS_TABLE: &[CanonOperation] = &[\n");
    for (index, id) in &operation_ids {
        let op = &OPERATIONS[*index];
        let sid_text = format!("{:08b}", id);
        let surfaces = collect_surfaces_detailed(root, &sid_text);
        generated.push_str("    CanonOperation {\n");
        generated.push_str(&format!(
            "        canonical_name: {:?},\n",
            op.canonical_name
        ));
        generated.push_str(&format!(
            "        semantic_id: my_lisp::sid!({}),\n",
            sid_text
        ));
        generated.push_str(&format!("        formal_action: {:?},\n", op.formal_action));
        generated.push_str("        surfaces: &[\n");
        for (lang, word) in &surfaces {
            generated.push_str(&format!("            ({lang:?}, {word:?}),\n"));
        }
        generated.push_str("        ],\n");
        generated.push_str(&format!(
            "        cml_ir_projection: {:?},\n",
            op.cml_ir_projection
        ));
        generated.push_str("        backend_projections: &[\n");
        for (backend, proj) in op.backend_projections {
            generated.push_str(&format!("            ({backend:?}, {proj:?}),\n"));
        }
        generated.push_str("        ],\n");
        generated.push_str(&format!("        status: {:?},\n", op.status));
        generated.push_str(&format!(
            "        authority_owner: {:?},\n",
            op.authority_owner
        ));
        generated.push_str(&format!(
            "        provenance_witness: {:?},\n",
            op.provenance_witness
        ));
        generated.push_str("    },\n");
    }
    generated.push_str("];\n\n");

    generated.push_str("pub const CANON_BUILD_SOURCE_SEMANTIC_IDS: &[Sid8] = &[\n");
    for id in &build_source_semantic_ids {
        generated.push_str(&format!("    my_lisp::sid!({:08b}),\n", id));
    }
    generated.push_str("];\n\n");
    generated.push_str(&format!(
        "pub const CANON_BUILD_SOURCE_REGISTRY_FNV1A64: u64 = 0x{build_source_registry_digest:016x};\n\n"
    ));

    generated.push_str("pub const CANON_BUILTIN_NAMES: &[(Sid8, &str)] = &[\n");
    for (id, name) in BUILTIN_PROJECTIONS {
        generated.push_str(&format!("    (my_lisp::sid!({id}), {name:?}),\n"));
    }
    generated.push_str("];\n\n");

    generated.push_str("pub const CANON_UPPER_SURFACES: &[&str] = &[\n");
    for surface in &upper_surfaces {
        generated.push_str(&format!("    {surface:?},\n"));
    }
    generated.push_str("];\n");
    generated.push_str("pub const CANON_EXACT_SURFACES: &[&str] = &[\n");
    for surface in &exact_surfaces {
        generated.push_str(&format!("    {surface:?},\n"));
    }
    generated.push_str("];\n\n");

    for (name, ids) in DISPATCH_FORMS {
        let (upper, exact) = collect_surfaces(root, ids);
        generated.push_str(&format!("pub const CANON_{name}_UPPER: &[&str] = &[\n"));
        for surface in &upper {
            generated.push_str(&format!("    {surface:?},\n"));
        }
        generated.push_str("];\n");
        generated.push_str(&format!("pub const CANON_{name}_EXACT: &[&str] = &[\n"));
        for surface in &exact {
            generated.push_str(&format!("    {surface:?},\n"));
        }
        generated.push_str("];\n\n");
    }

    let out_dir = env::var("OUT_DIR").expect("OUT_DIR must be set");
    let out_path = PathBuf::from(out_dir).join("canon_spellings.rs");
    fs::write(&out_path, generated)
        .unwrap_or_else(|e| panic!("cml#14: could not write {}: {e}", out_path.display()));

    // Generate machine-readable table: contracts/cml-operations.lisp
    let mut s_expr = String::new();
    s_expr.push_str(
        "; cml-operations.lisp — machine-readable table of CML operations and Canon projections\n",
    );
    s_expr.push_str("; Generated at build time from my-lisp/lib/surface/semantic-registry.lisp. DO NOT EDIT BY HAND.\n\n");
    s_expr.push_str("((kind . cml-operations-table)\n");
    s_expr.push_str(" (version . (1 0))\n");
    s_expr.push_str(" (authority . ((language . juv4uk/sens)\n");
    s_expr.push_str("               (compiler . juv4uk/cml)))\n");
    s_expr.push_str(" (operations\n  . (");
    for (i, (index, id)) in operation_ids.iter().enumerate() {
        let op = &OPERATIONS[*index];
        let sid_text = format!("{:08b}", id);
        let surfaces = collect_surfaces_detailed(root, &sid_text);
        if i > 0 {
            s_expr.push_str("\n     ");
        }
        s_expr.push_str(&format!("((canonical-name . {:?})\n", op.canonical_name));
        s_expr.push_str(&format!("      (semantic-id . {:?})\n", sid_text));
        s_expr.push_str(&format!("      (formal-action . {:?})\n", op.formal_action));
        s_expr.push_str("      (surfaces . (");
        for (j, (lang, word)) in surfaces.iter().enumerate() {
            if j > 0 {
                s_expr.push(' ');
            }
            s_expr.push_str(&format!("({lang} . {word:?})"));
        }
        s_expr.push_str("))\n");
        s_expr.push_str(&format!(
            "      (cml-ir-projection . {:?})\n",
            op.cml_ir_projection
        ));
        s_expr.push_str("      (backend-projections . (");
        for (j, (backend, proj)) in op.backend_projections.iter().enumerate() {
            if j > 0 {
                s_expr.push(' ');
            }
            s_expr.push_str(&format!("({backend} . {proj:?})"));
        }
        s_expr.push_str("))\n");
        s_expr.push_str(&format!("      (status . {})\n", op.status));
        s_expr.push_str(&format!(
            "      (authority-owner . {:?})\n",
            op.authority_owner
        ));
        s_expr.push_str(&format!(
            "      (provenance-witness . {:?}))",
            op.provenance_witness
        ));
    }
    s_expr.push_str(")))\n");

    let operations_path = PathBuf::from(&manifest_dir)
        .join("contracts")
        .join("cml-operations.lisp");
    fs::write(&operations_path, s_expr)
        .unwrap_or_else(|e| panic!("cml#14: could not write {}: {e}", operations_path.display()));
}
