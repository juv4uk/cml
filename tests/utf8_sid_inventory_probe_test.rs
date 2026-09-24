#![cfg(all(target_os = "linux", target_arch = "x86_64"))]

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use cml::ast::Expr as CExpr;
use cml::ir::Ir;
use cml::lower::lower_program;
use cml::upstream_sid_bridge::{BridgeError, convert_lisp_expr, key_definition_by_sid};
use my_lisp::parse;

fn upstream_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("external/my-lisp")
        .join(relative)
}

fn lisp_owned_define(source: &str, name: &str) -> Result<Vec<CExpr>, BridgeError> {
    let parsed = parse(source).map_err(|e| BridgeError::UnsupportedAst(format!("{e:?}")))?;
    let mut out = Vec::new();
    for expr in parsed {
        if let my_lisp::ExprKind::List(items) = &expr.kind {
            let head_is_define = matches!(
                items.first().map(|h| &h.kind),
                Some(my_lisp::ExprKind::Sid(s)) if s.to_string() == "00001001"
            );
            let name_matches = matches!(
                items.get(1).map(|n| &n.kind),
                Some(my_lisp::ExprKind::Symbol(s)) if s.as_ref() == name
            );
            if head_is_define && name_matches {
                out.push(key_definition_by_sid(convert_lisp_expr(&expr)?));
            }
        }
    }
    Ok(out)
}

fn walk(ir: &Ir, seen: &mut BTreeSet<my_lisp::Sid8>, ordered: &mut Vec<my_lisp::Sid8>) {
    match ir {
        Ir::Sid(sid) => {
            if seen.insert(*sid) {
                ordered.push(*sid);
            }
        }
        Ir::App { func, args } => {
            walk(func, seen, ordered);
            for arg in args { walk(arg, seen, ordered); }
        }
        Ir::Lambda { body, .. } => walk(body, seen, ordered),
        Ir::Def { value, .. } => walk(value, seen, ordered),
        Ir::Let { bindings, body } => {
            for (_, value) in bindings { walk(value, seen, ordered); }
            walk(body, seen, ordered);
        }
        Ir::Cond { branches } => {
            for (test, body) in branches {
                walk(test, seen, ordered);
                walk(body, seen, ordered);
            }
        }
        Ir::CondMatch { branches } => {
            for (query, _expected, body) in branches {
                walk(query, seen, ordered);
                walk(body, seen, ordered);
            }
        }
        Ir::Prim { args, .. } | Ir::MachinePrim { args, .. } | Ir::TailSelfCall { args } => {
            for arg in args { walk(arg, seen, ordered); }
        }
        _ => {}
    }
}

#[test]
fn print_current_utf8_sid_call_inventory() {
    let core = fs::read_to_string(upstream_path("lib/core.lisp")).unwrap();
    let utf8 = fs::read_to_string(upstream_path("lib/utf8.lisp")).unwrap();

    let mut forms = Vec::new();
    for name in ["list", "reverse", "reverse-onto", "not", "truthy?"] {
        forms.extend(lisp_owned_define(&core, name).unwrap_or_default());
    }
    for expr in parse(&utf8).unwrap() {
        forms.push(convert_lisp_expr(&expr).unwrap());
    }

    let program = lower_program(&forms).unwrap();
    let mut seen = BTreeSet::new();
    let mut ordered = Vec::new();
    for ir in &program {
        walk(ir, &mut seen, &mut ordered);
    }

    for sid in ordered {
        let surfaces = my_lisp::semantic_registry_export::admitted_surfaces_for_semantic_id(sid);
        let label = surfaces
            .iter()
            .find(|s| s.namespace == "en")
            .or_else(|| surfaces.first())
            .map(|s| s.name)
            .unwrap_or("<no-surface>");
        println!("UTF8_SID {} {}", sid, label);
    }
}
