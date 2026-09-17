use cml::machine_inst::{MachineInst, Provenance, X86Reg};
use std::process::Command;

fn assemble_gnu_as(text: &str) -> Vec<u8> {
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock")
        .as_nanos();
    let base = std::env::temp_dir().join(format!("cml-divq-test-{}-{nonce}", std::process::id()));
    let s_path = base.with_extension("s");
    let o_path = base.with_extension("o");
    let bin_path = base.with_extension("bin");

    let mut file = std::fs::File::create(&s_path).expect("create temp asm file");
    writeln!(file, ".global _start\n_start:\n    {text}").expect("write asm text");
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
        "GNU as failed for `{text}`: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let status = Command::new("objcopy")
        .arg("-O")
        .arg("binary")
        .arg("--only-section=.text")
        .arg(&o_path)
        .arg(&bin_path)
        .status()
        .expect("run objcopy");
    assert!(status.success(), "objcopy failed");

    let bytes = std::fs::read(&bin_path).expect("read oracle bytes");
    let _ = std::fs::remove_file(s_path);
    let _ = std::fs::remove_file(o_path);
    let _ = std::fs::remove_file(bin_path);
    bytes
}

#[test]
fn unsigned_divq_is_a_semantics_neutral_structured_machine_instruction() {
    let provenance = Provenance::new(None, "unsigned divide mechanism");

    for divisor in [X86Reg::Rcx, X86Reg::R9] {
        let inst = MachineInst::DivReg {
            divisor,
            provenance: provenance.clone(),
        };

        assert_eq!(inst.provenance().semantic_id, None);

        let asm = inst.print_gnu_asm();
        assert_eq!(asm, format!("divq {}", divisor.name()));
        assert_eq!(
            inst.encode_bytes(),
            assemble_gnu_as(&asm),
            "direct bytes must match GNU as for {divisor:?}"
        );
    }
}
