use cml::c_backend::CBackend;
use cml::compiler::Compiler;
use cml::ir::{BufferLiteral, Ir, Params, PrimOp, Quoted};
use cml::x86_freestanding::X86FreestandingBackend;
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
