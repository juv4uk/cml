//! #46 dual physical-route semantic witness.
//!
//! Semantic authority stays in the pinned my-lisp corpus + witness-runner.
//! Rust owns only compiler/encoding/execution transport. The source witness is
//! selected from the upstream corpus by actual MachineInst admission; no
//! expected Lisp answer is copied here.

#![cfg(all(target_os = "linux", target_arch = "x86_64"))]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use cml::elf64::Elf64Executable;
use cml::lisp_asm_vertical::{items_to_gnu_asm, select_arithmetic_slice};
use cml::machine_inst::{MachineItem, assemble_program};
use cml::macros::MacroExpander;
use cml::witness_bridge::canonical_actual_from_word;
use cml::{lower, parser};
use my_lisp::{Session, eval_program, load_core_library};

fn upstream_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("external/my-lisp")
        .join(relative)
}

fn upstream_corpus() -> String {
    let path = upstream_path("tests/fixtures/conformance.lisp");
    fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "#46 requires pinned upstream conformance corpus at {}: {error}",
            path.display()
        )
    })
}

fn expr_from_row(row: &str) -> Option<String> {
    let tail = row.split_once("((expr . \"")?.1;
    let expr = tail.split_once("\")")?.0;
    Some(expr.to_string())
}

fn first_machineinst_admitted_upstream_row(corpus: &str) -> (String, String, Vec<MachineItem>) {
    for row in corpus
        .lines()
        .filter(|line| !line.trim_start().starts_with(';'))
    {
        let Some(source) = expr_from_row(row) else {
            continue;
        };
        let Ok(parsed) = parser::parse(&source) else {
            continue;
        };
        let Ok(expanded) = MacroExpander::new().process(&parsed) else {
            continue;
        };
        let Ok(ir) = lower::lower_program(&expanded) else {
            continue;
        };
        let Ok(items) = select_arithmetic_slice(&ir) else {
            continue;
        };
        return (row.to_string(), source, items);
    }

    panic!("#46 requires at least one upstream witness admitted by the #36 MachineInst slice");
}

fn canonical_actual_from_stdout(stdout: &[u8]) -> String {
    assert_eq!(
        stdout.len(),
        8,
        "MachineInst witness must expose exactly one target word"
    );
    let word = u64::from_le_bytes(stdout.try_into().expect("exactly eight bytes"));
    canonical_actual_from_word(word).expect("admitted arithmetic result must canonicalize")
}

fn execute_direct(items: &[MachineItem], nonce: u128) -> String {
    let bytes = assemble_program(items).expect("direct MachineInst encoder must succeed");
    let path = std::env::temp_dir().join(format!("cml-46-direct-{nonce}"));
    Elf64Executable::new(bytes)
        .write_executable(&path)
        .expect("direct ELF writer must succeed");

    let output = Command::new(&path)
        .output()
        .expect("execute direct-byte ELF");
    let _ = fs::remove_file(&path);
    assert!(
        output.status.success() || output.status.code().is_some(),
        "direct-byte witness must execute to a normal process result"
    );
    canonical_actual_from_stdout(&output.stdout)
}

fn execute_gnu(items: &[MachineItem], nonce: u128) -> String {
    let temp = std::env::temp_dir();
    let source = temp.join(format!("cml-46-gnu-{nonce}.s"));
    let object = temp.join(format!("cml-46-gnu-{nonce}.o"));
    let executable = temp.join(format!("cml-46-gnu-{nonce}"));

    fs::write(&source, items_to_gnu_asm(items)).expect("write GNU projection");
    assert!(
        Command::new("as")
            .arg("-o")
            .arg(&object)
            .arg(&source)
            .status()
            .expect("invoke GNU as")
            .success(),
        "GNU assembler must accept the MachineInst projection"
    );
    assert!(
        Command::new("ld")
            .arg("-o")
            .arg(&executable)
            .arg(&object)
            .status()
            .expect("invoke GNU ld")
            .success(),
        "GNU linker must accept the assembled witness"
    );

    let output = Command::new(&executable)
        .output()
        .expect("execute GNU-assembled ELF");
    let _ = fs::remove_file(source);
    let _ = fs::remove_file(object);
    let _ = fs::remove_file(executable);

    assert!(
        output.status.success() || output.status.code().is_some(),
        "GNU witness must execute to a normal process result"
    );
    canonical_actual_from_stdout(&output.stdout)
}

fn assert_lisp_owned_verdict(row: &str, actual: &str) {
    let runner_path = upstream_path("tests/fixtures/witness-runner.lisp");
    let runner = fs::read_to_string(&runner_path).unwrap_or_else(|error| {
        panic!(
            "#46 requires pinned Lisp-owned witness protocol at {}: {error}",
            runner_path.display()
        )
    });

    let mut session = Session::default();
    load_core_library(&mut session).expect("pinned my-lisp core library must load");
    eval_program(&runner, &mut session).expect("pinned witness-runner.lisp must load");

    let verdict = eval_program(
        &format!("(witness-pass? (witness-verdict (quote {row}) (quote {actual})))"),
        &mut session,
    )
    .expect("Lisp-owned witness verdict must execute")
    .value
    .to_string();

    assert_eq!(
        verdict, "t",
        "semantic PASS must come only from upstream Lisp-owned witness-verdict"
    );
}

#[test]
fn upstream_witness_has_direct_byte_and_gnu_actuals_judged_by_lisp() {
    let corpus = upstream_corpus();
    let (row, source, items) = first_machineinst_admitted_upstream_row(&corpus);
    assert!(
        corpus.contains(&row) && row.contains(&format!("(expr . \"{source}\")")),
        "witness source must come directly from the pinned upstream corpus"
    );

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let direct_actual = execute_direct(&items, nonce);
    let gnu_actual = execute_gnu(&items, nonce);

    assert_eq!(
        direct_actual, gnu_actual,
        "encoding mechanisms must agree on the canonical actual before semantic judgement"
    );

    assert_lisp_owned_verdict(&row, &direct_actual);
    assert_lisp_owned_verdict(&row, &gnu_actual);
}
