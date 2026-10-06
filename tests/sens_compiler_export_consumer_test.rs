use cml::compiler_mechanism::RichCompilerMechanismRef;
use cml::sens_compiler_export::{
    CompilerExportError, parse_compiler_export, verify_exported_request,
};
use cml::sens_rich_bridge::RichBridgeError;
use std::collections::BTreeSet;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn run_pinned_sens_export() -> String {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest = root.join("external/sens/Cargo.toml");
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos();
    let target = std::env::temp_dir().join(format!(
        "cml-sens-compiler-export-{}-{nonce}",
        std::process::id()
    ));

    let output = Command::new("cargo")
        .current_dir(root.join("external/sens"))
        .env("CARGO_TARGET_DIR", &target)
        .args([
            "run",
            "--quiet",
            "--manifest-path",
            manifest.to_str().expect("UTF-8 SENS manifest path"),
            "-p",
            "xtask",
            "--",
            "compiler-export",
        ])
        .output()
        .expect("pinned SENS compiler-export must execute");

    let _ = std::fs::remove_dir_all(&target);

    assert!(
        output.status.success(),
        "pinned SENS compiler-export failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("compiler export is UTF-8")
}

#[test]
fn real_pinned_sens_export_verifies_all_nine_roles() {
    let text = run_pinned_sens_export();
    let requests = parse_compiler_export(&text).expect("parse real SENS compiler export");
    assert_eq!(requests.len(), 9);

    let mut d3 = 0usize;
    let mut d4 = 0usize;
    let mut mechanisms = BTreeSet::new();

    for request in requests {
        match request.identity {
            sens::DomainIdentity::D3(_) => d3 += 1,
            sens::DomainIdentity::D4(_) => d4 += 1,
            other => panic!("compiler export admitted unexpected identity: {other:?}"),
        }

        let verified = verify_exported_request(request).expect("SENS export must verify in CML");
        mechanisms.insert(verified.mechanism_ref().as_str().to_string());
    }

    assert_eq!(d3, 7);
    assert_eq!(d4, 2);
    assert_eq!(mechanisms.len(), 9, "all nine private mechanisms must be reached");

    for required in [
        RichCompilerMechanismRef::Quote,
        RichCompilerMechanismRef::AtomPredicateD1,
        RichCompilerMechanismRef::SelectorTail,
        RichCompilerMechanismRef::SelectorHead,
        RichCompilerMechanismRef::AtomEqualityD1,
        RichCompilerMechanismRef::ConditionalD1,
        RichCompilerMechanismRef::PairConstruct,
        RichCompilerMechanismRef::Lambda,
        RichCompilerMechanismRef::Define,
    ] {
        assert!(
            mechanisms.contains(required.as_str()),
            "missing verified mechanism {}",
            required.as_str()
        );
    }
}

#[test]
fn carried_role_cannot_override_identity_from_real_export() {
    let text = run_pinned_sens_export();
    let mut requests = parse_compiler_export(&text).expect("parse real SENS compiler export");
    let request = requests
        .iter_mut()
        .find(|request| matches!(request.identity, sens::DomainIdentity::D3(_)))
        .expect("D3 request");

    request.lowering_role = sens::CompilerLoweringRole::DefineForm;
    assert!(matches!(
        verify_exported_request(request.clone()),
        Err(CompilerExportError::Verification(
            RichBridgeError::LoweringRoleMismatch
        ))
    ));
}

#[test]
fn stale_export_revision_is_rejected_after_transport_parse() {
    let text = run_pinned_sens_export();
    let mut requests = parse_compiler_export(&text).expect("parse real SENS compiler export");
    requests[0].provenance.revision = "0".repeat(40);

    assert!(matches!(
        verify_exported_request(requests.remove(0)),
        Err(CompilerExportError::Verification(
            RichBridgeError::Authority(
                cml::sens_domain_bridge::BridgeError::StaleAuthorityRevision
            )
        ))
    ));
}
