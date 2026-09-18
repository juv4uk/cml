//! External CML compiler process boundary for host integrations such as my-idea.
//!
//! This binary is mechanism only: it reuses the already admitted CML pipeline
//! and owns no Lisp expected semantic answers.

use std::{env, fs, path::Path, process::ExitCode};

use cml::{
    elf64::Elf64Executable, lisp_asm_vertical::select_arithmetic_slice, lower,
    machine_inst::assemble_program, macros::MacroExpander, parser,
};

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
    let machine_items = select_arithmetic_slice(&ir).map_err(|error| error.to_string())?;
    let bytes = assemble_program(&machine_items).map_err(|error| error.to_string())?;

    Elf64Executable::new(bytes)
        .write_executable(&output)
        .map_err(|error| format!("could not write {}: {error}", Path::new(&output).display()))?;

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
