//! External CML compiler process boundary for host integrations such as my-idea.
//!
//! This binary is mechanism only: it reuses the already admitted CML pipeline
//! and owns no Lisp expected semantic answers.

use std::{
    env, fs,
    path::Path,
    process::{Command, ExitCode},
};

use cml::{lower, macros::MacroExpander, parser, x86_freestanding::X86FreestandingBackend};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args_os();
    let _program = args.next();
    let command = args
        .next()
        .ok_or_else(|| "usage: cml-compile x86-elf <source.lisp> <output>".to_string())?;
    let source = args
        .next()
        .ok_or_else(|| "usage: cml-compile x86-elf <source.lisp> <output>".to_string())?;
    let output = args
        .next()
        .ok_or_else(|| "usage: cml-compile x86-elf <source.lisp> <output>".to_string())?;
    if args.next().is_some() {
        return Err("usage: cml-compile x86-elf <source.lisp> <output>".into());
    }

    if command != "x86-elf" {
        return Err(format!(
            "unsupported compiler target: {}",
            command.to_string_lossy()
        ));
    }

    let source_path = Path::new(&source);
    if source_path
        .extension()
        .and_then(|extension| extension.to_str())
        != Some("lisp")
    {
        return Err("compiler source must use the canonical .lisp extension".into());
    }

    let source_text = fs::read_to_string(source_path)
        .map_err(|error| format!("could not read {}: {error}", source_path.display()))?;
    let expressions =
        parser::parse(&source_text).map_err(|error| parse_diagnostic(source_path, &error))?;
    let expanded = MacroExpander::new()
        .process(&expressions)
        .map_err(|error| error.to_string())?;
    let ir = lower::lower_program(&expanded).map_err(|error| error.to_string())?;
    let assembly = X86FreestandingBackend::new()
        .compile_program(&ir)
        .map_err(|error| error.to_string())?;
    let base = std::env::temp_dir().join(format!("cml-compile-{}", std::process::id()));
    let source = base.with_extension("s");
    let launcher = base.with_extension("c");
    fs::write(&source, assembly).map_err(|error| error.to_string())?;
    fs::write(&launcher, "#include <stdint.h>\nextern uint64_t wsm_entry(void *);\nint main(void) { (void)wsm_entry(0); return 0; }\n").map_err(|error| error.to_string())?;
    let nucleus = cml::x86_freestanding::resolve_nucleus_asm_path()?;
    let linked = Command::new("cc")
        .arg(&launcher)
        .arg(&source)
        .arg(nucleus)
        .arg("-o")
        .arg(&output)
        .output()
        .map_err(|error| error.to_string())?;
    let _ = fs::remove_file(&source);
    let _ = fs::remove_file(&launcher);
    if !linked.status.success() {
        return Err(format!(
            "x86 ELF link failed: {}",
            String::from_utf8_lossy(&linked.stderr)
        ));
    }

    Ok(())
}

fn parse_diagnostic(path: &Path, error: &parser::ParseError) -> String {
    let line = error.line().unwrap_or(0);
    let column = error.column().unwrap_or(0);
    format!(
        "CML-DIAGNOSTIC\tstage=parse\tpath={}\tline={line}\tcolumn={column}\tmessage={}",
        escape_diagnostic_field(&path.to_string_lossy()),
        escape_diagnostic_field(&error.to_string())
    )
}

fn escape_diagnostic_field(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\t', "\\t")
        .replace('\r', "\\r")
        .replace('\n', "\\n")
}
