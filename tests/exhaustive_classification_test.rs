use cml::c_backend::{CBackend, CompileError as CCompileError};
use cml::compiler::{Compiler, CompileError as FpgaCompileError};
use cml::ir::{BufferLiteral, Ir, Params, PrimOp, Quoted};
use cml::x86_freestanding::{X86FreestandingBackend, CompileError as X86CompileError};
use std::panic;

fn all_quoted() -> Vec<Quoted> {
    vec![
        Quoted::Int(42),
        Quoted::Float(3.14),
        Quoted::Rational(1, 2),
        Quoted::Sym("foo".to_string()),
        Quoted::Str("bar".to_string()),
        Quoted::Nil,
        Quoted::List(vec![Quoted::Int(1)]),
        Quoted::DottedList(vec![Quoted::Int(1)], Box::new(Quoted::Int(2))),
    ]
}

fn all_ir() -> Vec<Ir> {
    let mut irs = vec![
        Ir::Int(42),
        Ir::Float(3.14),
        Ir::Rational(1, 2),
        Ir::String("hello".to_string()),
        Ir::Buffer(BufferLiteral::I32(vec![1, 2, 3])),
        Ir::Buffer(BufferLiteral::F32(vec![1, 2, 3])),
        Ir::Nil,
        Ir::True,
        Ir::Var("x".to_string()),
        Ir::Builtin("car".to_string()),
        Ir::Lambda {
            params: Params::Fixed(vec![]),
            body: Box::new(Ir::Nil),
        },
        Ir::App {
            func: Box::new(Ir::Var("f".to_string())),
            args: vec![],
        },
        Ir::Cond {
            branches: vec![(Ir::True, Ir::Int(1))],
        },
        Ir::Let {
            bindings: vec![("x".to_string(), Ir::Int(1))],
            body: Box::new(Ir::Var("x".to_string())),
        },
        Ir::Def {
            name: "f".to_string(),
            value: Box::new(Ir::Lambda {
                params: Params::Fixed(vec![]),
                body: Box::new(Ir::Nil),
            }),
        },
        Ir::Prim {
            op: PrimOp::Add,
            args: vec![Ir::Int(1), Ir::Int(2)],
        },
        Ir::TailSelfCall {
            args: vec![Ir::Int(1)],
        },
    ];

    for q in all_quoted() {
        irs.push(Ir::Quote(q));
    }

    irs
}

/// Result of classifying an IR variant by a backend.
#[derive(Debug, PartialEq, Eq)]
enum Classified {
    /// Successfully emitted code.
    Emitted,
    /// Rejected with a typed CompileError (not a panic).
    RejectedTypedError(String),
}

/// Classify an IR variant using the FPGA backend.
fn classify_fpga(ir: &Ir) -> Classified {
    let result = panic::catch_unwind(|| {
        let mut compiler = Compiler::new();
        compiler.compile(&[ir.clone()])
    });
    match result {
        Ok(Ok(_)) => Classified::Emitted,
        Ok(Err(e)) => Classified::RejectedTypedError(e.to_string()),
        Err(_) => panic!("FPGA backend panicked on IR: {:?}", ir),
    }
}

/// Classify an IR variant using the C backend.
fn classify_c(ir: &Ir) -> Classified {
    let result = panic::catch_unwind(|| {
        let mut backend = CBackend::new();
        backend.compile_program(&[ir.clone()])
    });
    match result {
        Ok(Ok(_)) => Classified::Emitted,
        Ok(Err(e)) => Classified::RejectedTypedError(e.to_string()),
        Err(_) => panic!("C backend panicked on IR: {:?}", ir),
    }
}

/// Classify an IR variant using the x86 freestanding backend.
fn classify_x86(ir: &Ir) -> Classified {
    let result = panic::catch_unwind(|| {
        let backend = X86FreestandingBackend::new();
        backend.compile_program(&[ir.clone()])
    });
    match result {
        Ok(Ok(_)) => Classified::Emitted,
        Ok(Err(e)) => Classified::RejectedTypedError(e.to_string()),
        Err(_) => panic!("x86_freestanding backend panicked on IR: {:?}", ir),
    }
}

