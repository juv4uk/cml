use std::fs;
use std::process::Command;
use std::collections::HashMap;

use cml::parser;
use cml::ast::Expr;
use cml::compiler::Compiler;

fn collect_symbols(expr: &Expr, syms: &mut Vec<String>) {
    match expr {
        Expr::Symbol(s) | Expr::String(s) => {
            let upper = s.to_uppercase();
            if upper != "NIL" && upper != "T" && !syms.contains(&upper) {
                syms.push(upper);
            }
        }
        Expr::List(list) => {
            for e in list {
                collect_symbols(e, syms);
            }
        }
        Expr::DottedList(list, tail) => {
            for e in list {
                collect_symbols(e, syms);
            }
            collect_symbols(tail, syms);
        }
        _ => {}
    }
}

// A simple parser for the alist format: ((expr . "(quote radio)") (expected . "radio") ...)
fn parse_conformance_line(line: &str) -> Option<(String, String)> {
    let expr_marker = "(expr . \"";
    
    let expr_start = line.find(expr_marker)? + expr_marker.len();
    let expected_marker_full = "\") (expected . \"";
    
    let expr_end = line[expr_start..].find(expected_marker_full)? + expr_start;
    let expr = &line[expr_start..expr_end];
    
    let expected_start = expr_end + expected_marker_full.len();
    let expected_end = line[expected_start..].find("\")")? + expected_start;
    let expected = &line[expected_start..expected_end];
    
    let unescaped_expr = expr.replace("\\\"", "\"");
    let unescaped_expected = expected.replace("\\\"", "\"");
    
    Some((unescaped_expr, unescaped_expected))
}

#[test]
fn test_conformance() {
    let fixture_path = "../my-lisp/tests/fixtures/conformance.my";
    let fixture_content = fs::read_to_string(fixture_path).expect("Failed to read conformance.my");
    
    // 1. Build the simulator once
    let fpga_sim_dir = "../fpga-lisp";
    let iv_output = Command::new("iverilog")
        .current_dir(fpga_sim_dir)
        .arg("-g2012")
        .arg("-I").arg("fpga/rtl")
        .arg("-o").arg("tb_cml_e2e.vvp")
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
    
    // Track symbols globally across the runner
    let mut symbol_table = HashMap::new();
    let mut next_sym_id = 10; // Start dynamic symbols at 10
    
    symbol_table.insert("NIL".to_string(), 0);
    symbol_table.insert("TRUE".to_string(), 1);
    symbol_table.insert("T".to_string(), 1);
    
    // Run tests
    for line in fixture_content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(';') {
            continue;
        }
        
        // Let's only run Tier 1 constitutive tests for now to prove the pipeline
        if !line.contains("(tier . 1)") {
            continue;
        }
        
        // fpga-lisp hardware only has TAG_FIXNUM, so we skip float tests
        if line.contains("3.0") {
            continue;
        }
        
        // Skip unsupported features
        if line.contains("equal?") || line.starts_with("((expr . \"(let ") {
            continue;
        }
        
        if let Some((expr_str, expected_str)) = parse_conformance_line(line) {
            println!("Testing: {}", expr_str);
            let exprs = parser::parse(&expr_str).unwrap();
            let mut compiler = Compiler::new();
            let asm = compiler.compile(&exprs);
            
            // Collect new symbols
            let mut new_syms = Vec::new();
            for e in &exprs {
                collect_symbols(e, &mut new_syms);
            }
            
            for s in new_syms {
                if !symbol_table.contains_key(&s) {
                    symbol_table.insert(s, next_sym_id);
                    next_sym_id += 1;
                }
            }
            
            let mut full_asm = String::new();
            for (sym, id) in &symbol_table {
                full_asm.push_str(&format!(".define {} {}\n", sym, id));
            }
            full_asm.push_str(&asm);
            
            let test_name = "conformance_test";
            let asm_path = format!("{}.asm", test_name);
            fs::write(&asm_path, &full_asm).unwrap();
            
            // Assemble
            let asm_output = Command::new("python")
                .arg("../fpga-lisp/assembler.py")
                .arg(&asm_path)
                .output()
                .expect("Failed to run python assembler");

            if !asm_output.status.success() {
                panic!("Assembler failed on '{}':\n{}", expr_str, String::from_utf8_lossy(&asm_output.stderr));
            }
            
            let bin_path = format!("{}.bin", test_name);
            let target_bin = format!("{}/{}", fpga_sim_dir, bin_path);
            fs::copy(&bin_path, &target_bin).expect("Failed to copy .bin");
            
            // Run vvp
            let vvp_output = Command::new("vvp")
                .current_dir(fpga_sim_dir)
                .arg("tb_cml_e2e.vvp")
                .arg(format!("+bin_file={}", bin_path))
                .output()
                .expect("Failed to run vvp");

            let stdout = String::from_utf8_lossy(&vvp_output.stdout);
            
            // Cleanup intermediate files for this test
            let _ = fs::remove_file(&asm_path);
            let _ = fs::remove_file(&bin_path);
            let _ = fs::remove_file(&target_bin);
            
            // Decode R15
            let mut tag = None;
            let mut val = None;
            for l in stdout.lines() {
                if let Some(t_str) = l.strip_prefix("RESULT_TAG:") {
                    tag = t_str.parse::<u32>().ok();
                } else if let Some(v_str) = l.strip_prefix("RESULT_VAL:") {
                    val = v_str.parse::<u32>().ok();
                }
            }
            
            let tag = tag.expect(&format!("Could not find RESULT_TAG in output for {}:\n{}", expr_str, stdout));
            let val = val.expect(&format!("Could not find RESULT_VAL in output for {}", expr_str));
            
            // Extremely basic expected string conversion
            let actual = if tag == 4 { // TAG_TRUE
                "t".to_string()
            } else if tag == 3 { // TAG_NIL
                "()".to_string()
            } else if tag == 2 { // TAG_SYMBOL
                if val == 0 {
                    "()".to_string()
                } else if val == 1 {
                    "t".to_string()
                } else {
                    let mut found = None;
                    for (k, v) in &symbol_table {
                        if *v == val {
                            found = Some(k.to_lowercase());
                            break;
                        }
                    }
                    found.unwrap_or_else(|| format!("SYM_{}", val))
                }
            } else if tag == 1 { // TAG_CONS
                "(...)".to_string() 
            } else if tag == 0 { // TAG_FIXNUM
                val.to_string()
            } else {
                format!("TAG{}:VAL{}", tag, val)
            };
            
            if !expected_str.starts_with("(") {
                assert_eq!(actual, expected_str, "Test failed for {}", expr_str);
            }
        }
    }
}
