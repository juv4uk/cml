pub mod ast;
pub mod parser;
pub mod compiler;

use std::env;
use std::fs;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: cml <file.my>");
        std::process::exit(1);
    }

    let filename = &args[1];
    let code = match fs::read_to_string(filename) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error reading file {}: {}", filename, e);
            std::process::exit(1);
        }
    };

    match parser::parse(&code) {
        Ok(exprs) => {
            let mut comp = compiler::Compiler::new();
            let asm = comp.compile(&exprs);
            println!("{}", asm);
        }
        Err(e) => {
            eprintln!("Parse error: {:?}", e);
        }
    }
}
