//! cml#9: generate the Canon 0+7 reserved-surface list from the real,
//! vendored my-lisp `lib/surface/semantic-registry.wsm` instead of a
//! hand-transcribed duplicate living in `src/semantic.rs`. Same pattern
//! wsm-my-lisp proved for its own `dll/build.rs` (minimal s-expression
//! reader, not a full Lisp parser -- this registry's shape doesn't need
//! one): read the registry at build time, extract the specific IDs cml
//! actually reserves against shadowing, generate a small Rust source file,
//! and fail the build (not silently emit an empty list) if the registry is
//! missing, malformed, or an expected ID has no real surfaces.
//!
//! Scope deliberately narrow, per wsm-my-lisp's own advice: only the seven
//! Canon 0+7 IDs `semantic.rs`'s `is_reserved_canon_surface` already
//! covers (0001 quote, 0002 atom, 0003 eq, 0004 cons, 0005 car, 0006 cdr,
//! 0007 cond) -- not the whole registry, and not lambda/define/defmacro
//! (0010/0011/0012), which is a separate, larger question already flagged
//! in issue cml#9 and not resolved here.

use std::env;
use std::fs;
use std::path::PathBuf;

/// IDs whose surfaces feed `is_reserved_canon_surface`. Order matches the
/// registry's own numbering, not any semantic ranking.
const TARGET_IDS: &[&str] = &["0001", "0002", "0003", "0004", "0005", "0006", "0007"];

#[derive(Debug, Clone)]
enum Sexp {
    Atom(String),
    List(Vec<Sexp>),
}

/// Minimal whitespace/paren tokenizer + recursive-descent reader. Not a
/// full Lisp reader: no quote sugar, no strings, no numeric literals --
/// semantic-registry.wsm's own shape (bare atoms and parenthesized lists
/// only) doesn't need one, matching wsm-my-lisp's own build.rs approach.
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
        // `;` starts a comment that runs to end of line, same convention
        // every other `.my`/`.wsm` file in this ecosystem already uses.
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
    tokens.reverse(); // pop() from the back acts like a FIFO front.
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
            "cml#9: could not read the real semantic-registry.wsm at {} ({e}). \
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
            panic!("cml#9: no top-level (sr/1 ...) form found in semantic-registry.wsm")
        });

    let mut upper_surfaces: Vec<String> = Vec::new();
    let mut exact_surfaces: Vec<String> = Vec::new();

    for id in TARGET_IDS {
        let entry = root
            .iter()
            .find(|form| matches!(form, Sexp::List(items) if !items.is_empty() && atom(&items[0]) == *id))
            .unwrap_or_else(|| panic!("cml#9: semantic-registry.wsm has no entry for Canon id {id}"));
        let fields = &list(entry)[1..];
        let mut found_any = false;
        for field in fields {
            let parts = list(field);
            if parts.len() < 2 {
                continue;
            }
            let lang = atom(&parts[0]);
            let word = atom(&parts[1]);
            if word == "—" {
                continue; // explicit "missing" placeholder, not a real surface.
            }
            found_any = true;
            match lang {
                "en" | "sym" => upper_surfaces.push(word.to_uppercase()),
                "uk" | "sa" => exact_surfaces.push(word.to_string()),
                other => panic!("cml#9: unknown surface language {other:?} for id {id}"),
            }
        }
        if !found_any {
            panic!("cml#9: Canon id {id} has zero real surfaces in semantic-registry.wsm");
        }
    }

    upper_surfaces.sort();
    upper_surfaces.dedup();
    exact_surfaces.sort();
    exact_surfaces.dedup();

    let mut generated = String::new();
    generated.push_str("// @generated by build.rs from my-lisp/lib/surface/semantic-registry.wsm (cml#9). Do not edit by hand.\n");
    generated.push_str("pub const CANON_UPPER_SURFACES: &[&str] = &[\n");
    for surface in &upper_surfaces {
        generated.push_str(&format!("    {surface:?},\n"));
    }
    generated.push_str("];\n");
    generated.push_str("pub const CANON_EXACT_SURFACES: &[&str] = &[\n");
    for surface in &exact_surfaces {
        generated.push_str(&format!("    {surface:?},\n"));
    }
    generated.push_str("];\n");

    let out_dir = env::var("OUT_DIR").expect("OUT_DIR must be set");
    let out_path = PathBuf::from(out_dir).join("canon_spellings.rs");
    fs::write(&out_path, generated)
        .unwrap_or_else(|e| panic!("cml#9: could not write {}: {e}", out_path.display()));
}
