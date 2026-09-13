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

use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::PathBuf;

/// IDs whose surfaces feed `is_reserved_canon_surface` (Canon 0+7).
const TARGET_IDS: &[&str] = &["0001", "0002", "0003", "0004", "0005", "0006", "0007"];

/// Callable primitives and library operations whose semantic identity must survive
/// surface spelling changes across English, Ukrainian, Sanskrit, and symbolic forms.
const CALLABLE_IDS: &[&str] = &[
    "0002", "0003", "0004", "0005", "0006", "0104", "1001", "1022", "1074",
];

/// Special-form dispatch sets: form name prefix -> registry IDs.
const DISPATCH_FORMS: &[(&str, &[&str])] = &[
    ("QUOTE", &["0001"]),
    ("COND", &["0007"]),
    ("LAMBDA", &["0010"]),
    ("DEFINE", &["0011", "1000"]),
    ("DEFMACRO", &["0012"]),
];

/// Canonical target builtin names for first-class values.
const BUILTIN_PROJECTIONS: &[(&str, &str)] = &[
    ("0002", "ATOM"),
    ("0003", "EQ"),
    ("0004", "CONS"),
    ("0005", "CAR"),
    ("0006", "CDR"),
    ("0104", "+"),
    ("1001", "-"),
    ("1022", "EQUAL?"),
    ("1074", "NUMERIC-BUFFER-MAP"),
];

