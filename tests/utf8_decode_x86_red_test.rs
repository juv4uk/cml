use std::fs;
use std::path::PathBuf;

use cml::ast::Expr as CExpr;
use cml::ir::Ir;
use cml::macros::MacroExpander;
use cml::upstream_sid_bridge::{convert_lisp_expr, key_definition_by_sid};
use cml::x86_freestanding::X86FreestandingBackend;
use cml::{lower, parser};

const MERGED_LET_STAR: &str = r#"
(defmacro let* (bindings body)
  (cond
    ((atom bindings) body)
    (t
     (cons (quote let)
           (cons (cons (car bindings) (quote ()))
                 (cons (cons (quote let*)
                             (cons (cdr bindings)
                                   (cons body (quote ()))))
                       (quote ())))))))
"#;

const MERGED_AND_OR: &str = r#"
(defmacro and rest
  (cond
    ((atom rest) t)
    ((atom (cdr rest)) (car rest))
    (t
     (cons (quote cond)
           (cons (cons (car rest)
                       (cons (cons (quote and) (cdr rest))
                             (quote ())))
                 (cons (cons t
                             (cons (quote ())
                                   (quote ())))
                       (quote ())))))))

(defmacro or rest
  (cond
    ((atom rest) (quote ()))
    ((atom (cdr rest)) (car rest))
    (t
     (cons (quote cond)
           (cons (cons (car rest)
                       (cons t (quote ())))
                 (cons (cons t
                             (cons (cons (quote or) (cdr rest))
                                   (quote ())))
                       (quote ())))))))
"#;

fn pinned_lisp_source(relative: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("external/my-lisp")
        .join(relative);
    fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "#89 requires pinned upstream Lisp law at {}: {error}",
            path.display()
        )
    })
}

fn lisp_owned_define(source: &str, name: &str) -> Vec<CExpr> {
    // core.lisp speaks byte-SID for library-defined functions (list,
    // reverse-onto, reverse...): (00001001 <name> <value>). Those rows are
    // consumed through the Lisp-owned reader + SID bridge, never by
    // reconstructing a classic spelling that the pin no longer contains.
    let parsed = my_lisp::parse(source).expect("pinned core.lisp must parse");
    let mut out = Vec::new();
    for expr in parsed {
        if let my_lisp::ExprKind::List(items) = &expr.kind {
            let is_define = matches!(
                items.first().map(|h| &h.kind),
                Some(my_lisp::ExprKind::Sid(s)) if s.to_string() == "00001001"
            );
            let named = matches!(
                items.get(1).map(|n| &n.kind),
                Some(my_lisp::ExprKind::Symbol(s)) if s.as_ref() == name
            );
            if is_define && named {
                out.push(key_definition_by_sid(
                    convert_lisp_expr(&expr).expect("byte-SID row must project"),
                ));
            }
        }
    }
    out
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

fn collect_apps(ir: &Ir, out: &mut Vec<String>) {
    match ir {
        Ir::App { func, args } => {
            if out.len() < 40 {
                out.push(format!("func={func:?}; arity={}", args.len()));
            }
            collect_apps(func, out);
            for arg in args {
                collect_apps(arg, out);
            }
        }
        Ir::Lambda { body, .. } | Ir::Def { value: body, .. } => collect_apps(body, out),
        Ir::Cond { branches } => {
            for (test, body) in branches {
                collect_apps(test, out);
                collect_apps(body, out);
            }
        }
        Ir::CondMatch { branches } => {
            for (query, _expected, body) in branches {
                collect_apps(query, out);
                collect_apps(body, out);
            }
        }
        Ir::Let { bindings, body } => {
            for (_, value) in bindings {
                collect_apps(value, out);
            }
            collect_apps(body, out);
        }
        Ir::Prim { args, .. } | Ir::MachinePrim { args, .. } | Ir::TailSelfCall { args } => {
            for arg in args {
                collect_apps(arg, out);
            }
        }
        _ => {}
    }
}

#[test]
fn merged_and_or_macros_expand_on_the_primitive_macro_substrate() {
    // my-lisp#525 / #527 / ece1fede rewrote only the AST constructors;
    // short-circuit law stays Lisp-owned. Replay the merged law until the
    // frozen supported pin advances.
    let source = format!("{MERGED_AND_OR}\n(and t t)\n(or (quote ()) t)\n");
    let parsed = parser::parse(&source).expect("merged AND/OR macro source must parse");
    MacroExpander::new().process(&parsed).expect(
        "#89: merged Lisp-owned AND/OR macros must expand on the primitive macro substrate",
    );
}

// Post-#145 replay marker: semantic 1017 <= and 1018 >= now lower to
// distinct ExactQLe/ExactQGe on master@36787b95; keep this real
// dependency-closure witness otherwise unchanged so the next blocker is real.
#[test]
fn real_utf8_decode_onto_reaches_x86_tail_loop_backend() {
    // Build the real decoder's Lisp-owned dependency closure rather than
    // asking CML to invent private LIST/REVERSE/NOT/AND/OR semantics.
    // LET* and AND/OR are replayed from their merged upstream portability
    // commits while external/my-lisp remains the frozen compatibility pin.
    let core = pinned_lisp_source("lib/core.lisp");
    let utf8 = pinned_lisp_source("lib/utf8.lisp");

    // Bootstrap macros are replayed from their merged upstream portability
    // commits while external/my-lisp remains the frozen compatibility pin.
    let mut forms: Vec<CExpr> = Vec::new();
    forms.extend(parser::parse(MERGED_LET_STAR).expect("merged LET* rerun must parse"));
    forms.extend(parser::parse(MERGED_AND_OR).expect("merged AND/OR rerun must parse"));

    // core.lisp speaks byte-SID for library-defined functions (list,
    // reverse-onto, reverse...); consume those rows through the Lisp-owned
    // reader + SID bridge rather than re-spelling the machine law.
    for name in ["list", "reverse-onto", "reverse", "not", "truthy?"] {
        forms.extend(lisp_owned_define(&core, name));
    }

    // The decoder and its exact-Q helpers stay classic-first in utf8.lisp.
    forms.extend(
        parser::parse(&top_level_form(&utf8, "(def utf8-decode-onto"))
            .expect("classic decoder def must parse"),
    );
    forms.push(
        parser::parse(&top_level_form(&utf8, "(def utf8-in-range?\n"))
            .expect("classic utf8-in-range? face must parse")
            .remove(0),
    );

    let parsed = MacroExpander::new()
        .process(&forms)
        .expect("real decoder dependency closure must macro-expand");
    let lowered = lower::lower_program_with_tail_calls(&parsed)
        .expect("real decoder dependency closure must reach backend-neutral IR");

    let mut residual_apps = Vec::new();
    for ir in &lowered {
        collect_apps(ir, &mut residual_apps);
    }

    let assembly = match X86FreestandingBackend::new().compile_program(&lowered) {
        Ok(assembly) => assembly,
        Err(error) => panic!(
            "#89: x86 must admit the real Lisp-owned decoder dependency closure: {error:?}; residual lowered applications: {residual_apps:#?}"
        ),
    };

    assert!(
        assembly.contains("jmp .Ltcloop_"),
        "#89: real decoder self recursion must become an x86 loop back-edge"
    );
}
