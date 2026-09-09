use cml::compiler::Compiler;
use cml::c_backend::CBackend;
use cml::lower;
use cml::macros::MacroExpander;
use cml::parser;
use cml::x86_freestanding::X86FreestandingBackend;
use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: cml <file.my> | cml x86-asm <file.wsm> | cml x86-elf <file.wsm> <output>");
        std::process::exit(1);
    }

    if args[1] == "x86-asm" || args[1] == "x86-elf" {
        if args.len() < 3 {
            eprintln!("Usage: cml <file.my> | cml x86-asm <file.wsm> | cml x86-elf <file.wsm> <output>");
            std::process::exit(1);
        }
        run_x86(&args);
        return;
    }

    let filename = &args[1];
    let contents = fs::read_to_string(filename).unwrap_or_else(|err| {
        eprintln!("Error reading file {}: {}", filename, err);
        std::process::exit(1);
    });

    let exprs = match parser::parse(&contents) {
        Ok(e) => e,
        Err(err) => {
            eprintln!("{err}");
            std::process::exit(1);
        }
    };
    let exprs = MacroExpander::new().process(&exprs).unwrap_or_else(|err| {
        eprintln!("Macro expansion error: {err}");
        std::process::exit(1);
    });
    let program = lower::lower_program(&exprs).unwrap_or_else(|err| {
        eprintln!("Lowering error: {}", err);
        std::process::exit(1);
    });

    if args.get(2).map(|s| s.as_str()) == Some("--c") {
        match CBackend::new().compile_program(&program) {
            Ok(code) => println!("{}", code),
            Err(err) => {
                eprintln!("C backend error: {err}");
                std::process::exit(1);
            }
        }
        return;
    }

    match Compiler::new().compile(&program) {
        Ok(asm) => println!("{}", asm),
        Err(err) => {
            eprintln!("Compile error: {err}");
            std::process::exit(1);
        }
    }
}

fn run_x86(args: &[String]) {
    let mode = &args[1];
    let filename = &args[2];
    let contents = fs::read_to_string(filename).unwrap_or_else(|err| {
        eprintln!("Error reading file {}: {}", filename, err);
        std::process::exit(1);
    });
    let exprs = match parser::parse(&contents) {
        Ok(e) => e,
        Err(err) => {
            eprintln!("{err}");
            std::process::exit(1);
        }
    };
    let exprs = MacroExpander::new().process(&exprs).unwrap_or_else(|err| {
        eprintln!("Macro expansion error: {err}");
        std::process::exit(1);
    });
    let program = lower::lower_program_with_tail_calls(&exprs).unwrap_or_else(|err| {
        eprintln!("Lowering error: {}", err);
        std::process::exit(1);
    });
    let mut backend = X86FreestandingBackend::new();
    let asm = backend.compile_program(&program).unwrap_or_else(|err| {
        eprintln!("x86 freestanding compile error: {err}");
        std::process::exit(1);
    });
    if mode == "x86-asm" {
        print!("{}", asm);
        return;
    }
    // x86-elf path continues in full main — see repo
    let _ = Path::new(filename);
    let _ = Command::new("true");
    println!("{}", asm);
}
