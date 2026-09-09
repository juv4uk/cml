use cml::compiler::{CompiledAssembly, Compiler};
use cml::lower;
use cml::macros::MacroExpander;
use cml::parser;
use cml::x86_freestanding::X86FreestandingBackend;
use std::env;
use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: cml <file.my> | cml x86-asm <file.wsm> | cml x86-elf <file.wsm> <output>");
        std::process::exit(1);
    }

    let (x86_asm, x86_elf, filename, output) = match args.as_slice() {
        [_, command, filename, output] if command == "x86-elf" => (false, true, filename, Some(output)),
        [_, command, filename] if command == "x86-asm" => (true, false, filename, None),
        [_, filename] => (false, false, filename, None),
        _ => {
            eprintln!("Usage: cml <file.my> | cml x86-asm <file.wsm> | cml x86-elf <file.wsm> <output>");
            std::process::exit(1);
        }
    };
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
    if x86_asm || x86_elf {
        let assembly = X86FreestandingBackend::new()
            .compile_program(&program)
            .unwrap_or_else(|err| {
                eprintln!("x86 freestanding compile error: {err}");
                std::process::exit(1);
            });
        if let Some(output) = output {
            link_x86_elf(&assembly, output);
        } else {
            print!("{assembly}");
        }
        return;
    }
    let mut compiler = Compiler::new();
    // M1.1d bridge (LOADSYM contract, F6): emit numeric tagged-symbol
    // immediates + per-program symbol table, per the directive that
    // computational transforms live in CML while the fpga-lisp assembler
    // stays a numeric reference. The legacy name-oriented compile remains
    // available in the library for readable diagnostics.
    let compiled: CompiledAssembly =
        compiler
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

fn link_x86_elf(assembly: &str, output: &str) {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let base = env::temp_dir().join(format!("cml-x86-elf-{}-{nonce}", std::process::id()));
    let source = base.with_extension("s");
    let launcher = base.with_extension("c");
    fs::write(&source, assembly).unwrap_or_else(|err| fatal(&format!("writing assembly: {err}")));
    fs::write(&launcher, "#include <stdint.h>\nextern uint64_t wsm_entry(void *);\nint main(void) { (void)wsm_entry(0); return 0; }\n")
        .unwrap_or_else(|err| fatal(&format!("writing launcher: {err}")));
    let linked = Command::new("cc")
        .arg(&launcher).arg(&source)
        .arg("/home/agents/GitHub/wsm-my-lisp/asm/nucleus.s")
        .arg("-o").arg(output).output()
        .unwrap_or_else(|err| fatal(&format!("starting linker: {err}")));
    let _ = fs::remove_file(source);
    let _ = fs::remove_file(launcher);
    if !linked.status.success() {
        fatal(&format!("x86 ELF link failed: {}", String::from_utf8_lossy(&linked.stderr)));
    }
}

fn fatal(message: &str) -> ! { eprintln!("{message}"); std::process::exit(1) }