struct OperationSpec {
    canonical_name: &'static str,
    semantic_id: &'static str,
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
        semantic_id: "0001",
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
        semantic_id: "0002",
        formal_action: "primitive:atom",
        cml_ir_projection: "Ir::Prim(PrimOp::Atom)",
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
        semantic_id: "0003",
        formal_action: "primitive:eq",
        cml_ir_projection: "Ir::Prim(PrimOp::Eq)",
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
        semantic_id: "0004",
        formal_action: "primitive:cons",
        cml_ir_projection: "Ir::Prim(PrimOp::Cons)",
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
        semantic_id: "0005",
        formal_action: "primitive:car",
        cml_ir_projection: "Ir::Prim(PrimOp::Car)",
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
        semantic_id: "0006",
        formal_action: "primitive:cdr",
        cml_ir_projection: "Ir::Prim(PrimOp::Cdr)",
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
        semantic_id: "0007",
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
        semantic_id: "0010",
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
        semantic_id: "0011",
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
        semantic_id: "1000",
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
        semantic_id: "0012",
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
        semantic_id: "0104",
        formal_action: "primitive:add",
        cml_ir_projection: "Ir::Prim(PrimOp::Add)",
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
        semantic_id: "1001",
        formal_action: "primitive:sub",
        cml_ir_projection: "Ir::Prim(PrimOp::Sub)",
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
        canonical_name: "equal?",
        semantic_id: "1022",
        formal_action: "primitive:equalp",
        cml_ir_projection: "Ir::Prim(PrimOp::EqualP)",
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
        semantic_id: "1074",
        formal_action: "library:numeric-buffer-map",
        cml_ir_projection: "Ir::App(Builtin(\"NUMERIC-BUFFER-MAP\"))",
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

fn collect_surfaces_detailed(root: &[Sexp], id: &str) -> Vec<(String, String)> {
    let entry = root
        .iter()
        .find(
            |form| matches!(form, Sexp::List(items) if !items.is_empty() && atom(&items[0]) == id),
        )
        .unwrap_or_else(|| panic!("cml#14: semantic-registry.wsm has no entry for Canon id {id}"));
    let fields = &list(entry)[1..];
    let mut surfaces = Vec::new();
    for field in fields {
        let parts = list(field);
        if parts.len() < 2 {
            continue;
        }
        let lang = atom(&parts[0]);
        let word = atom(&parts[1]);
        if word == "—" || lang == "compat" {
            continue;
        }
        surfaces.push((lang.to_string(), word.to_string()));
    }
    if surfaces.is_empty() {
        panic!("cml#14: Canon id {id} has zero real surfaces in semantic-registry.wsm");
    }
    surfaces
}

fn collect_surfaces(root: &[Sexp], ids: &[&str]) -> (Vec<String>, Vec<String>) {
    let mut upper_surfaces: Vec<String> = Vec::new();
    let mut exact_surfaces: Vec<String> = Vec::new();

    for id in ids {
        let surfaces = collect_surfaces_detailed(root, id);
        for (lang, word) in surfaces {
            match lang.as_str() {
                "en" | "sym" => upper_surfaces.push(word.to_uppercase()),
                "uk" | "sa" => exact_surfaces.push(word),
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
    let registry_path = PathBuf::from(&manifest_dir)
        .join("..")
        .join("my-lisp")
        .join("lib")
        .join("surface")
        .join("semantic-registry.wsm");
    println!("cargo:rerun-if-changed={}", registry_path.display());

    let source = fs::read_to_string(&registry_path).unwrap_or_else(|e| {
        panic!(
            "cml#14: could not read the real semantic-registry.wsm at {} ({e}). \
             This build depends on a sibling my-lisp checkout, same convention \
             as compatibility.my's own sibling-repo pin.",
            registry_path.display()
        )
    });

    let forms = parse_all(&source);
    let root = forms
        .iter()
        .find_map(|form| match form {
            Sexp::List(items)
                if !items.is_empty() && matches!(&items[0], Sexp::Atom(a) if a == "sr/1") =>
            {
                Some(&items[1..])
            }
            _ => None,
        })
        .unwrap_or_else(|| {
            panic!("cml#14: no top-level (sr/1 ...) form found in semantic-registry.wsm")
        });

    // Fail-closed collision detection across all admitted operations
    let mut seen_upper: HashMap<String, &str> = HashMap::new();
    let mut seen_exact: HashMap<String, &str> = HashMap::new();
    for op in OPERATIONS {
        let surfaces = collect_surfaces_detailed(root, op.semantic_id);
        for (lang, word) in &surfaces {
            if lang == "en" || lang == "sym" {
                let folded = word.to_uppercase();
                if let Some(prev) = seen_upper.insert(folded.clone(), op.semantic_id) {
                    if prev != op.semantic_id && !(op.semantic_id == "1000" && prev == "0011") {
                        panic!(
                            "cml#14: collision on upper surface {folded:?} between {prev} and {}",
                            op.semantic_id
                        );
                    }
                }
            } else if let Some(prev) = seen_exact.insert(word.clone(), op.semantic_id) {
                if prev != op.semantic_id {
                    panic!(
                        "cml#14: collision on exact surface {word:?} between {prev} and {}",
                        op.semantic_id
                    );
                }
            }
        }
    }

    let (upper_surfaces, exact_surfaces) = collect_surfaces(root, TARGET_IDS);

    let mut generated = String::new();
    generated.push_str("// @generated by build.rs from my-lisp/lib/surface/semantic-registry.wsm (cml#14). Do not edit by hand.\n\n");

    generated.push_str("#[derive(Debug, Clone, Copy, PartialEq, Eq)]\n");
    generated.push_str("pub struct CanonOperation {\n");
    generated.push_str("    pub canonical_name: &'static str,\n");
    generated.push_str("    pub semantic_id: &'static str,\n");
    generated.push_str("    pub formal_action: &'static str,\n");
    generated.push_str("    pub surfaces: &'static [(&'static str, &'static str)],\n");
    generated.push_str("    pub cml_ir_projection: &'static str,\n");
    generated.push_str("    pub backend_projections: &'static [(&'static str, &'static str)],\n");
    generated.push_str("    pub status: &'static str,\n");
    generated.push_str("    pub authority_owner: &'static str,\n");
    generated.push_str("    pub provenance_witness: &'static str,\n");
    generated.push_str("}\n\n");

    generated.push_str("pub const CANON_OPERATIONS_TABLE: &[CanonOperation] = &[\n");
    for op in OPERATIONS {
        let surfaces = collect_surfaces_detailed(root, op.semantic_id);
        generated.push_str("    CanonOperation {\n");
        generated.push_str(&format!(
            "        canonical_name: {:?},\n",
            op.canonical_name
        ));
        generated.push_str(&format!("        semantic_id: {:?},\n", op.semantic_id));
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

    generated.push_str("pub const CANON_BUILTIN_NAMES: &[(&str, &str)] = &[\n");
    for (id, name) in BUILTIN_PROJECTIONS {
        generated.push_str(&format!("    ({id:?}, {name:?}),\n"));
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

    generated.push_str("pub const CANON_CALLABLE_UPPER: &[(&str, &str)] = &[\n");
    let mut callable_exact = Vec::new();
    for id in CALLABLE_IDS {
        let (upper, exact) = collect_surfaces(root, &[*id]);
        for surface in upper {
            generated.push_str(&format!("    ({surface:?}, {id:?}),\n"));
        }
        for surface in exact {
            callable_exact.push((surface, *id));
        }
    }
    generated.push_str("];\n");
    generated.push_str("pub const CANON_CALLABLE_EXACT: &[(&str, &str)] = &[\n");
    for (surface, id) in callable_exact {
        generated.push_str(&format!("    ({surface:?}, {id:?}),\n"));
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

    // Generate machine-readable table: contracts/cml-operations.my
    let mut s_expr = String::new();
    s_expr.push_str(
        "; cml-operations.my — machine-readable table of CML operations and Canon projections\n",
    );
    s_expr.push_str("; Generated at build time from my-lisp/lib/surface/semantic-registry.wsm. DO NOT EDIT BY HAND.\n\n");
    s_expr.push_str("((kind . cml-operations-table)\n");
    s_expr.push_str(" (version . (1 0))\n");
    s_expr.push_str(" (authority . ((language . juv4uk/my-lisp)\n");
    s_expr.push_str("               (compiler . juv4uk/cml)))\n");
    s_expr.push_str(" (operations\n  . (");
    for (i, op) in OPERATIONS.iter().enumerate() {
        let surfaces = collect_surfaces_detailed(root, op.semantic_id);
        if i > 0 {
            s_expr.push_str("\n     ");
        }
        s_expr.push_str(&format!("((canonical-name . {:?})\n", op.canonical_name));
        s_expr.push_str(&format!("      (semantic-id . {:?})\n", op.semantic_id));
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
        .join("cml-operations.my");
    fs::write(&operations_path, s_expr)
        .unwrap_or_else(|e| panic!("cml#14: could not write {}: {e}", operations_path.display()));
}
