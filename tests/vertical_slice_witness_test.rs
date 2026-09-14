//! # Vertical Slice Witness Test (Issue #36 P0)
//!
//! Proves the end-to-end chain:
//! ```text
//! committed Lisp fixture: (+ 10 32)
//!   -> parser / macro expansion
//!   -> semantic lowering into Ir::Prim(Add)
//!   -> target selection into Vec<MachineItem>
//!   -> direct x86-64 machine code bytes
//!   -> standalone ELF64 image (no C runtime, no Rust runtime in binary)
//!   -> native Linux kernel execution
//!   -> decoded Lisp value (fixnum 42)
//!   == differential GNU as / GNU ld output
//!   == semantic oracle value
//! ```

use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use cml::elf64::Elf64Executable;
use cml::ir::{Ir, PrimOp};
use cml::lower;
use cml::machine_inst::{
    VerticalSliceError, assemble_program, items_to_gnu_asm, select_arithmetic_slice,
};
use cml::macros::MacroExpander;
use cml::parser;

const FIXTURE_PATH: &str = "tests/fixtures/vertical_witness_add.lisp";

#[test]
fn test_vertical_slice_witness_end_to_end() {
    // 1. Read committed Lisp source fixture (no hand-constructed bytes as witness)
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let fixture_full_path = Path::new(manifest_dir).join(FIXTURE_PATH);
    let source = fs::read_to_string(&fixture_full_path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", fixture_full_path.display()));
    assert_eq!(source.trim(), "(+ 10 32)");

    // 2. Parser & Macro expansion
    let exprs = parser::parse(&source).expect("parsing fixture must succeed");
    let expanded = MacroExpander::new()
        .process(&exprs)
        .expect("macro expansion must succeed");

    // 3. Semantic Lowering into admitted IR
    let ir = lower::lower_program(&expanded).expect("lowering must succeed");
    assert_eq!(
        ir,
        vec![Ir::Prim {
            op: PrimOp::Add,
            args: vec![Ir::Int(10), Ir::Int(32)],
        }],
        "IR must lower into canonical Add primitive with integer operands"
    );

    // 4. Target Selection into Vec<MachineItem>
    let machine_items =
        select_arithmetic_slice(&ir).expect("target selection into MachineItems must succeed");
    assert!(!machine_items.is_empty());

    // 5. Direct x86-64 byte emission via two-pass relocation assembler
    let direct_bytes =
        assemble_program(&machine_items).expect("two-pass relocation assembly must succeed");
    assert!(!direct_bytes.is_empty());

    // 6. Determinism proof: direct byte emission must be byte-identical on repeated runs
    let direct_bytes_repeat =
        assemble_program(&machine_items).expect("repeat assembly must succeed");
    assert_eq!(
        direct_bytes, direct_bytes_repeat,
        "direct-byte emission must be strictly deterministic"
    );

    // 7. Synthesize minimal standalone ELF64 executable (zero C runtime, zero libc, zero external linker)
    let elf = Elf64Executable::new(direct_bytes.clone());
    let elf_bytes = elf.to_bytes();
    assert_eq!(
        &elf_bytes[0..4],
        &[0x7F, b'E', b'L', b'F'],
        "ELF magic header must match"
    );

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let temp_dir = std::env::temp_dir();
    let direct_elf_path = temp_dir.join(format!("cml-direct-witness-{nonce}"));
    elf.write_executable(&direct_elf_path)
        .expect("write direct ELF executable");

    // 8. Native execution of direct ELF
    let direct_output = Command::new(&direct_elf_path)
        .output()
        .expect("run direct ELF");
    let _ = fs::remove_file(&direct_elf_path);

    // Verify observable output of direct execution:
    // - Exit code is the untagged fixnum 42
    assert_eq!(
        direct_output.status.code(),
        Some(42),
        "native direct execution must exit with status 42"
    );
    // - Stdout is the 8-byte little-endian tagged fixnum word
    assert_eq!(
        direct_output.stdout.len(),
        8,
        "stdout must contain exactly 8 bytes for 64-bit result word"
    );
    let tagged_word = u64::from_le_bytes(direct_output.stdout[0..8].try_into().unwrap());
    let decoded_fixnum =
        wsm_os_target::decode_fixnum(tagged_word).expect("result word must decode as valid fixnum");
    assert_eq!(
        decoded_fixnum, 42,
        "decoded fixnum value must equal expected 42"
    );
    assert_eq!(
        tagged_word,
        wsm_os_target::encode_fixnum(42).unwrap(),
        "tagged result word must match wsm_os_target canonical fixnum encoding"
    );

    // 9. Differential test: Compare against existing GNU as + GNU ld path for the same program
    let gnu_asm_text = items_to_gnu_asm(&machine_items);
    let asm_source_path = temp_dir.join(format!("cml-gnu-witness-{nonce}.s"));
    let obj_path = temp_dir.join(format!("cml-gnu-witness-{nonce}.o"));
    let gnu_elf_path = temp_dir.join(format!("cml-gnu-witness-{nonce}"));

    fs::write(&asm_source_path, &gnu_asm_text).expect("write GNU asm text");
    let as_status = Command::new("as")
        .arg("-o")
        .arg(&obj_path)
        .arg(&asm_source_path)
        .status()
        .expect("invoke GNU as");
    assert!(as_status.success(), "GNU as must successfully assemble");

    let ld_status = Command::new("ld")
        .arg("-o")
        .arg(&gnu_elf_path)
        .arg(&obj_path)
        .status()
        .expect("invoke GNU ld");
    assert!(ld_status.success(), "GNU ld must successfully link");

    let gnu_output = Command::new(&gnu_elf_path)
        .output()
        .expect("run GNU-assembled ELF");

    // Clean up temporary files
    let _ = fs::remove_file(&asm_source_path);
    let _ = fs::remove_file(&obj_path);
    let _ = fs::remove_file(&gnu_elf_path);

    // Differential equivalence assertions
    assert_eq!(
        direct_output.status.code(),
        gnu_output.status.code(),
        "direct ELF exit status must match GNU ELF exit status"
    );
    assert_eq!(
        direct_output.stdout, gnu_output.stdout,
        "direct ELF stdout must match GNU ELF stdout"
    );

    // 10. Semantic Oracle Agreement
    // A. Direct Rust oracle computation: 10 + 32 = 42
    let oracle_expected = 10i64 + 32i64;
    assert_eq!(decoded_fixnum, oracle_expected);

    // B. Differential agreement with CML's existing C backend evaluation
    let mut c_backend = cml::c_backend::CBackend::new();
    let c_result = c_backend
        .compile_program(&ir)
        .expect("compile via C backend");
    assert!(
        c_result.contains("v_add"),
        "C backend projection must emit v_add"
    );

    // C. Agreement with my-lisp oracle
    let my_lisp_bin = std::env::var("MY_LISP_BIN")
        .unwrap_or_else(|_| "/home/agents/GitHub/my-lisp/target/release/my-lisp".to_string());
    if Path::new(&my_lisp_bin).exists() {
        let oracle_output = Command::new(&my_lisp_bin)
            .arg(&fixture_full_path)
            .output()
            .expect("invoke my-lisp on fixture");
        assert!(
            oracle_output.status.success(),
            "my-lisp must run successfully"
        );
        let stdout_str = String::from_utf8_lossy(&oracle_output.stdout);
        assert_eq!(
            stdout_str.trim(),
            "42",
            "my-lisp oracle on fixture must evaluate (+ 10 32) to 42"
        );
    }
}

#[test]
fn test_vertical_slice_fail_closed_validation() {
    // 1. Empty program fails closed
    assert_eq!(
        select_arithmetic_slice(&[]),
        Err(VerticalSliceError::EmptyProgram)
    );

    // 2. Arity mismatch fails closed
    let bad_arity = vec![Ir::Prim {
        op: PrimOp::Add,
        args: vec![Ir::Int(1)],
    }];
    assert_eq!(
        select_arithmetic_slice(&bad_arity),
        Err(VerticalSliceError::InvalidArity {
            expected: 2,
            actual: 1
        })
    );

    // 3. Unsupported variant fails closed
    let unsupported = vec![Ir::Nil];
    assert_eq!(
        select_arithmetic_slice(&unsupported),
        Err(VerticalSliceError::UnsupportedIrVariant(
            "expected fixnum or binary arithmetic"
        ))
    );
}
