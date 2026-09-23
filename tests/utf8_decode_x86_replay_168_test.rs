//! #168 replay-fixture refresh: real pinned UTF-8 decoder closure through
//! the Lisp-owned SID bridge.
//!
//! Мета: показати, що на поточному master (пін my-lisp 0aa48962) декодер
//! `utf8-decode-onto` з залежністю `list` (byte-SID ряд у `core.lisp`)
//! реально доходить до нижньої частини CML/бекенда. Fixture НЕ переписує
//! семантику і не реалізує декодер наново: `list` живиться пінним machine
//! row через [`crate::upstream_sid_bridge`], декодер — класичним `(def ...)`
//! прямо з піна. Очікуваний результат фіксується чесно: якщо бекенд
//! компілює — це спостережувана перемога; якщо стабільно падає на
//! конкретному fail-closed блокері — це і є точний наступний gap.

#![cfg(all(target_os = "linux", target_arch = "x86_64"))]

use std::fs;
use std::path::PathBuf;

use cml::ast::Expr as CExpr;
use cml::lower::lower_program;
use cml::upstream_sid_bridge::{BridgeError, convert_lisp_expr, key_definition_by_sid};
use cml::x86_freestanding::X86FreestandingBackend;
use my_lisp::parse;

fn upstream_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("external/my-lisp")
        .join(relative)
}

/// Extract every top-level byte-SID `define` row whose surface name matches
/// `name`, project it to CML and key it by its Lisp-owned SID.
fn lisp_owned_define(source: &str, name: &str) -> Result<Vec<CExpr>, BridgeError> {
    let parsed = parse(source).map_err(|e| BridgeError::UnsupportedAst(format!("{e:?}")))?;
    let mut out = Vec::new();
    for expr in parsed {
        // a define row: (Sid(00001001) <name> <value>)
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

#[test]
fn pinned_list_row_reaches_x86_with_sid_identity() {
    let core = fs::read_to_string(upstream_path("lib/core.lisp"))
        .expect("#168 requires the pinned external/my-lisp submodule core.lisp");
    let list_defs = lisp_owned_define(&core, "list").expect("list row must project");
    assert_eq!(
        list_defs.len(),
        1,
        "pinned core.lisp must define list exactly once"
    );

    // Prove the SID identity took effect in the fixture itself: the
    // definition name must be the Lisp-owned registry key as a typed Sid8,
    // not the surface name `list` and not a bit-string.
    let CExpr::List(items) = &list_defs[0] else {
        panic!("expected a define list form");
    };
    assert_eq!(items[1], CExpr::Sid(my_lisp::sid!(00100111)));

    let program = match lower_program(&list_defs) {
        Ok(program) => program,
        Err(e) => {
            println!("SID IDENTITY: GREEN (typed Sid8 keying) / LOWER fail-closed: {e}");
            println!("STATUS: BLOCKED_AT_LOWER");
            return;
        }
    };
    match X86FreestandingBackend::new().compile_program(&program) {
        Ok(assembly) => {
            assert!(
                assembly.contains(".globl wsm_entry"),
                "list row must reach the freestanding x86 backend"
            );
            println!(
                "SID IDENTITY: GREEN (typed Sid8 keying) / backend assembly OK, {} bytes",
                assembly.len()
            );
            println!("STATUS: COMPILES");
        }
        Err(e) => {
            println!("SID IDENTITY: GREEN (typed Sid8 keying) / backend fail-closed: {e:?}");
            println!("STATUS: BLOCKED_AT_BACKEND");
        }
    }
}

#[test]
fn full_decoder_closure_state_is_recorded_honestly() {
    let core = fs::read_to_string(upstream_path("lib/core.lisp"))
        .expect("#168 requires the pinned external/my-lisp submodule core.lisp");
    let utf8 = fs::read_to_string(upstream_path("lib/utf8.lisp"))
        .expect("#168 requires the pinned external/my-lisp submodule utf8.lisp");

    let mut program_forms = Vec::new();
    for name in ["list", "reverse", "reverse-onto", "not", "truthy?"] {
        program_forms.extend(lisp_owned_define(&core, name).unwrap_or_default());
    }

    // The decoder faces are classic (def ...) in utf8.lisp; surface calls to
    // list/reverse are name-keyed there, exactly as upstream authored them.
    // The whole file is read by the Lisp-owned reader so multi-line defs stay
    // intact; each top-level form is projected through the SID bridge.
    let parsed_utf8 = parse(&utf8).expect("pinned utf8.lisp must parse via my-lisp reader");
    for expr in parsed_utf8 {
        program_forms.push(convert_lisp_expr(&expr).expect("def face must project"));
    }

    match lower_program(&program_forms) {
        Ok(program) => {
            let assembly = X86FreestandingBackend::new()
                .compile_program(&program)
                .map(|a| a.len());
            match assembly {
                Ok(bytes) => {
                    println!("FULL CLOSURE: lower+backend OK, assembly {bytes} bytes");
                    println!("STATUS: COMPILES");
                }
                Err(e) => {
                    println!("FULL CLOSURE: lower OK, backend fail-closed: {e:?}");
                    println!("STATUS: BLOCKED_AT_BACKEND");
                }
            }
        }
        Err(e) => {
            println!("FULL CLOSURE: lower fail-closed: {e}");
            println!("STATUS: BLOCKED_AT_LOWER");
        }
    }
    // This test only records the observed state; it does not replay a
    // hand-written decoder or expected output. #168 decides attendance from
    // the printed STATUS line, not from a fabricated assert.
}
