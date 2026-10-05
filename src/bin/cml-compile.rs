//! External CML compiler process boundary for host integrations such as my-idea.
//!
//! This binary is mechanism only: it reuses the already admitted CML pipeline
//! and owns no Lisp expected semantic answers.

use std::{env, fs, path::Path, process::ExitCode, time::Instant};

use cml::{
    elf64::Elf64Executable, lisp_asm_vertical::select_arithmetic_slice, lower,
    machine_inst::assemble_program, macros::MacroExpander, parser,
    x86_freestanding::X86FreestandingBackend,
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
    const USAGE: &str = "usage: cml-compile x86-elf <source.lisp> <output> | \
cml-compile x86-elf-profile <source.lisp> <output> | cml-compile passes";
    let command = args.next().ok_or_else(|| USAGE.to_string())?;

    if command == "passes" {
        if args.next().is_some() {
            return Err(USAGE.into());
        }
        for pass in cml::compiler_passes::pass_manifest() {
            println!(
                "CML-PASS\tid={}\tinput={}\toutput={}\tevidence={}\tobligation={}",
                pass.id,
                pass.input,
                pass.output,
                pass.current_evidence.as_str(),
                pass.obligation
            );
        }
        return Ok(());
    }

    let source = args.next().ok_or_else(|| USAGE.to_string())?;
    let output = args.next().ok_or_else(|| USAGE.to_string())?;
    if args.next().is_some() {
        return Err(USAGE.into());
    }

    let profile = command == "x86-elf-profile";
    if command != "x86-elf" && !profile {
        return Err(format!(
            "unsupported compiler target: {}",
            command.to_string_lossy()
        ));
    }

    let total_started = Instant::now();
    let source_path = Path::new(&source);
    if source_path
        .extension()
        .and_then(|extension| extension.to_str())
        != Some("lisp")
    {
        return Err("compiler source must use the canonical .lisp extension".into());
    }

    let phase_started = Instant::now();
    let source_text = fs::read_to_string(source_path)
        .map_err(|error| format!("could not read {}: {error}", source_path.display()))?;
    emit_phase(profile, "load", phase_started.elapsed());

    let phase_started = Instant::now();
    let expressions =
        parser::parse(&source_text).map_err(|error| parse_diagnostic(source_path, &error))?;
    emit_phase(profile, "parse", phase_started.elapsed());

    let phase_started = Instant::now();
    let expanded = MacroExpander::new()
        .process(&expressions)
        .map_err(|error| error.to_string())?;
    emit_phase(profile, "macro-expand", phase_started.elapsed());

    let phase_started = Instant::now();
    let ir = lower::lower_program(&expanded).map_err(|error| error.to_string())?;
    emit_phase(profile, "lower", phase_started.elapsed());

    let phase_started = Instant::now();
    if let Ok(machine_items) = select_arithmetic_slice(&ir) {
        let bytes = assemble_program(&machine_items).map_err(|error| error.to_string())?;
        emit_phase(profile, "target-codegen", phase_started.elapsed());

        let phase_started = Instant::now();
        Elf64Executable::new(bytes)
            .write_executable(&output)
            .map_err(|error| {
                format!("could not write {}: {error}", Path::new(&output).display())
            })?;
        emit_phase(profile, "emit", phase_started.elapsed());
        emit_phase(profile, "total", total_started.elapsed());
        return Ok(());
    }

    let assembly = X86FreestandingBackend::new()
        .compile_program(&ir)
        .map_err(|error| error.to_string())?;
    emit_phase(profile, "target-codegen", phase_started.elapsed());

    let phase_started = Instant::now();
    cml::x86_elf::link_x86_elf(&assembly, Path::new(&output))?;
    emit_phase(profile, "emit", phase_started.elapsed());
    emit_phase(profile, "total", total_started.elapsed());

    Ok(())
}

fn emit_phase(enabled: bool, name: &str, elapsed: std::time::Duration) {
    if enabled {
        eprintln!("CML-PHASE\tname={name}\telapsed_ns={}", elapsed.as_nanos());
    }
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
