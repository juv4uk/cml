use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use cml::c_backend::CBackend;
use cml::ir::{Ir, Quoted};
use cml::sens_domain_bridge::{
    pinned_authority, verify_call, MechanismStatus, SemanticRequest, SemanticStatus,
};

fn d3(raw: u8) -> sens::CoreDomainIdentity {
    sens::CoreDomainIdentity::D3(sens::Bija3::from_word(
        sens::Bit3::new(raw).expect("valid D3 bits"),
    ))
}

fn verified(identity: sens::CoreDomainIdentity, arg: Ir) -> cml::sens_domain_bridge::VerifiedDomainCall {
    verify_call(
        SemanticRequest {
            identity,
            law_ref: "language-contract.lisp:d3-foundation".into(),
            proof_ref: "contracts/bija3-l1-l5-ratification.lisp".into(),
            semantic_status: SemanticStatus::Current,
            mechanism_status: MechanismStatus::Admitted,
            provenance: pinned_authority().expect("pinned SENS authority"),
        },
        vec![arg],
    )
    .expect("current D3 request must verify")
}

fn quoted_pair() -> Ir {
    Ir::Quote(Quoted::List(vec![
        Quoted::Sym {
            uppercased: "A".into(),
            original: "A".into(),
        },
        Quoted::Sym {
            uppercased: "B".into(),
            original: "B".into(),
        },
    ]))
}

fn compile_and_run_domain_call(
    call: &cml::sens_domain_bridge::VerifiedDomainCall,
    stem: &str,
) -> String {
    let c_source = CBackend::new()
        .compile_verified_domain_call(call)
        .expect("verified domain call must reach C mechanism");

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let c_path = format!("domain_bridge_{stem}_{}_{}.c", std::process::id(), nonce);
    let bin_path = format!("domain_bridge_{stem}_{}_{}", std::process::id(), nonce);
    fs::write(&c_path, &c_source).unwrap();

    let mut gcc = Command::new("gcc");
    if std::env::var("C_INCLUDE_PATH").is_err()
        && std::path::Path::new("/var/guix/profiles/shared/guix-profile/include").exists()
    {
        gcc.env(
            "C_INCLUDE_PATH",
            "/var/guix/profiles/shared/guix-profile/include",
        );
    }

    let compile = gcc.arg(&c_path).arg("-o").arg(&bin_path).output().unwrap();
    if !compile.status.success() {
        panic!(
            "gcc failed:\n{}\n--- generated C ---\n{}",
            String::from_utf8_lossy(&compile.stderr),
            c_source
        );
    }

    let run = Command::new(format!("./{bin_path}")).output().unwrap();
    let _ = fs::remove_file(c_path);
    let _ = fs::remove_file(bin_path);

    assert!(
        run.status.success(),
        "compiled domain witness failed: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    String::from_utf8(run.stdout).unwrap().trim().to_string()
}

#[test]
fn current_d3_car_reaches_executable_c_without_sid8_projection() {
    let call = verified(d3(0b100), quoted_pair());
    assert_eq!(call.identity().width(), 3);
    assert_eq!(call.identity().packed_bits(), 0b100);
    assert_eq!(compile_and_run_domain_call(&call, "car"), "A");
}

#[test]
fn current_d3_cdr_reaches_executable_c_without_sid8_projection() {
    let call = verified(d3(0b011), quoted_pair());
    assert_eq!(call.identity().width(), 3);
    assert_eq!(call.identity().packed_bits(), 0b011);
    assert_eq!(compile_and_run_domain_call(&call, "cdr"), "(B)");
}
