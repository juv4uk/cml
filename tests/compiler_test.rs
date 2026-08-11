use cml::parser;
use cml::compiler::Compiler;
use std::env;
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

    let output = Command::new("python3")
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
    
    // `t` is constructed through ATOM, matching the current target's TRUE
    // representation; the old test still expected the pre-truthiness-fix
    // `LOADSYM R1 TRUE` sequence.
    // `t` будується через ATOM; старий тест очікував код до truthiness-фіксу.
    // `t` wird über ATOM gebaut; der alte Test erwartete Code vor dem Fix.
    assert!(asm.contains("ATOM R1 R1"));
    assert!(asm.contains("JF R3"));
    assert!(asm.contains("LOADSYM R15 A"));
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
fn test_compile_let_as_lambda_application() {
    let exprs = parser::parse("(let ((x 'test)) x)").unwrap();
    let mut compiler = Compiler::new();
    let asm = compiler.compile(&exprs);

    assert!(asm.contains("; LAMBDA START"));
    assert!(asm.contains("LOADSYM R12 X"));
    assert!(asm.contains("LOADSYM R1 TEST"));
    run_assembler(&asm, "test_let");
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

#[test]
fn test_end_to_end_execution() {
    let code = "((lambda (x) x) 'test)";
    let exprs = parser::parse(code).unwrap();
    let mut compiler = Compiler::new();
    let asm = compiler.compile(&exprs);

    let mut full_asm = String::new();
    // Prepend basic symbols
    full_asm.push_str(".define NIL 0\n");
    full_asm.push_str(".define TRUE 1\n");
    full_asm.push_str(".define X 4\n");
    full_asm.push_str(".define TEST 7\n");
    full_asm.push_str(&asm);

    let test_name = "cml_e2e";
    let asm_path = format!("{}.asm", test_name);
    fs::write(&asm_path, full_asm).unwrap();

    // 1. Assemble to cml_e2e.bin
    let asm_output = Command::new("python3")
        .arg("../fpga-lisp/assembler.py")
        .arg(&asm_path)
        .output()
        .expect("Failed to run python assembler");

    if !asm_output.status.success() {
        let stderr = String::from_utf8_lossy(&asm_output.stderr);
        let stdout = String::from_utf8_lossy(&asm_output.stdout);
        panic!("Assembler failed:\nSTDOUT: {}\nSTDERR: {}", stdout, stderr);
    }

    // 2. Compile simulation testbench with iverilog. Sources are read from
    // ../fpga-lisp (current_dir), but the compiled .vvp is written back into
    // this crate's own directory via an absolute -o path, so nothing is
    // written under the sibling repo (its own WSL/Guix user may not have
    // write access there).
    let cwd = env::current_dir().unwrap();
    let bin_path = format!("{}.bin", test_name);
    let bin_abs = cwd.join(&bin_path);
    let vvp_path = format!("{}.vvp", test_name);
    let vvp_abs = cwd.join(&vvp_path);

    let fpga_sim_dir = "../fpga-lisp";
    let iv_output = Command::new("iverilog")
        .current_dir(fpga_sim_dir)
        .arg("-g2012")
        .arg("-I").arg("fpga/rtl")
        .arg("-o").arg(&vvp_abs)
        .arg("fpga/rtl/lisp_word.sv")
        .arg("fpga/rtl/heap.sv")
        .arg("fpga/rtl/lisp_data_unit.sv")
        .arg("fpga/rtl/registers.sv")
        .arg("fpga/rtl/instruction_decoder.sv")
        .arg("fpga/rtl/control.sv")
        .arg("fpga/rtl/uart.sv")
        .arg("fpga/rtl/bootloader.sv")
        .arg("fpga/rtl/lisp_machine.sv")
        .arg("fpga/sim/tb_cml_e2e.sv")
        .output()
        .expect("Failed to run iverilog");

    if !iv_output.status.success() {
        let stderr = String::from_utf8_lossy(&iv_output.stderr);
        let stdout = String::from_utf8_lossy(&iv_output.stdout);
        panic!("Icarus Verilog compilation failed:\nSTDOUT: {}\nSTDERR: {}", stdout, stderr);
    }

    // 3. Run simulation with vvp, pointing it at the .bin via the testbench's
    // +bin_file= plusarg instead of copying the .bin into the sibling repo.
    let vvp_output = Command::new("vvp")
        .arg(&vvp_abs)
        .arg(format!("+bin_file={}", bin_abs.display()))
        .output()
        .expect("Failed to run vvp");

    let stdout = String::from_utf8_lossy(&vvp_output.stdout);

    // Clean up
    let _ = fs::remove_file(asm_path);
    let _ = fs::remove_file(&bin_path);
    let _ = fs::remove_file(&vvp_path);

    if !stdout.contains("CML E2E PASSED") {
        panic!("E2E Simulation failed or did not print PASSED.\nSTDOUT:\n{}", stdout);
    }
}
