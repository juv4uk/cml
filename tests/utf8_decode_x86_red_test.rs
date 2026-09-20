use std::fs;
use std::path::PathBuf;

use cml::macros::MacroExpander;
use cml::x86_freestanding::X86FreestandingBackend;
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

#[test]
fn real_utf8_decode_onto_reaches_x86_tail_loop_backend() {
    // my-lisp#501 / d2951bef preserved the sequential let* law while
    // expressing its expansion constructor on the primitive macro substrate.
    // Replay that merged Lisp-owned law against the same pinned decoder so
    // this RED asks the backend question only, without broad dependency drift.
    let let_star = r#"
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

    let utf8 = pinned_utf8_source();
    let decoder = top_level_form(&utf8, "(def utf8-decode-onto");

    let mut source = String::from(let_star);
    source.push('\n');
    source.push_str(&decoder);
    source.push_str("\n(utf8-decode-onto (quote (65)) (quote ()))\n");

    let parsed = parser::parse(&source).expect("real decoder program must parse");
    let expanded = MacroExpander::new()
        .process(&parsed)
        .expect("merged Lisp-owned let* law must expand before x86 lowering");
    let lowered = lower::lower_program_with_tail_calls(&expanded)
        .expect("real decoder program must reach backend-neutral tail-loop IR");
    let assembly = X86FreestandingBackend::new()
        .compile_program(&lowered)
        .expect("#89: x86 must admit the real Lisp-owned decoder walker as a native loop");

    assert!(
        assembly.contains("jmp .Ltcloop_"),
        "#89: real decoder self recursion must become an x86 loop back-edge"
    );
}