/// Legacy test: just verify no panic (kept for backward compatibility).
#[test]
fn fpga_backend_exhaustively_classifies_all_ir_without_panicking() {
    for ir in all_ir() {
        let result = panic::catch_unwind(|| {
            let mut compiler = Compiler::new();
            let _ = compiler.compile(&[ir.clone()]);
        });
        assert!(result.is_ok(), "FPGA backend panicked on IR: {:?}", ir);
    }
}

#[test]
fn c_backend_exhaustively_classifies_all_ir_without_panicking() {
    for ir in all_ir() {
        let result = panic::catch_unwind(|| {
            let mut backend = CBackend::new();
            let _ = backend.compile_program(&[ir.clone()]);
        });
        assert!(result.is_ok(), "C backend panicked on IR: {:?}", ir);
    }
}

#[test]
fn x86_freestanding_backend_exhaustively_classifies_all_ir_without_panicking() {
    for ir in all_ir() {
        let result = panic::catch_unwind(|| {
            let backend = X86FreestandingBackend::new();
            let _ = backend.compile_program(&[ir.clone()]);
        });
        assert!(
            result.is_ok(),
            "x86_freestanding backend panicked on IR: {:?}",
            ir
        );
    }
}

/// Explicit classification tests: every IR variant must be either
/// Emitted or RejectedTypedError — no panics, no wildcard paths.
#[test]
fn fpga_backend_explicitly_classifies_every_ir_variant() {
    let mut emitted = 0;
    let mut rejected = 0;

    for ir in all_ir() {
        let classified = classify_fpga(&ir);
        match classified {
            Classified::Emitted => emitted += 1,
            Classified::RejectedTypedError(err) => {
                // Verify it's a typed FpgaCompileError, not a generic string
                assert!(
                    err.contains("UnsupportedVariant") ||
                    err.contains("UnsupportedNumericBuffer") ||
                    err.contains("TooManyArguments") ||
                    err.contains("IntegerOutOfRange") ||
                    err.contains("SymbolTableOverflow") ||
                    err.contains("unsupported IR variant for FPGA target") ||
                    err.contains("unsupported typed numeric buffer for FPGA target"),
                    "FPGA error must be a typed CompileError variant, got: {}",
                    err
                );
                rejected += 1;
            }
        }
    }
    println!("FPGA: emitted={}, rejected={}", emitted, rejected);
    assert!(emitted > 0 && rejected > 0, "FPGA must both emit and reject some variants");
}

#[test]
fn c_backend_explicitly_classifies_every_ir_variant() {
    let mut emitted = 0;
    let mut rejected = 0;

    for ir in all_ir() {
        let classified = classify_c(&ir);
        match classified {
            Classified::Emitted => emitted += 1,
            Classified::RejectedTypedError(err) => {
                assert!(
                    err.contains("UnsupportedVariant") ||
                    err.contains("UnsupportedTypedBuffer") ||
                    err.contains("NestedDef") ||
                    err.contains("unsupported IR variant in C backend") ||
                    err.contains("unsupported typed numeric buffer in C backend"),
                    "C error must be a typed CompileError variant, got: {}",
                    err
                );
                rejected += 1;
            }
        }
    }
    println!("C backend: emitted={}, rejected={}", emitted, rejected);
    assert!(emitted > 0 && rejected > 0, "C backend must both emit and reject some variants");
}

#[test]
fn x86_freestanding_explicitly_classifies_every_ir_variant() {
    let mut emitted = 0;
    let mut rejected = 0;

    for ir in all_ir() {
        let classified = classify_x86(&ir);
        match classified {
            Classified::Emitted => emitted += 1,
            Classified::RejectedTypedError(err) => {
                assert!(
                    err.contains("UnsupportedVariant") ||
                    err.contains("InvalidArity") ||
                    err.contains("FixnumOutOfRange") ||
                    err.contains("TooManySymbols") ||
                    err.contains("unsupported IR in x86_64-freestanding backend"),
                    "x86 error must be a typed CompileError variant, got: {}",
                    err
                );
                rejected += 1;
            }
        }
    }
    println!("x86 freestanding: emitted={}, rejected={}", emitted, rejected);
    assert!(emitted > 0 && rejected > 0, "x86 must both emit and reject some variants");
}