use std::fs;
use std::path::PathBuf;

use cml::ir::Ir;
use cml::macros::MacroExpander;
use cml::{lower, parser};

fn pinned_my_lisp_source(relative: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("external/my-lisp")
        .join(relative);
    fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "#89 requires pinned upstream Lisp authority at {}: {error}",
            path.display()
        )
    })
}

fn top_level_form(source: &str, marker: &str) -> String {
    let start = source
        .find(marker)
        .unwrap_or_else(|| panic!("pinned upstream must contain {marker}"));
    let mut depth = 0usize;
    let mut started = false;
    for (offset, ch) in source[start..].char_indices() {
        match ch {
            '(' => {
                depth += 1;
                started = true;
            }
            ')' => {
                depth -= 1;
                if started && depth == 0 {
                    return source[start..start + offset + 1].to_string();
                }
            }
            _ => {}
        }
    }
    panic!("unterminated upstream form beginning with {marker}")
}

fn contains_tail_self_call(ir: &Ir) -> bool {
    match ir {
        Ir::TailSelfCall { .. } => true,
        Ir::Def { value, .. } => contains_tail_self_call(value),
        Ir::Lambda { body, .. } => contains_tail_self_call(body),
        Ir::Cond { branches } => branches
            .iter()
            .any(|(test, body)| contains_tail_self_call(test) || contains_tail_self_call(body)),
        Ir::CondMatch { branches } => branches.iter().any(|(query, _expected, body)| {
            contains_tail_self_call(query) || contains_tail_self_call(body)
        }),
        Ir::Let { bindings, body } => {
            bindings
                .iter()
                .any(|(_, value)| contains_tail_self_call(value))
                || contains_tail_self_call(body)
        }
        Ir::App { func, args } => {
            contains_tail_self_call(func) || args.iter().any(contains_tail_self_call)
        }
        Ir::Prim { args, .. } | Ir::MachinePrim { args, .. } => {
            args.iter().any(contains_tail_self_call)
        }
        _ => false,
    }
}

#[test]
fn pinned_utf8_decode_onto_reaches_tail_loop_through_real_macro_frontend() {
    // lower.rs explicitly requires macro-expanded input. Carry the pinned
    // Lisp-owned let* law with the real decoder instead of teaching lowering
    // a UTF-8-specific or let*-specific shortcut.
    let core = pinned_my_lisp_source("lib/core.lisp");
    let utf8 = pinned_my_lisp_source("lib/utf8.lisp");
    let let_star = top_level_form(&core, "(defmacro let*");
    let decoder = top_level_form(&utf8, "(def utf8-decode-onto");

    let mut parsed = parser::parse(&let_star).expect("pinned Lisp-owned let* macro must parse");
    parsed.extend(parser::parse(&decoder).expect("real upstream utf8-decode-onto must parse"));

    let expanded = MacroExpander::new()
        .process(&parsed)
        .expect("#89: CML frontend must expand the pinned Lisp-owned let* law before lowering");
    let lowered = lower::lower_program_with_tail_calls(&expanded).expect(
        "#89: expanded upstream utf8-decode-onto must lower without a UTF-8-specific opcode",
    );

    assert!(
        lowered.iter().any(contains_tail_self_call),
        "#89: the real Lisp-owned decoder must reuse generic TailSelfCall loop lowering"
    );
}
