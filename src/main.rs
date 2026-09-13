use cml::build::{self, BuildOptions};
use cml::compiler::{CompiledAssembly, Compiler};
use cml::lower;
use cml::macros::MacroExpander;
use cml::parser;
use cml::x86_freestanding::X86FreestandingBackend;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn usage() -> ! {
    eprintln!(
        "Usage:\n  cml <file.lisp>                  emit fpga-lisp assembly\n  cml build <file.lisp> [-o out] [--keep-c]\n                                   COMPILER-01: C backend \u{2192} native executable\n  cml x86-asm <file.lisp>          emit x86 freestanding assembly\n  cml x86-elf <file.lisp> <output> link x86 freestanding ELF"
    );
    std::process::exit(1);
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        usage();
    }

    // COMPILER-01: cml build <file.my> [-o out] [--keep-c]
    if args.get(1).map(|s| s.as_str()) == Some("build") {
        run_build(&args[2..]);
        return;
    }

    let (x86_asm, x86_elf, filename, output) = match args.as_slice() {
        [_, command, filename, output] if command == "x86-elf" => {
            (false, true, filename, Some(output))
        }
        [_, command, filename] if command == "x86-asm" => (true, false, filename, None),
        [_, filename] => (false, false, filename, None),
        _ => usage(),
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

fn run_build(args: &[String]) {
    let mut file: Option<&str> = None;
    let mut output = PathBuf::from("a.out");
    let mut keep_c = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-o" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("build: -o requires a path");
                    std::process::exit(1);
                }
                output = PathBuf::from(&args[i]);
            }
            "--keep-c" => keep_c = true,
            "-h" | "--help" => usage(),
            flag if flag.starts_with('-') => {
                eprintln!("build: unknown flag {flag}");
                usage();
            }
            path => {
                if file.is_some() {
                    eprintln!("build: unexpected extra argument {path}");
                    usage();
                }
                file = Some(path);
            }
        }
        i += 1;
    }
    let file = file.unwrap_or_else(|| {
        eprintln!("build: missing <file.my>");
        usage();
    });
    let opts = BuildOptions {
        output: output.clone(),
        keep_c,
        c_path: None,
    };
    if let Err(err) = build::build_file(std::path::Path::new(file), &opts) {
        eprintln!("{err}");
        std::process::exit(1);
    }
    eprintln!("wrote {}", output.display());
}

fn link_x86_elf(assembly: &str, output: &str) {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let base = env::temp_dir().join(format!("cml-x86-elf-{}-{nonce}", std::process::id()));
    let source = base.with_extension("s");
    let launcher = base.with_extension("c");
    fs::write(&source, assembly).unwrap_or_else(|err| fatal(&format!("writing assembly: {err}")));
    fs::write(
        &launcher,
        "#include <stdint.h>\nextern uint64_t wsm_entry(void *);\nint main(void) { (void)wsm_entry(0); return 0; }\n",
    )
    .unwrap_or_else(|err| fatal(&format!("writing launcher: {err}")));
    let nucleus_path =
        cml::x86_freestanding::resolve_nucleus_asm_path().unwrap_or_else(|err| fatal(&err));
    let linked = Command::new("cc")
        .arg(&launcher)
        .arg(&source)
        .arg(&nucleus_path)
        .arg("-o")
        .arg(output)
        .output()
        .unwrap_or_else(|err| fatal(&format!("starting linker: {err}")));
    let _ = fs::remove_file(source);
    let _ = fs::remove_file(launcher);
    if !linked.status.success() {
        fatal(&format!(
            "x86 ELF link failed: {}",
            String::from_utf8_lossy(&linked.stderr)
        ));
    }
}

fn fatal(message: &str) -> ! {
    eprintln!("{message}");
    std::process::exit(1)
}
