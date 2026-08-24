use cml::compiler::{Compiler, CompiledAssembly};
use cml::lower;
use cml::macros::MacroExpander;
use cml::parser;
use std::env;
use std::fs;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: cml <file.my>");
        std::process::exit(1);
    }

    let filename = &args[1];
    let contents = fs::read_to_string(filename).unwrap_or_else(|err| {
        eprintln!("Error reading file {}: {}", filename, err);
        std::process::exit(1);
    });

    let exprs = parser::parse(&contents).unwrap();
    let exprs = MacroExpander::new().process(&exprs).unwrap_or_else(|err| {
        eprintln!("Macro expansion error: {err}");
        std::process::exit(1);
    });
    let program = lower::lower_program(&exprs).unwrap_or_else(|err| {
        eprintln!("Lowering error: {}", err);
        std::process::exit(1);
    });
    let mut compiler = Compiler::new();
    // M1.1d bridge (LOADSYM contract, F6): emit numeric tagged-symbol
    // immediates + per-program symbol table, per the directive that
    // computational transforms live in CML while the fpga-lisp assembler
    // stays a numeric reference. The legacy name-oriented compile remains
    // available in the library for readable diagnostics.
    let compiled: CompiledAssembly = compiler
        .compile_with_symbols(&program)
        .unwrap_or_else(|err| {
            eprintln!("Compile error: {err}");
            std::process::exit(1);
        });

    println!("{}", compiled.assembly);
    if !compiled.symbols.is_empty() {
        println!("; SYMBOL TABLE (id name) — LOADSYM immediates above");
        for (id, name) in &compiled.symbols {
            println!("; SYM {id} {name}");
        }
    }
}
