//! Conformance and Triple-Oracle tests for the CML ↔ my-lisp encoder bridge (#52).
//!
//! Verifies:
//! 1. Authority boundary: Lisp owns the physical byte encoding and semantic IDs.
//! 2. Triple-oracle identity: Lisp-owned encoder (A) == CML direct encoder (B) == GNU as oracle (C).
//! 3. Native machine execution witness on Linux x86-64.
//! 4. Fail-closed rejection of unadmitted instructions and non-inst items.

#![cfg(all(target_os = "linux", target_arch = "x86_64"))]

use cml::lisp_encoder_bridge::{
    BridgeError, inst_to_lisp_encoder_call, items_to_lisp_encoder_program, parse_lisp_byte_list_str,
};
use cml::machine_inst::{AluOp, MachineInst, MachineItem, Provenance, X86Reg, assemble_program};
use my_lisp::{Session, eval_program, load_core_library};
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;

fn test_prov() -> Provenance {
    Provenance::new(None, "lisp_encoder_bridge_test")
}

/// The `external/my-lisp` submodule's checked-out tree — the single pin
/// (SUBMODULE-DEPENDENCY-MODEL-2026-09-16), not a sibling checkout guess.
fn submodule_repo(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("external")
        .join(name)
}

fn create_lisp_encoder_session() -> Session {
    let mut session = Session::default();
    load_core_library(&mut session).expect("load_core_library must succeed");

    let encoder_path = submodule_repo("my-lisp").join("lib/machine/encoding/x86-64.lisp");
    let encoder_src = fs::read_to_string(&encoder_path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", encoder_path.display()));
    eval_program(&encoder_src, &mut session)
        .unwrap_or_else(|e| panic!("failed to evaluate {}: {e:?}", encoder_path.display()));
    session
}

fn eval_lisp_encoder(session: &mut Session, expr: &str) -> Vec<u8> {
    let result = eval_program(expr, session)
        .unwrap_or_else(|e| panic!("Lisp evaluation failed for `{expr}`: {e:?}"));
    parse_lisp_byte_list_str(&result.value.to_string())
        .unwrap_or_else(|e| panic!("Failed to parse Lisp byte list `{}`: {e}", result.value))
}

fn assemble_gnu_as(text: &str) -> Vec<u8> {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let temp_dir = std::env::temp_dir();
    let s_path = temp_dir.join(format!("cml-as-test-{nonce}.s"));
    let o_path = temp_dir.join(format!("cml-as-test-{nonce}.o"));
    let bin_path = temp_dir.join(format!("cml-as-test-{nonce}.bin"));

    let mut file = fs::File::create(&s_path).expect("create temp asm file");
    writeln!(file, ".text\n{text}").expect("write asm text");
    file.flush().expect("flush asm text");

    let output = Command::new("as")
        .arg("--64")
        .arg(&s_path)
        .arg("-o")
        .arg(&o_path)
        .output()
        .expect("run GNU as");
    assert!(
        output.status.success(),
        "GNU as failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let objcopy = Command::new("objcopy")
        .arg("-O")
        .arg("binary")
        .arg("--only-section=.text")
        .arg(&o_path)
        .arg(&bin_path)
        .status()
        .expect("run objcopy");
    assert!(objcopy.success(), "objcopy failed");

    let bytes = fs::read(&bin_path).expect("read binary bytes");
    let _ = fs::remove_file(s_path);
    let _ = fs::remove_file(o_path);
    let _ = fs::remove_file(bin_path);
    bytes
}

unsafe fn execute_bytes_as_fn(bytes: &[u8]) -> u64 {
    use std::ffi::c_void;
    const PROT_READ: i32 = 0x1;
    const PROT_WRITE: i32 = 0x2;
    const PROT_EXEC: i32 = 0x4;
    const MAP_PRIVATE: i32 = 0x02;
    const MAP_ANONYMOUS: i32 = 0x20;

    unsafe extern "C" {
        fn mmap(
            addr: *mut c_void,
            len: usize,
            prot: i32,
            flags: i32,
            fd: i32,
            offset: isize,
        ) -> *mut c_void;
        fn mprotect(addr: *mut c_void, len: usize, prot: i32) -> i32;
        fn munmap(addr: *mut c_void, len: usize) -> i32;
    }

    unsafe {
        let len = bytes.len().max(4096);
        let ptr = mmap(
            std::ptr::null_mut(),
            len,
            PROT_READ | PROT_WRITE,
            MAP_PRIVATE | MAP_ANONYMOUS,
            -1,
            0,
        );
        assert!(
            !ptr.is_null() && ptr != usize::MAX as *mut c_void,
            "mmap failed: {}",
            std::io::Error::last_os_error()
        );

        std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr as *mut u8, bytes.len());
        let ret = mprotect(ptr, len, PROT_READ | PROT_EXEC);
        assert_eq!(
            ret,
            0,
            "mprotect failed: {}",
            std::io::Error::last_os_error()
        );

        let func: extern "C" fn() -> u64 = std::mem::transmute(ptr);
        let result = func();
        munmap(ptr, len);
        result
    }
}

/// The submodule gitlink itself is the only pin (SUBMODULE-DEPENDENCY-MODEL-
/// 2026-09-16) — there is no separate SHA constant to compare it against
/// anymore, so this just proves the contract dependency is actually checked
/// out and not an empty/uninitialized submodule directory.
#[test]
fn test_external_my_lisp_submodule_is_checked_out() {
    let encoder_path = submodule_repo("my-lisp").join("lib/machine/encoding/x86-64.lisp");
    assert!(
        encoder_path.exists(),
        "external/my-lisp submodule not checked out at {} — run `git submodule update --init`",
        encoder_path.display()
    );
}

#[test]
fn test_triple_oracle_mov_imm64() {
    let mut session = create_lisp_encoder_session();
    let cases = [
        (X86Reg::Rax, 0u64, "movabsq $0, %rax"),
        (X86Reg::Rax, 42u64, "movabsq $42, %rax"),
        (X86Reg::Rcx, 1000u64, "movabsq $1000, %rcx"),
        (
            X86Reg::R8,
            0x123456789ABCDEF0u64,
            "movabsq $0x123456789abcdef0, %r8",
        ),
        (X86Reg::R15, 0xFFFFFFFFFFFFFFFFu64, "movabsq $-1, %r15"),
    ];

    for (reg, imm, gnu_asm) in cases {
        let inst = MachineInst::MovImm64 {
            dst: reg,
            imm,
            provenance: test_prov(),
        };

        // Oracle A: Upstream Lisp encoder
        let lisp_expr = inst_to_lisp_encoder_call(&inst).unwrap();
        let bytes_a = eval_lisp_encoder(&mut session, &lisp_expr);

        // Oracle B: CML direct encoder
        let bytes_b = inst.encode_bytes();

        // Oracle C: GNU as assembler
        let bytes_c = assemble_gnu_as(gnu_asm);

        assert_eq!(
            bytes_a, bytes_b,
            "Lisp encoder (A) != CML encoder (B) for {reg:?} = {imm:#x}"
        );
        assert_eq!(
            bytes_b, bytes_c,
            "CML encoder (B) != GNU as (C) for {reg:?} = {imm:#x}"
        );
    }
}

#[test]
fn test_triple_oracle_add_r64_r64() {
    let mut session = create_lisp_encoder_session();
    let cases = [
        (X86Reg::Rax, X86Reg::Rcx, "addq %rcx, %rax"),
        (X86Reg::Rcx, X86Reg::Rdx, "addq %rdx, %rcx"),
        (X86Reg::R8, X86Reg::R9, "addq %r9, %r8"),
        (X86Reg::Rax, X86Reg::R15, "addq %r15, %rax"),
        (X86Reg::R14, X86Reg::Rbx, "addq %rbx, %r14"),
    ];

    for (dst, src, gnu_asm) in cases {
        let inst = MachineInst::AluRegReg {
            op: AluOp::Add,
            dst,
            src,
            provenance: test_prov(),
        };

        let lisp_expr = inst_to_lisp_encoder_call(&inst).unwrap();
        let bytes_a = eval_lisp_encoder(&mut session, &lisp_expr);
        let bytes_b = inst.encode_bytes();
        let bytes_c = assemble_gnu_as(gnu_asm);

        assert_eq!(
            bytes_a, bytes_b,
            "Lisp encoder (A) != CML encoder (B) for add {dst:?}, {src:?}"
        );
        assert_eq!(
            bytes_b, bytes_c,
            "CML encoder (B) != GNU as (C) for add {dst:?}, {src:?}"
        );
    }
}

#[test]
fn test_triple_oracle_ret() {
    let mut session = create_lisp_encoder_session();
    let inst = MachineInst::Ret {
        provenance: test_prov(),
    };

    let lisp_expr = inst_to_lisp_encoder_call(&inst).unwrap();
    let bytes_a = eval_lisp_encoder(&mut session, &lisp_expr);
    let bytes_b = inst.encode_bytes();
    let bytes_c = assemble_gnu_as("ret");

    assert_eq!(bytes_a, vec![0xC3]);
    assert_eq!(bytes_a, bytes_b);
    assert_eq!(bytes_b, bytes_c);
}

#[test]
fn test_triple_oracle_scalar_witness_program() {
    let mut session = create_lisp_encoder_session();

    // Program: (+ 10 32) -> 42
    // mov rax, 10
    // mov rcx, 32
    // add rax, rcx
    // ret
    let items = vec![
        MachineItem::Inst(MachineInst::MovImm64 {
            dst: X86Reg::Rax,
            imm: 10,
            provenance: test_prov(),
        }),
        MachineItem::Inst(MachineInst::MovImm64 {
            dst: X86Reg::Rcx,
            imm: 32,
            provenance: test_prov(),
        }),
        MachineItem::Inst(MachineInst::AluRegReg {
            op: AluOp::Add,
            dst: X86Reg::Rax,
            src: X86Reg::Rcx,
            provenance: test_prov(),
        }),
        MachineItem::Inst(MachineInst::Ret {
            provenance: test_prov(),
        }),
    ];

    // Oracle A: Lisp encoder program
    let lisp_prog = items_to_lisp_encoder_program(&items).unwrap();
    let bytes_a = eval_lisp_encoder(&mut session, &lisp_prog);

    // Oracle B: CML direct assembler
    let bytes_b = assemble_program(&items).unwrap();

    // Oracle C: GNU as
    let gnu_asm = "movabsq $10, %rax\nmovabsq $32, %rcx\naddq %rcx, %rax\nret";
    let bytes_c = assemble_gnu_as(gnu_asm);

    assert_eq!(bytes_a, bytes_b, "Oracle A != Oracle B");
    assert_eq!(bytes_b, bytes_c, "Oracle B != Oracle C");

    // Native execution witness: call machine code directly, verify result 42
    let result = unsafe { execute_bytes_as_fn(&bytes_a) };
    assert_eq!(
        result, 42,
        "native execution of Lisp-encoded bytes must return 42"
    );
}

#[test]
fn test_unadmitted_items_fail_closed() {
    let unadmitted_label = vec![
        MachineItem::Label("loop".to_string()),
        MachineItem::Inst(MachineInst::Ret {
            provenance: test_prov(),
        }),
    ];
    assert!(matches!(
        items_to_lisp_encoder_program(&unadmitted_label),
        Err(BridgeError::InvalidProgram(_))
    ));

    let unadmitted_jmp = vec![MachineItem::JmpLabel {
        target: "loop".to_string(),
        provenance: test_prov(),
    }];
    assert!(matches!(
        items_to_lisp_encoder_program(&unadmitted_jmp),
        Err(BridgeError::InvalidProgram(_))
    ));
}
