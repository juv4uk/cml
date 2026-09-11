use cml::c_backend::{CBackend, CompileError as CCompileError};
use cml::compiler::{CompileError as FpgaCompileError, Compiler};
use cml::ir::{BufferLiteral, Ir, Params, PrimOp, Quoted};
use cml::x86_freestanding::{CompileError as X86CompileError, X86FreestandingBackend};
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

#[derive(Debug)]
enum Classified<E> {
    Emitted,
    Rejected(E),
}

fn classify_fpga(ir: &Ir) -> Classified<FpgaCompileError> {
    let result = panic::catch_unwind(|| {
        let mut compiler = Compiler::new();
        compiler.compile(&[ir.clone()])
    });
    match result {
        Ok(Ok(_)) => Classified::Emitted,
        Ok(Err(error)) => Classified::Rejected(error),
        Err(_) => panic!("FPGA backend panicked on IR: {ir:?}"),
    }
}

fn classify_c(ir: &Ir) -> Classified<CCompileError> {
    let result = panic::catch_unwind(|| {
        let mut backend = CBackend::new();
        backend.compile_program(&[ir.clone()])
    });
    match result {
        Ok(Ok(_)) => Classified::Emitted,
        Ok(Err(error)) => Classified::Rejected(error),
        Err(_) => panic!("C backend panicked on IR: {ir:?}"),
    }
}

fn classify_x86(ir: &Ir) -> Classified<X86CompileError> {
    let result = panic::catch_unwind(|| {
        let backend = X86FreestandingBackend::new();
        backend.compile_program(&[ir.clone()])
    });
    match result {
        Ok(Ok(_)) => Classified::Emitted,
        Ok(Err(error)) => Classified::Rejected(error),
        Err(_) => panic!("x86_freestanding backend panicked on IR: {ir:?}"),
    }
}

#[test]
fn fpga_backend_exhaustively_classifies_all_ir_without_panicking() {
    for ir in all_ir() {
        let result = panic::catch_unwind(|| {
            let mut compiler = Compiler::new();
            let _ = compiler.compile(&[ir.clone()]);
        });
        assert!(result.is_ok(), "FPGA backend panicked on IR: {ir:?}");
    }
}

#[test]
fn c_backend_exhaustively_classifies_all_ir_without_panicking() {
    for ir in all_ir() {
        let result = panic::catch_unwind(|| {
            let mut backend = CBackend::new();
            let _ = backend.compile_program(&[ir.clone()]);
        });
        assert!(result.is_ok(), "C backend panicked on IR: {ir:?}");
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
            "x86_freestanding backend panicked on IR: {ir:?}"
        );
    }
}

#[test]
fn fpga_backend_explicitly_classifies_every_ir_variant() {
    let mut emitted = 0;
    let mut rejected = 0;

    for ir in all_ir() {
        match classify_fpga(&ir) {
            Classified::Emitted => emitted += 1,
            Classified::Rejected(error) => {
                // Тип є доказом: не прив'язуємо контракт тесту до тексту Display.
                match error {
                    FpgaCompileError::TooManyArguments { .. }
                    | FpgaCompileError::IntegerOutOfRange { .. }
                    | FpgaCompileError::UnsupportedNumericBuffer
                    | FpgaCompileError::SymbolTableOverflow
                    | FpgaCompileError::UnsupportedVariant(_) => {}
                }
                rejected += 1;
            }
        }
    }
    println!("FPGA: emitted={emitted}, rejected={rejected}");
    assert!(
        emitted > 0 && rejected > 0,
        "FPGA must both emit and reject some variants"
    );
}

#[test]
fn c_backend_explicitly_classifies_every_ir_variant() {
    let mut emitted = 0;
    let mut rejected = 0;

    for ir in all_ir() {
        match classify_c(&ir) {
            Classified::Emitted => emitted += 1,
            Classified::Rejected(error) => {
                match error {
                    CCompileError::NestedDef
                    | CCompileError::UnsupportedTypedBuffer
                    | CCompileError::UnsupportedVariant(_) => {}
                }
                rejected += 1;
            }
        }
    }
    println!("C backend: emitted={emitted}, rejected={rejected}");
    assert!(
        emitted > 0 && rejected > 0,
        "C backend must both emit and reject some variants"
    );
}

#[test]
fn x86_freestanding_explicitly_classifies_every_ir_variant() {
    let mut emitted = 0;
    let mut rejected = 0;

    for ir in all_ir() {
        match classify_x86(&ir) {
            Classified::Emitted => emitted += 1,
            Classified::Rejected(error) => {
                match error {
                    X86CompileError::EmptyProgram
                    | X86CompileError::UnsupportedVariant(_)
                    | X86CompileError::InvalidArity { .. }
                    | X86CompileError::DefArityMismatch { .. }
                    | X86CompileError::FixnumOutOfRange(_)
                    | X86CompileError::TooManySymbols => {}
                }
                rejected += 1;
            }
        }
    }
    println!("x86 freestanding: emitted={emitted}, rejected={rejected}");
    assert!(
        emitted > 0 && rejected > 0,
        "x86 must both emit and reject some variants"
    );
}
