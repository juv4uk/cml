use cml::parser;
use cml::compiler::Compiler;
use cml::macros::MacroExpander;
use std::env;
use std::fs;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: cml <file.my>");
        std::process::exit(1);
    }
    
    let filename = &args[1];
    let contents = fs::read_to_string(filename)
        .unwrap_or_else(|err| {
            eprintln!("Error reading file {}: {}", filename, err);
            std::process::exit(1);
        });
        
    let exprs = parser::parse(&contents).unwrap();
    let exprs = MacroExpander::new().process(&exprs);
    let mut compiler = Compiler::new();
    let asm = compiler.compile(&exprs);
    
    println!("{}", asm);
}
