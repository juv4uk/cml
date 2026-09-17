use std::fs;
use std::path::PathBuf;

use cml::ir::Ir;
use cml::{lower, parser};

fn pinned_utf8_source() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("external/my-lisp/lib/utf8.lisp");
    fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "#89 requires pinned upstream UTF-8 law at {}: {error}",
            path.display()
        )
    })
}

fn utf8_decode_onto_form(source: &str) -> String {
    let start = source
        .find("(def utf8-decode-onto")
        .expect("pinned upstream must define utf8-decode-onto");
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
    panic!("unterminated utf8-decode-onto form in pinned upstream")
}

fn contains_tail_self_call(ir: &Ir) -> bool {
    match ir {
        Ir::TailSelfCall { .. } => true,
        Ir::Def { value, .. } => contains_tail_self_call(value),
        Ir::Lambda { body, .. } => contains_tail_self_call(body),
        Ir::Cond { branches } => branches
            .iter()
            .any(|(test, body)| contains_tail_self_call(test) || contains_tail_self_call(body)),
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
fn pinned_utf8_decode_onto_reaches_existing_tail_loop_ir() {
    let source = pinned_utf8_source();
    let form = utf8_decode_onto_form(&source);
    let parsed = parser::parse(&form).expect("real upstream utf8-decode-onto must parse");
    let lowered = lower::lower_program(&parsed)
        .expect("#89: real upstream utf8-decode-onto must lower without a UTF-8-specific opcode");

    assert!(
        lowered.iter().any(contains_tail_self_call),
        "#89: the real Lisp-owned decoder must reuse generic TailSelfCall loop lowering"
    );
}
