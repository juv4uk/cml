use std::fs;
use std::path::PathBuf;

use cml::ir::Ir;
use cml::macros::MacroExpander;
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

    let dependencies = [
        top_level_form(&core, "(def list "),
        top_level_form(&core, "(def not"),
        top_level_form(&core, "(def reverse-onto"),
        top_level_form(&core, "(def reverse\n"),
        top_level_form(&utf8, "(def utf8-continuation-byte?"),
    ];
    let decoder = top_level_form(&utf8, "(def utf8-decode-onto");

    let mut source = String::new();
    source.push_str(MERGED_LET_STAR);
    source.push('\n');
    source.push_str(MERGED_AND_OR);
    source.push('\n');
    for dependency in dependencies {
        source.push_str(&dependency);
        source.push('\n');
    }
    source.push_str(&decoder);
    source.push_str("\n(utf8-decode-onto (quote (65)) (quote ()))\n");

    let parsed = parser::parse(&source).expect("real decoder dependency closure must parse");
    let expanded = MacroExpander::new()
        .process(&parsed)
        .expect("merged Lisp-owned bootstrap macros must expand before x86 lowering");
    let lowered = lower::lower_program_with_tail_calls(&expanded)
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
