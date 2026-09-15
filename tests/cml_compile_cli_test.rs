//! RED contract for #72: CML must expose the real external compiler process
//! consumed by my-idea's CompilerBridge.
//!
//! This test owns no Lisp expected answer. It checks transport/mechanism parity:
//! the CLI artifact must be byte-for-byte the artifact produced by CML's already
//! admitted in-process vertical pipeline for the same canonical `.lisp` source.

use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use cml::elf64::Elf64Executable;
use cml::lisp_asm_vertical::select_arithmetic_slice;
use cml::lower;
use cml::machine_inst::assemble_program;
use cml::macros::MacroExpander;
use cml::parser;

const FIXTURE_PATH: &str = "tests/fixtures/vertical_witness_add.lisp";

#[test]
fn cml_compile_x86_elf_matches_the_admitted_library_pipeline() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let source_path = manifest_dir.join(FIXTURE_PATH);
    let source = fs::read_to_string(&source_path).expect("read canonical Lisp fixture");

    let exprs = parser::parse(&source).expect("parse source");
    let expanded = MacroExpander::new()
        .process(&exprs)
        .expect("macro expansion");
    let ir = lower::lower_program(&expanded).expect("lower admitted source");
    let machine_items = select_arithmetic_slice(&ir).expect("select admitted machine slice");
    let direct_bytes = assemble_program(&machine_items).expect("encode admitted machine slice");
    let expected_elf = Elf64Executable::new(direct_bytes).to_bytes();

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let output_path = std::env::temp_dir().join(format!("cml-cli-{nonce}"));

    let status = Command::new(env!("CARGO_BIN_EXE_cml-compile"))
        .arg("x86-elf")
        .arg(&source_path)
        .arg(&output_path)
        .status()
        .expect("launch cml-compile");

    assert!(status.success(), "cml-compile must succeed for the admitted fixture");
    let actual_elf = fs::read(&output_path).expect("CLI must produce requested artifact");
    let _ = fs::remove_file(&output_path);

    assert_eq!(actual_elf, expected_elf);
}
