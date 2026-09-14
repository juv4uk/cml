//! End-to-end Lisp -> CML -> structured x86 -> native ELF witness.
//!
//! The produced executable contains no C runtime and no Rust runtime. Rust is
//! used only by the compiler/test harness that constructs and inspects the
//! target artifact.

use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use cml::elf64::Elf64Executable;
use cml::ir::{Ir, PrimOp};
use cml::lisp_asm_vertical::{VerticalSliceError, items_to_gnu_asm, select_arithmetic_slice};
use cml::lower;
use cml::machine_inst::assemble_program;
use cml::macros::MacroExpander;
use cml::parser;

const FIXTURE_PATH: &str = "tests/fixtures/vertical_witness_add.lisp";

#[test]
fn lisp_source_reaches_native_x86_without_c_or_rust_runtime() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let fixture_full_path = Path::new(manifest_dir).join(FIXTURE_PATH);
    let source = fs::read_to_string(&fixture_full_path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", fixture_full_path.display()));
    assert_eq!(source.trim(), "(+ 10 32)");

    let exprs = parser::parse(&source).expect("parsing fixture must succeed");
    let expanded = MacroExpander::new()
        .process(&exprs)
        .expect("macro expansion must succeed");

    let ir = lower::lower_program(&expanded).expect("lowering must succeed");
    assert_eq!(
        ir,
        vec![Ir::Prim {
            op: PrimOp::Add,
            args: vec![Ir::Int(10), Ir::Int(32)],
        }],
        "fixture must lower through admitted Lisp IR, not hand-built target bytes"
    );

    let machine_items =
        select_arithmetic_slice(&ir).expect("target selection into MachineItems must succeed");
    assert!(!machine_items.is_empty());

    let direct_bytes =
        assemble_program(&machine_items).expect("two-pass relocation assembly must succeed");
    assert!(!direct_bytes.is_empty());
    assert_eq!(
        direct_bytes,
        assemble_program(&machine_items).expect("repeat assembly must succeed"),
        "direct-byte emission must be deterministic"
    );

    let elf = Elf64Executable::new(direct_bytes.clone());
    let elf_bytes = elf.to_bytes();
    assert_eq!(&elf_bytes[0..4], &[0x7F, b'E', b'L', b'F']);

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let temp_dir = std::env::temp_dir();
    let direct_elf_path = temp_dir.join(format!("cml-direct-witness-{nonce}"));
    elf.write_executable(&direct_elf_path)
        .expect("write direct ELF executable");

    let direct_output = Command::new(&direct_elf_path)
        .output()
        .expect("run direct ELF");
    let _ = fs::remove_file(&direct_elf_path);

    assert_eq!(direct_output.status.code(), Some(42));
    assert_eq!(direct_output.stdout.len(), 8);
    let tagged_word = u64::from_le_bytes(direct_output.stdout[0..8].try_into().unwrap());
    let decoded_fixnum =
        wsm_os_target::decode_fixnum(tagged_word).expect("result must be a target fixnum");
    assert_eq!(decoded_fixnum, 42);
    assert_eq!(
        tagged_word,
        wsm_os_target::encode_fixnum(42).unwrap(),
        "native result must use the ratified target representation"
    );

    // Independent projection oracle: the same structured machine program must
    // produce equivalent observable behavior through GNU as/ld.
    let gnu_asm_text = items_to_gnu_asm(&machine_items);
    let asm_source_path = temp_dir.join(format!("cml-gnu-witness-{nonce}.s"));
    let obj_path = temp_dir.join(format!("cml-gnu-witness-{nonce}.o"));
    let gnu_elf_path = temp_dir.join(format!("cml-gnu-witness-{nonce}"));

    fs::write(&asm_source_path, &gnu_asm_text).expect("write GNU asm text");
    assert!(
        Command::new("as")
            .arg("-o")
            .arg(&obj_path)
            .arg(&asm_source_path)
            .status()
            .expect("invoke GNU as")
            .success()
    );
    assert!(
        Command::new("ld")
            .arg("-o")
            .arg(&gnu_elf_path)
            .arg(&obj_path)
            .status()
            .expect("invoke GNU ld")
            .success()
    );

    let gnu_output = Command::new(&gnu_elf_path)
        .output()
        .expect("run GNU-assembled ELF");
    let _ = fs::remove_file(&asm_source_path);
    let _ = fs::remove_file(&obj_path);
    let _ = fs::remove_file(&gnu_elf_path);

    assert_eq!(direct_output.status.code(), gnu_output.status.code());
    assert_eq!(direct_output.stdout, gnu_output.stdout);
}

#[test]
fn arithmetic_slice_fails_closed_outside_its_admitted_scope() {
    assert_eq!(
        select_arithmetic_slice(&[]),
        Err(VerticalSliceError::EmptyProgram)
    );

    let bad_arity = vec![Ir::Prim {
        op: PrimOp::Add,
        args: vec![Ir::Int(1)],
    }];
    assert_eq!(
        select_arithmetic_slice(&bad_arity),
        Err(VerticalSliceError::InvalidArity {
            expected: 2,
            actual: 1,
        })
    );

    assert_eq!(
        select_arithmetic_slice(&[Ir::Nil]),
        Err(VerticalSliceError::UnsupportedIrVariant(
            "expected fixnum or binary arithmetic"
        ))
    );
}
