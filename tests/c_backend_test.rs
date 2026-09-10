// Proves ir::Ir is a real second-backend boundary, not just fpga-lisp-
// shaped by accident (docs/heterogeneous-backends.md step 2): compiles a
// def+lambda+primitive program through c_backend.rs, compiles the
// resulting C with a real `gcc`, runs it, and checks the printed result
// against the my-lisp reference value for the same source
// (`((lambda (x) (+ x 1)) 41)` -> `42`, verified live against the
// my-lisp CLI/oracle).
use std::fs;
use std::process::Command;

use cml::c_backend::CBackend;
use cml::compute::{ComputeBackend, CpuComputeBackend};
use cml::ir::{BufferLiteral, Ir, Params, PrimOp};
use cml::lower;
use cml::parser;

fn compile_and_run_first_class(code: &str, stem: &str) -> String {
    let exprs = parser::parse(code).unwrap();
    let program = lower::lower_program_with_first_class_builtins(&exprs).unwrap();
    let c_source = CBackend::new().compile_program(&program).unwrap();
    let c_path = format!("c_backend_{stem}_test.c");
    let bin_path = format!("c_backend_{stem}_test");
    fs::write(&c_path, &c_source).unwrap();

    let compile = Command::new("gcc")
        .arg(&c_path)
        .arg("-o")
        .arg(&bin_path)
        .output()
        .unwrap();
    if !compile.status.success() {
        panic!(
            "gcc failed:\nSTDERR: {}\n--- generated C ---\n{}",
            String::from_utf8_lossy(&compile.stderr),
            c_source
        );
    }
    let run = Command::new(format!("./{bin_path}")).output().unwrap();
    let _ = fs::remove_file(c_path);
    let _ = fs::remove_file(bin_path);
    assert!(run.status.success(), "compiled C program failed");
    String::from_utf8(run.stdout).unwrap().trim().to_lowercase()
}

