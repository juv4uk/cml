//! #667: a real physical Core1 T5 -> SENS verified current-domain source ->
//! existing CML lowering -> freestanding x86 assembly -> real WSM nucleus.
//!
//! This test is an integration experiment on an EXACT candidate SENS SHA; it
//! never moves the production external/sens gitlink or Cargo.lock. Current
//! compiler-export and target mechanisms are the only semantic/ABI owners.
//! It fails closed: no hand-built IR, pre-baked native output or Sid8 fallback.
use cml::sens_current_lowering::lower_current_sens_source;
use cml::x86_freestanding::X86FreestandingBackend;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

const UPSTREAM_SENS_SHA: &str = "5ccbeabfb1703ce5259dcb8cf55569f9e574c6c1";
const NATIVE_WSM_SHA: &str = "fbfbcf6adb84f161757ae23eab4abbaa59985ddf";
const EXPECTED_SOURCE_BYTES: usize = 22;
const EXPECTED_PHYSICAL_SHA256: &str =
    "43846420b5b4464ab6538d8927edfdf4759bd2446c427f34a165cc89c8812a2a";

fn hex_digest(bytes: &[u8]) -> String {
    sens::sha256_source(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn candidate_file() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("external/sens/tests/fixtures/core1-domain-canary/second.sens")
}

fn candidate_words() -> String {
    let file = candidate_file();
    assert!(
        file.exists(),
        "BLOCK: missing exact-source Core1 fixture from SENS {UPSTREAM_SENS_SHA}"
    );
    let bytes = fs::read(file).expect("physical T5 file required");
    assert_eq!(bytes.len(), EXPECTED_SOURCE_BYTES);
    assert_eq!(hex_digest(&bytes), EXPECTED_PHYSICAL_SHA256,
               "BLOCK: candidate source not pinned to tested 22-byte T5 artifact");
    let words = sens::decode_ternary_program(&bytes)
        .expect("BLOCK: source is not canonical physical T5/D2 syntax");
    let visible = sens::render_ternary_words_spaced(&words);
    assert_eq!(
        sens::encode_binary_projection_ternary(&visible)
            .expect("current SENS exact-domain encode must succeed"),
        bytes,
        "BLOCK: T5 typed-identity round trip differs"
    );
    visible
}

fn output(command: &mut Command, label: &str) -> Output {
    let actual = command.output().unwrap_or_else(|error| {
        panic!("BLOCK: {label} could not launch: {error}")
    });
    assert!(
        actual.status.success(),
        "BLOCK: {label} exit={:?} stderr={} stdout={}",
        actual.status.code(),
        String::from_utf8_lossy(&actual.stderr),
        String::from_utf8_lossy(&actual.stdout),
    );
    actual
}

#[test]
fn physical_core1_second_compiles_through_current_cml_to_real_native_wsm() {
    let projection = candidate_words();
    let export = fs::read_to_string(std::env::var("SENS_COMPILER_EXPORT_FILE")
        .expect("BLOCK: SENS export path must come from exact candidate checkout"))
        .expect("BLOCK: SENS compiler export cannot be read");

    // CML must consume current SENS-verified mechanism identities and roles.
    // No manual bit->meaning dispatcher or custom Rust CAR/CDR/CONS evaluator.
    let lowered = lower_current_sens_source(&projection, &export)
        .expect("BLOCK: current exact-domain SENS source not admitted by CML");
    assert_eq!(lowered.ir.len(), 1, "one closed specialization, not two evaluators");
    let debug_ir = format!("{:?}", lowered.ir);
    for forbidden in ["Sid(", "Builtin("] {
        assert!(!debug_ir.contains(forbidden),
                "BLOCK: legacy runtime dispatch found: {forbidden}");
    }

    let backend = X86FreestandingBackend::new();
    let asm = backend.compile_program(&lowered.ir)
        .expect("BLOCK: current verified D3 mechanism unsupported by WSM backend");
    assert_eq!(asm, backend.compile_program(&lowered.ir).unwrap(),
               "same exact input must produce deterministic freestanding assembly");
    for intrinsic in ["call wsm_cons", "call wsm_car", "call wsm_cdr"] {
        assert!(asm.contains(intrinsic),
                "BLOCK: expected current mechanism missing from generated machine source: {intrinsic}");
    }
    assert!(!asm.contains("wsm_sid8"),
            "BLOCK: historical SID8 cannot become current machine authority");

    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)
        .expect("clock after epoch").as_nanos();
    let folder = std::env::temp_dir().join(format!(
        "core1-t5-cml-wsm-{}-{nonce}", std::process::id()));
    fs::create_dir(&folder).expect("create isolated test output directory");

    let asm_path = folder.join("core1-generated.s");
    let object_path = folder.join("core1-generated.o");
    let runtime_path = PathBuf::from(
        std::env::var("WSM_NATIVE_NUCLEUS")
            .expect("BLOCK: exact WSM native nucleus path missing")
    );
    assert!(runtime_path.is_file(), "BLOCK: WSM candidate nucleus missing");

    let harness_path = folder.join("main.c");
    let binary_path = folder.join("core1-native");
    fs::write(&asm_path, &asm).expect("write generated CML assembly");
    // These C functions are mechanical entrypoint/error reporting only.
    // All language structural operations are in the real WSM x86 nucleus.
    fs::write(&harness_path, r#"
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
extern uint64_t wsm_entry(void *ctx);
void wsm_fail(void *ctx, uint32_t code, uint64_t a, uint64_t b) {
    (void)ctx; (void)a; (void)b;
    fprintf(stderr, "WSM_FAIL %u\n", code);
    exit(97);
}
int main(void) {
    printf("%llu\n", (unsigned long long)wsm_entry((void *)0));
    return 0;
}
"#).expect("write only target entrypoint stub");

    output(Command::new("cc")
        .arg("-x").arg("assembler").arg("-c")
        .arg(&asm_path).arg("-o").arg(&object_path), "CML assembler");
    let machine_bytes = fs::read(&object_path).expect("read compiled ELF object");
    assert!(!machine_bytes.is_empty(), "BLOCK: compiled machine object is empty");

    output(Command::new("cc").arg(&object_path).arg(&runtime_path)
        .arg(&harness_path).arg("-o").arg(&binary_path),
        "link compiler-emitted object to actual WSM nucleus");

    let native = output(&mut Command::new(&binary_path),
                        "native executable from physical Core1 .sens");
    let actual: u64 = String::from_utf8(native.stdout).unwrap().trim()
        .parse().expect("native entry prints exactly one target-contract word");
    assert_eq!(actual, wsm_os_target::NIL,
               "actual native result must match SENS D3:000 EMPTY oracle result");

    println!("CORE1_T5_TO_NATIVE_PASS source_sens={} wsm_nucleus={} physical_sha256={} generated_asm_sha256={} object_sha256={} native_word={}",
        UPSTREAM_SENS_SHA, NATIVE_WSM_SHA, EXPECTED_PHYSICAL_SHA256,
        hex_digest(asm.as_bytes()), hex_digest(&machine_bytes), actual);
    let _ = fs::remove_dir_all(folder);
}

#[test]
fn tampered_t5_source_fails_before_compiler_or_native() {
    let mut bytes = fs::read(candidate_file()).unwrap();
    bytes.push(242); // an obsolete EOS byte, never part of physical T5 file
    assert!(sens::decode_ternary_program(&bytes).is_err());
    assert!(sens::decode_ternary_program(&[243u8]).is_err());
    assert!(sens::encode_binary_projection_ternary("(CAR (CDR X))").is_err());
}
