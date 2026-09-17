#![cfg(all(target_os = "linux", target_arch = "x86_64"))]

use cml::machine_inst::{MachineInst, Provenance, X86Reg};
use std::fs;
use std::io::Write;
use std::process::Command;

fn assemble_gnu_as(text: &str) -> Vec<u8> {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let temp_dir = std::env::temp_dir();
    let s_path = temp_dir.join(format!("cml-divq-red-{nonce}.s"));
    let o_path = temp_dir.join(format!("cml-divq-red-{nonce}.o"));
    let bin_path = temp_dir.join(format!("cml-divq-red-{nonce}.bin"));

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

fn mechanism_provenance() -> Provenance {
    Provenance::new(None, "#101 semantics-neutral unsigned divq substrate")
}

#[test]
fn unsigned_divq_register_projection_matches_gnu_oracle_without_semantic_identity() {
    for divisor in [X86Reg::Rcx, X86Reg::R9] {
        let inst = MachineInst::DivReg {
            divisor,
            provenance: mechanism_provenance(),
        };

        assert_eq!(
            inst.provenance().semantic_id,
            None,
            "machine divq substrate must not claim semantic identity 1007"
        );

        let asm = inst.print_gnu_asm();
        assert_eq!(asm, format!("divq {}", divisor.name()));
        assert_eq!(
            inst.encode_bytes(),
            assemble_gnu_as(&asm),
            "direct encoder must match GNU as for {divisor:?}"
        );
    }
}