fn compile_and_run_failure(code: &str, stem: &str) -> std::process::Output {
    let exprs = parser::parse(code).unwrap();
    let program = lower::lower_program_with_first_class_builtins(&exprs).unwrap();
    let c_source = CBackend::new().compile_program(&program).unwrap();
    let c_path = format!("c_backend_{stem}_test.c");
    let bin_path = format!("c_backend_{stem}_test");
    fs::write(&c_path, &c_source).unwrap();
    let compile = Command::new("gcc")
        .arg(&c_path)
        .arg("-o")
        .arg(&bin_path)
        .output()
        .unwrap();
    assert!(
        compile.status.success(),
        "gcc failed: {}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let run = Command::new(format!("./{bin_path}")).output().unwrap();
    let _ = fs::remove_file(c_path);
    let _ = fs::remove_file(bin_path);
    run
}

fn compile_ir_and_run(program: &[Ir], stem: &str) -> std::process::Output {
    let c_source = CBackend::new().compile_program(program).unwrap();
    let c_path = format!("c_backend_{stem}_test.c");
    let bin_path = format!("c_backend_{stem}_test");
    fs::write(&c_path, &c_source).unwrap();
    let compile = Command::new("gcc")
        .arg(&c_path)
        .arg("-o")
        .arg(&bin_path)
        .output()
        .unwrap();
    if !compile.status.success() {
        panic!(
            "gcc failed:\nSTDERR: {}\n--- generated C ---\n{}",
            String::from_utf8_lossy(&compile.stderr),
            c_source
        );
    }
    let run = Command::new(format!("./{bin_path}")).output().unwrap();
    let _ = fs::remove_file(c_path);
    let _ = fs::remove_file(bin_path);
    run
}

fn cpu_reference_i32_map(source: &str) -> String {
    let expressions = parser::parse(source).unwrap();
    let program = lower::lower_program(&expressions).unwrap();
    let buffer = CpuComputeBackend.execute(&program[0]).unwrap();
    match buffer {
        BufferLiteral::I32(values) => format!(
            "#i32({})",
            values
                .iter()
                .map(i32::to_string)
                .collect::<Vec<_>>()
                .join(" ")
        ),
        BufferLiteral::F32(_) => panic!("unexpected f32 reference result"),
    }
}

#[test]
fn c_backend_calls_a_builtin_stored_as_a_value() {
    assert_eq!(
        compile_and_run_first_class("(def f +) (f 20 22)", "builtin_value"),
        "42"
    );
}

#[test]
fn c_backend_lexically_shadows_a_builtin() {
    let code = "(let ((car (lambda (x) (quote shadowed)))) (car (quote (1 2))))";
    assert_eq!(
        compile_and_run_first_class(code, "builtin_shadow"),
        "shadowed"
    );
}

#[test]
fn c_backend_passes_a_builtin_as_a_higher_order_argument() {
    let code = "((lambda (f) (f 2 3)) +)";
    assert_eq!(
        compile_and_run_first_class(code, "builtin_higher_order"),
        "5"
    );
}

#[test]
fn c_backend_prints_the_contractual_builtin_representation() {
    assert_eq!(
        compile_and_run_first_class("+", "builtin_print"),
        "#<builtin +>"
    );
}

#[test]
fn c_backend_supports_first_class_subtraction() {
    assert_eq!(
        compile_and_run_first_class("-", "builtin_subtract_print"),
        "#<builtin ->"
    );
    assert_eq!(
        compile_and_run_first_class("(- 5)", "builtin_subtract_unary"),
        "-5"
    );
    assert_eq!(
        compile_and_run_first_class("(- 20 3 2)", "builtin_subtract_many"),
        "15"
    );
}

#[test]
fn c_backend_rejects_subtraction_without_arguments() {
    let run = compile_and_run_failure("(-)", "builtin_subtract_arity");
    assert!(String::from_utf8_lossy(&run.stderr).contains("Arity: -"));
}

#[test]
fn c_backend_rejects_a_non_callable_with_a_named_type_error() {
    let run = compile_and_run_failure("(42 1 2)", "non_callable");
    assert!(!run.status.success());
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(
        stderr.starts_with("NotCallable:") || stderr.starts_with("Type:"),
        "expected NotCallable: or Type:, got {stderr:?}"
    );
}

#[test]
fn c_backend_rejects_wrong_builtin_arity_with_a_named_error() {
    let run = compile_and_run_failure("(+ 1)", "builtin_arity");
    assert!(!run.status.success());
    assert!(String::from_utf8_lossy(&run.stderr).starts_with("Arity:"));
}

#[test]
fn c_backend_reports_contractual_core_error_kinds() {
    for (code, stem, kind) in [
        ("(car 5)", "error_car_int", "Type:"),
        ("(car (quote ()))", "error_car_nil", "Type:"),
        ("(eq (quote (1)) (quote (2)))", "error_eq_cons", "Type:"),
        (
            "(undefined-symbol)",
            "error_unknown_symbol",
            "UnknownSymbol:",
        ),
        ("(cons 1)", "error_cons_arity", "Arity:"),
        (
            "((lambda (a b . rest) a) 1)",
            "error_lambda_arity",
            "Arity:",
        ),
    ] {
        let run = compile_and_run_failure(code, stem);
        assert!(!run.status.success(), "{code} unexpectedly succeeded");
        assert!(
            String::from_utf8_lossy(&run.stderr).starts_with(kind),
            "{code}: expected {kind}, stderr was {:?}",
            String::from_utf8_lossy(&run.stderr)
        );
    }
}

#[test]
fn c_backend_supports_i32_buffers_and_rejects_f32_by_name() {
    assert_eq!(
        compile_and_run_first_class("#i32(1 -2 3)", "i32_buffer"),
        "#i32(1 -2 3)"
    );
    let i32_program =
        lower::lower_program_with_first_class_builtins(&parser::parse("#i32(1)").unwrap()).unwrap();
    let i32_c_source = CBackend::new().compile_program(&i32_program).unwrap();
    assert!(i32_c_source.contains("OutOfMemory"));
    assert!(i32_c_source.contains("checked_malloc"));
    let exprs = parser::parse("#f32(1.0 2.0)").unwrap();
    let program = lower::lower_program_with_first_class_builtins(&exprs).unwrap();
    assert!(matches!(
        CBackend::new().compile_program(&program),
        Err(cml::c_backend::CompileError::UnsupportedTypedBuffer)
    ));
}

#[test]
fn c_backend_executes_numeric_buffer_map_as_i32_reference_path() {
    let program = [Ir::App {
        func: Box::new(Ir::Var("NUMERIC-BUFFER-MAP".into())),
        args: vec![
            Ir::Lambda {
                params: Params::Fixed(vec!["x".into()]),
                body: Box::new(Ir::Prim {
                    op: PrimOp::Add,
                    args: vec![Ir::Var("x".into()), Ir::Int(1)],
                }),
            },
            Ir::Buffer(BufferLiteral::I32(vec![1, 2, 3])),
        ],
    }];
    let run = compile_ir_and_run(&program, "i32_map");
    assert!(run.status.success(), "numeric-buffer-map failed: {:?}", run);
    assert_eq!(String::from_utf8_lossy(&run.stdout).trim(), "#i32(2 3 4)");
}

#[test]
fn c_backend_lowers_source_level_numeric_buffer_map() {
    assert_eq!(
        compile_and_run_first_class(
            "(numeric-buffer-map (lambda (x) (+ x 1)) #i32(1 2 3))",
            "source_i32_map"
        ),
        "#i32(2 3 4)"
    );
}

#[test]
fn c_backend_i32_map_matches_cpu_reference_for_source_fixtures() {
    for (index, source) in [
        "(numeric-buffer-map (lambda (x) (+ x 1)) #i32(1 2 3))",
        "(numeric-buffer-map (lambda (x) (+ x -2)) #i32(-3 4))",
        "(numeric-buffer-map (lambda (x) (+ (+ x 10) -3)) #i32(0 7 -9))",
        "(numeric-buffer-map (lambda (x) (+ x 1)) #i32())",
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(
            compile_and_run_first_class(source, &format!("source_i32_differential_{index}")),
            cpu_reference_i32_map(source),
            "source: {source}"
        );
    }
}

#[test]
fn c_backend_numeric_buffer_map_fails_closed_on_i32_overflow() {
    let program = [Ir::App {
        func: Box::new(Ir::Var("NUMERIC-BUFFER-MAP".into())),
        args: vec![
            Ir::Lambda {
                params: Params::Fixed(vec!["x".into()]),
                body: Box::new(Ir::Prim {
                    op: PrimOp::Add,
                    args: vec![Ir::Var("x".into()), Ir::Int(1)],
                }),
            },
            Ir::Buffer(BufferLiteral::I32(vec![i32::MAX])),
        ],
    }];
    let run = compile_ir_and_run(&program, "i32_map_overflow");
    assert!(!run.status.success());
    assert!(String::from_utf8_lossy(&run.stderr).starts_with("NumericOverflow:"));
}

#[test]
fn compiles_add1_to_c_and_runs_it() {
    let code = "(def add1 (lambda (x) (+ x 1))) (add1 41)";
    let exprs = parser::parse(code).unwrap();
    let program = lower::lower_program_with_first_class_builtins(&exprs).unwrap();
    let mut backend = CBackend::new();
    let c_source = backend.compile_program(&program).unwrap();

    let c_path = "c_backend_add1_test.c";
    let bin_path = "c_backend_add1_test";
    fs::write(c_path, &c_source).unwrap();

    let compile = Command::new("gcc")
        .arg(c_path)
        .arg("-o")
        .arg(bin_path)
        .output()
        .expect("failed to run gcc -- is it in PATH? (see manifest.scm's gcc-toolchain)");
    if !compile.status.success() {
        panic!(
            "gcc failed:\nSTDOUT: {}\nSTDERR: {}\n--- generated C ---\n{}",
            String::from_utf8_lossy(&compile.stdout),
            String::from_utf8_lossy(&compile.stderr),
            c_source
        );
    }

    let run = Command::new(format!("./{bin_path}"))
        .output()
        .expect("failed to run compiled binary");
    let stdout = String::from_utf8_lossy(&run.stdout);

    let _ = fs::remove_file(c_path);
    let _ = fs::remove_file(bin_path);

    assert_eq!(
        stdout.trim(),
        "42",
        "expected 42 (matches my-lisp oracle for the same source), got: {stdout}"
    );
}

#[test]
fn compiles_self_recursive_def_to_c_and_runs_it() {
    // Same fixture used to root-cause the fpga-lisp backend's R4/ENV-
    // clobber bug (e73f93a) -- here to prove the C backend's independent
    // letrec-placeholder-plus-backpatch (compile_def in c_backend.rs)
    // gets self-recursion right too, not just fixed-arity application.
    let code = "(def count (lambda (n) (cond ((eq n 0) 99) (t (count (+ n -1)))))) (count 3)";
    let exprs = parser::parse(code).unwrap();
    let program = lower::lower_program_with_first_class_builtins(&exprs).unwrap();
    let mut backend = CBackend::new();
    let c_source = backend.compile_program(&program).unwrap();

    let c_path = "c_backend_count_test.c";
    let bin_path = "c_backend_count_test";
    fs::write(c_path, &c_source).unwrap();

    let compile = Command::new("gcc")
        .arg(c_path)
        .arg("-o")
        .arg(bin_path)
        .output()
        .unwrap();
    if !compile.status.success() {
        panic!(
            "gcc failed:\nSTDERR: {}\n--- generated C ---\n{}",
            String::from_utf8_lossy(&compile.stderr),
            c_source
        );
    }

    let run = Command::new(format!("./{bin_path}")).output().unwrap();
    let stdout = String::from_utf8_lossy(&run.stdout);

    let _ = fs::remove_file(c_path);
    let _ = fs::remove_file(bin_path);

    assert_eq!(
        stdout.trim(),
        "99",
        "expected 99 (matches my-lisp oracle: (count 3) -> 99), got: {stdout}"
    );
}

#[test]
fn compiles_let_to_c_and_runs_it() {
    // CML-C-BACKEND-LET: compile_expr's Ir::Let arm (derives let via an
    // immediately-applied lambda, same technique compiler.rs uses) has
    // never actually been run before this test.
    let code = "(let ((x 5) (y 3)) (+ x y))";
    let exprs = parser::parse(code).unwrap();
    let program = lower::lower_program_with_first_class_builtins(&exprs).unwrap();
    let mut backend = CBackend::new();
    let c_source = backend.compile_program(&program).unwrap();

    let c_path = "c_backend_let_test.c";
    let bin_path = "c_backend_let_test";
    fs::write(c_path, &c_source).unwrap();

    let compile = Command::new("gcc")
        .arg(c_path)
        .arg("-o")
        .arg(bin_path)
        .output()
        .unwrap();
    if !compile.status.success() {
        panic!(
            "gcc failed:\nSTDERR: {}\n--- generated C ---\n{}",
            String::from_utf8_lossy(&compile.stderr),
            c_source
        );
    }

    let run = Command::new(format!("./{bin_path}")).output().unwrap();
    let stdout = String::from_utf8_lossy(&run.stdout);

    let _ = fs::remove_file(c_path);
    let _ = fs::remove_file(bin_path);

    assert_eq!(
        stdout.trim(),
        "8",
        "expected 8 (matches my-lisp oracle: (let ((x 5) (y 3)) (+ x y)) -> 8), got: {stdout}"
    );
}

#[test]
fn compiles_variadic_and_dotted_lambda_params_to_c_and_runs_it() {
    // CML-C-BACKEND-VARIADIC: compile_lambda previously panicked on
    // Params::Variadic/AllRest.
    let code = "(cons (car ((lambda args args) 1 2 3)) (car ((lambda (a . rest) rest) 1 2 3)))";
    let exprs = parser::parse(code).unwrap();
    let program = lower::lower_program_with_first_class_builtins(&exprs).unwrap();
    let mut backend = CBackend::new();
    let c_source = backend.compile_program(&program).unwrap();

    let c_path = "c_backend_variadic_test.c";
    let bin_path = "c_backend_variadic_test";
    fs::write(c_path, &c_source).unwrap();

    let compile = Command::new("gcc")
        .arg(c_path)
        .arg("-o")
        .arg(bin_path)
        .output()
        .unwrap();
    if !compile.status.success() {
        panic!(
            "gcc failed:\nSTDERR: {}\n--- generated C ---\n{}",
            String::from_utf8_lossy(&compile.stderr),
            c_source
        );
    }

    let run = Command::new(format!("./{bin_path}")).output().unwrap();
    let stdout = String::from_utf8_lossy(&run.stdout);

    let _ = fs::remove_file(c_path);
    let _ = fs::remove_file(bin_path);

    assert_eq!(
        stdout.trim(),
        "(1 . 2)",
        "expected (1 . 2) (matches my-lisp oracle: car of bare-symbol-params args -> 1, car of dotted-rest -> 2), got: {stdout}"
    );
}

#[test]
fn compiles_quoted_list_access_to_c_and_runs_it() {
    // CML-C-BACKEND-QUOTED-LISTS: compile_quoted previously panicked on
    // Quoted::List/DottedList. car/(car (cdr ...)) into a quoted list
    // exercises the fix without depending on print_value's raw dotted-pair
    // format matching my-lisp's own list printer.
    let code = "(cons (car (quote (1 2 3))) (car (cdr (quote (1 2 3)))))";
    let exprs = parser::parse(code).unwrap();
    let program = lower::lower_program_with_first_class_builtins(&exprs).unwrap();
    let mut backend = CBackend::new();
    let c_source = backend.compile_program(&program).unwrap();

    let c_path = "c_backend_quoted_list_test.c";
    let bin_path = "c_backend_quoted_list_test";
    fs::write(c_path, &c_source).unwrap();

    let compile = Command::new("gcc")
        .arg(c_path)
        .arg("-o")
        .arg(bin_path)
        .output()
        .unwrap();
    if !compile.status.success() {
        panic!(
            "gcc failed:\nSTDERR: {}\n--- generated C ---\n{}",
            String::from_utf8_lossy(&compile.stderr),
            c_source
        );
    }

    let run = Command::new(format!("./{bin_path}")).output().unwrap();
    let stdout = String::from_utf8_lossy(&run.stdout);

    let _ = fs::remove_file(c_path);
    let _ = fs::remove_file(bin_path);

    // print_value renders a cons pair as "(car . cdr)"; car=1, cdr=2 here.
    assert_eq!(
        stdout.trim(),
        "(1 . 2)",
        "expected (1 . 2) (matches my-lisp oracle: car of '(1 2 3) -> 1, car of cdr -> 2), got: {stdout}"
    );
}

#[test]
fn nested_def_returns_graceful_error() {
    // CML-C-BACKEND-ERROR-HANDLING: a nested `def` must produce a
    // CompileError::NestedDef instead of panicking.
    let code = "(def x (def y 1))";
    let exprs = parser::parse(code).unwrap();
    let program = lower::lower_program_with_first_class_builtins(&exprs).unwrap();
    let mut backend = CBackend::new();
    let err = backend.compile_program(&program).unwrap_err();
    assert!(
        matches!(err, cml::c_backend::CompileError::NestedDef),
        "expected NestedDef error, got: {err}"
    );
}

// --- Macro expansion error regression tests (CML-C-BACKEND-ERROR-HANDLING) ---

#[test]
fn macro_unbound_symbol_returns_graceful_error() {
    use cml::macros::{MacroError, MacroExpander};
    // A bare symbol `bar` in the macro body (not in a list) that's unbound
    let code = "(defmacro foo (x) bar) (foo 1)";
    let exprs = parser::parse(code).unwrap();
    let err = MacroExpander::new().process(&exprs).unwrap_err();
    assert!(
        matches!(err, MacroError::UnboundSymbol(ref s) if s == "bar"),
        "expected UnboundSymbol(\"bar\"), got: {err}"
    );
}

#[test]
fn macro_expected_operator_returns_graceful_error() {
    use cml::macros::{MacroError, MacroExpander};
    // A macro body that's a non-symbol list head: ((1 2) x)
    let code = "(defmacro foo (x) ((1 2) x)) (foo 1)";
    let exprs = parser::parse(code).unwrap();
    let err = MacroExpander::new().process(&exprs).unwrap_err();
    assert!(
        matches!(err, MacroError::ExpectedOperator),
        "expected ExpectedOperator, got: {err}"
    );
}

#[test]
fn macro_unsupported_form_returns_graceful_error() {
    use cml::macros::{MacroError, MacroExpander};
    let code = "(defmacro foo (x) (unknown-form x)) (foo 1)";
    let exprs = parser::parse(code).unwrap();
    let err = MacroExpander::new().process(&exprs).unwrap_err();
    assert!(
        matches!(err, MacroError::UnsupportedForm(ref s) if s == "unknown-form"),
        "expected UnsupportedForm(\"unknown-form\"), got: {err}"
    );
}

// C4 exact-numeric: Ir::Rational lowered through the C backend, compiled
// with a real gcc and executed (hosted Linux process), checked against the
// my-lisp oracle's exact rational printer. These are all live-verified
// against `my-lisp` v0.34 (oracle) -- not asserted from memory.
#[test]
fn c_backend_lowers_rational_add_matches_oracle() {
    assert_eq!(compile_and_run_first_class("(+ 1/2 1/3)", "rational_add"), "5/6");
}

#[test]
fn c_backend_lowers_rational_sub_matches_oracle() {
    assert_eq!(compile_and_run_first_class("(- 1/2 1/3)", "rational_sub"), "1/6");
}

#[test]
fn c_backend_lowers_rational_mul_matches_oracle() {
    assert_eq!(compile_and_run_first_class("(* 1/2 1/3)", "rational_mul"), "1/6");
}

#[test]
fn c_backend_lowers_rational_div_matches_oracle() {
    assert_eq!(compile_and_run_first_class("(/ 1/2 1/3)", "rational_div"), "3/2");
}

#[test]
fn c_backend_lowers_rational_reduces_matches_oracle() {
    // (2/3 * 3/4) reduces to 1/2; (1/6 + 1/3) reduces to 1/2.
    assert_eq!(compile_and_run_first_class("(* 2/3 3/4)", "rational_reduce_mul"), "1/2");
    assert_eq!(compile_and_run_first_class("(+ 1/6 1/3)", "rational_reduce_add"), "1/2");
}

#[test]
fn c_backend_lowers_rational_int_mix_matches_oracle() {
    // Int operand participates through the exact to_rational path.
    assert_eq!(compile_and_run_first_class("(/ 3 2)", "rational_div_int"), "3/2");
    assert_eq!(compile_and_run_first_class("(+ 1/2 1)", "rational_add_int"), "3/2");
}

#[test]
fn c_backend_lowers_rational_unary_minus_matches_oracle() {
    assert_eq!(compile_and_run_first_class("(- 1/2)", "rational_unary_minus"), "-1/2");
}

#[test]
fn c_backend_true_is_an_ordinary_symbol_not_a_manufactured_tag() {
    // Regression for CML-C-BACKEND-TAG-TRUE-MANUFACTURED-PRIMITIVE
    // (2026-09-04): before the fix, the self-evaluating literal `t` and a
    // quoted symbol `(quote t)` carried different C runtime tags
    // (TAG_TRUE vs TAG_SYM), so `(eq t (quote t))` would have compared
    // unequal tags and returned () -- exactly the class of bug the
    // owner's paradigm forbids (a substrate inventing a primitive
    // category the language itself never asked for; t is plain
    // Symbol("t") in canonical WSM). Print, atom, and cross-representation
    // eq must all agree t is just an ordinary symbol.
    //
    // The test harness lowercases captured stdout before comparing, so
    // the expected strings below are lowercase regardless of the runtime's
    // own internal case convention -- but the *internal* comparison inside
    // the compiled program is case-sensitive strcmp, and that is where this
    // test genuinely caught a real bug while being written: mk_sym stores
    // canonical symbols uppercase (lower.rs uppercases every symbol name,
    // cml's own established convention, distinct from my-lisp's lowercase
    // display), so a first version of this fix using a lowercase "t"
    // literal for TRUE_V made `(eq t (quote t))` compile to
    // `strcmp("t", "T")` internally -- genuinely unequal, real () result,
    // not a test-harness artifact. Fixed by storing TRUE_V's symbol as
    // "T" to match mk_sym's own canonical case.
    assert_eq!(compile_and_run_first_class("t", "true_prints_as_t"), "t");
    assert_eq!(compile_and_run_first_class("(atom t)", "true_is_atom"), "t");
    assert_eq!(
        compile_and_run_first_class("(eq t (quote t))", "true_eq_quoted_symbol_t"),
        "t"
    );
}
