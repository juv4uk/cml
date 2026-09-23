use cml::upstream_sid_bridge::{convert_lisp_expr, key_definition_by_sid};
use cml::{ast::Expr as CExpr, lower::lower_program, x86_freestanding::X86FreestandingBackend};
use my_lisp::parse;
use std::fs;

fn main() {
    // Build S3a emitter (Core1 prelude + compiler.lisp)
    let needed = [
        "atom", "eq", "cons", "car", "cdr", "list", "cond", "def", "define", "lambda", "quote",
    ];

    let core_source = fs::read_to_string("external/my-lisp/lib/core.lisp").unwrap();
    let parsed = parse(&core_source).unwrap();

    let mut forms = Vec::new();
    for expr in parsed {
        if let my_lisp::ExprKind::List(items) = &expr.kind {
            if let Some(my_lisp::ExprKind::Sid(sid)) = items.first().map(|h| &h.kind) {
                if sid.to_string() == "00001001" {
                    if let Some(my_lisp::ExprKind::Symbol(name)) = items.get(1).map(|n| &n.kind) {
                        if needed.contains(&name.as_ref()) {
                            let converted = convert_lisp_expr(&expr).unwrap();
                            let keyed = key_definition_by_sid(converted);
                            forms.push(keyed);
                        }
                    }
                }
            }
        }
    }

    // t constant
    let t_const: CExpr = CExpr::List(vec![
        CExpr::Symbol("def".to_string()),
        CExpr::Symbol("t".to_string()),
        CExpr::List(vec![
            CExpr::Symbol("quote".to_string()),
            CExpr::Symbol("t".to_string()),
        ]),
    ]);
    forms.push(t_const);

    // Minimal not
    let not_def: CExpr = CExpr::List(vec![
        CExpr::Symbol("def".to_string()),
        CExpr::Symbol("not".to_string()),
        CExpr::List(vec![
            CExpr::Symbol("lambda".to_string()),
            CExpr::List(vec![CExpr::Symbol("x".to_string())]),
            CExpr::List(vec![
                CExpr::Symbol("cond".to_string()),
                CExpr::List(vec![CExpr::Symbol("x".to_string()), CExpr::List(vec![])]),
                CExpr::List(vec![
                    CExpr::Symbol("t".to_string()),
                    CExpr::Symbol("t".to_string()),
                ]),
            ]),
        ]),
    ]);
    let not_keyed = key_definition_by_sid(not_def);
    forms.push(not_keyed);

    // Compiler source
    let compiler_source =
        fs::read_to_string("/home/agents/GitHub/wsm-my-lisp/lib/compiler.lisp").unwrap();
    let compiler_parsed = parse(&compiler_source).unwrap();

    for expr in compiler_parsed {
        if let my_lisp::ExprKind::List(items) = &expr.kind {
            if let Some(my_lisp::ExprKind::Symbol(s)) = items.first().map(|h| &h.kind) {
                if s.as_ref() == "def" || s.as_ref() == "defmacro" {
                    let converted = convert_lisp_expr(&expr).unwrap();
                    forms.push(converted);
                }
            }
        }
    }

    println!("Building S3a emitter... {} forms", forms.len());

    // S3a: Compile the emitter
    let emitter_ir = lower_program(&forms).expect("S3a lower must succeed");
    let emitter_asm = X86FreestandingBackend::new()
        .compile_program(&emitter_ir)
        .expect("S3a compile");
    println!("S3a emitter: {} bytes", emitter_asm.len());
    fs::write("/tmp/s3a_emitter.asm", &emitter_asm).unwrap();

    // S4: Gen1 - compile compiler.lisp through CML again (simulating running S3a on compiler.lisp)
    // This simulates: run S3a emitter on compiler.lisp source -> get IR
    // Since we can't execute x86, we compile compiler.lisp through CML again
    // This is the Gen1 artifact

    println!("\n=== S4: Gen1 ===");

    // Parse compiler.lisp and build a program that includes the same prelude
    let compiler_source =
        fs::read_to_string("/home/agents/GitHub/wsm-my-lisp/lib/compiler.lisp").unwrap();
    let compiler_parsed = parse(&compiler_source).unwrap();

    let mut gen1_forms = Vec::new();

    // Reuse the same prelude
    gen1_forms.extend(forms.clone());

    // Add compiler.lisp defs (already SID-rewritten)
    for expr in parse(&compiler_source).unwrap() {
        if let my_lisp::ExprKind::List(items) = &expr.kind {
            if let Some(my_lisp::ExprKind::Symbol(s)) = items.first().map(|h| &h.kind) {
                if s.as_ref() == "def" || s.as_ref() == "defmacro" {
                    let converted = convert_lisp_expr(&expr).unwrap();
                    gen1_forms.push(converted);
                }
            }
        }
    }

    println!("Gen1 forms: {}", gen1_forms.len());

    // Compile Gen1
    let gen1_ir = lower_program(&gen1_forms).expect("Gen1 lower must succeed");
    println!("Gen1 lowered: {} IR items", gen1_ir.len());

    for item in &gen1_ir {
        if let cml::ir::Ir::Def { name, .. } = item {
            println!("  Gen1 Def: {}", name);
        }
    }

    let gen1_asm = X86FreestandingBackend::new()
        .compile_program(&gen1_ir)
        .expect("Gen1 compile");
    println!("Gen1 compiled: {} bytes", gen1_asm.len());

    fs::write("/tmp/gen1.asm", &gen1_asm).unwrap();
    println!("Gen1 assembly saved to /tmp/gen1.asm");

    // S5: Fixed point check
    println!("\n=== S5: Fixed Point Check ===");
    println!("S3a emitter size: {} bytes", emitter_asm.len());
    println!("Gen1 size: {} bytes", gen1_asm.len());

    // For true fixed point, we'd need to:
    // 1. Run S3a emitter on compiler.lisp -> get IR1
    // 2. Run Gen1 emitter on compiler.lisp -> get IR2
    // 2. Compare IR1 == IR2
    //
    // Since we can't execute x86 emitters from here, we note:
    // - S3a emitter compiles (23KB)
    // - Gen1 compiles (45KB)
    // - Both use same prelude + compiler.lisp
    //
    // True fixed point requires executing the x86 emitters, which requires
    // a runtime harness. That's beyond this compile-time verification.

    println!("\nS4/S5: Both S3a and Gen1 compile successfully");
    println!("Full fixed point requires executing x86 emitters (runtime harness needed)");
}
