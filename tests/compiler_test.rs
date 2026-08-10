use cml::parser;
use cml::compiler::Compiler;
use std::fs;
use std::process::Command;

fn run_assembler(asm_code: &str, test_name: &str) {
    let mut full_asm = String::new();
    // Prepend some basic symbol definitions that cml expects
    full_asm.push_str(".define NIL 0\n");
    full_asm.push_str(".define TRUE 1\n");
    full_asm.push_str(".define A 2\n");
    full_asm.push_str(".define B 3\n");
    full_asm.push_str(".define X 4\n");
    full_asm.push_str(".define Y 5\n");
    full_asm.push_str(".define SUCCESS 6\n");
    full_asm.push_str(".define TEST 7\n");
    full_asm.push_str(".define C 8\n");
    full_asm.push_str(".define D 9\n");
    full_asm.push_str(asm_code);

    let asm_path = format!("{}.asm", test_name);
    fs::write(&asm_path, full_asm).unwrap();

    let output = Command::new("python")
        .arg("../fpga-lisp/assembler.py")
        .arg(&asm_path)
        .output();

    if let Ok(output) = output {
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            panic!("Assembler failed for {}:\nSTDOUT: {}\nSTDERR: {}", test_name, stdout, stderr);
        }
    } else {
        panic!("Failed to run python. Is assembler.py at ../fpga-lisp/assembler.py?");
    }

    // Clean up
    let bin_path = format!("{}.bin", test_name);
    let _ = fs::remove_file(asm_path);
    let _ = fs::remove_file(bin_path);
}

#[test]
fn test_compile_cond() {
    let code = "(cond (t 'a) (nil 'b))";
    let exprs = parser::parse(code).unwrap();
    let mut compiler = Compiler::new();
    let asm = compiler.compile(&exprs);
    
    assert!(asm.contains("LOADSYM R1 TRUE"));
    assert!(asm.contains("JF R1"));
    assert!(asm.contains("LOADSYM R15 A"));
    assert!(asm.contains("LOADSYM R1 NIL"));
    assert!(asm.contains("LOADSYM R15 B"));
    assert!(asm.contains("HALT"));

    run_assembler(&asm, "test_cond");
}

#[test]
fn test_compile_lambda() {
    let code = "(lambda (x) x)";
    let exprs = parser::parse(code).unwrap();
    let mut compiler = Compiler::new();
    let asm = compiler.compile(&exprs);
    
    assert!(asm.contains("LAMBDA START"));
    assert!(asm.contains("LAMBDA END"));
    assert!(asm.contains("CONS R15 R15 R4")); // Closure building
    assert!(asm.contains("RET R14")); // Return

    run_assembler(&asm, "test_lambda");
}

#[test]
fn test_compile_apply() {
    let code = "((lambda (x) x) 'test)";
    let exprs = parser::parse(code).unwrap();
    let mut compiler = Compiler::new();
    let asm = compiler.compile(&exprs);
    
    assert!(asm.contains("CALL START"));
    assert!(asm.contains("CALL END"));
    assert!(asm.contains("CAR R10 R15"));
    assert!(asm.contains("CDR R4 R15"));
    assert!(asm.contains("RET R10"));

    run_assembler(&asm, "test_apply");
}

#[test]
fn test_compile_nested_apply() {
    let code = "((lambda (x y) (cond (x y) (nil nil))) t 'success)";
    let exprs = parser::parse(code).unwrap();
    let mut compiler = Compiler::new();
    let asm = compiler.compile(&exprs);

    run_assembler(&asm, "test_nested_apply");
}

#[test]
fn test_compile_quoted_list() {
    let code = "'(a (b c) d)";
    let exprs = parser::parse(code).unwrap();
    let mut compiler = Compiler::new();
    let asm = compiler.compile(&exprs);

    assert!(asm.contains("LOADSYM R15 D"));
    assert!(asm.contains("LOADSYM R15 C"));
    assert!(asm.contains("LOADSYM R15 B"));
    assert!(asm.contains("LOADSYM R15 A"));
    assert!(asm.contains("CONS R15 R15 R12"));
    
    run_assembler(&asm, "test_quoted_list");
}
