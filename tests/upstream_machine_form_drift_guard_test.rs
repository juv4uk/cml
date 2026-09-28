use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn field(contract: &str, name: &str) -> String {
    let marker = format!("({name} \"");
    let tail = contract
        .split_once(&marker)
        .unwrap_or_else(|| panic!("bridge provenance missing {name}"))
        .1;
    tail.split_once('"')
        .map(|(value, _)| value.to_string())
        .expect("quoted provenance field must terminate")
}

#[test]
fn upstream_machine_form_bridge_pin_and_blob_fail_closed_on_drift() {
    let root = repo_root();
    let contract = fs::read_to_string(root.join("contracts/upstream-machine-form-bridge.lisp"))
        .expect("#74 bridge provenance contract must exist");

    let expected_revision = field(&contract, "revision-sha");
    let expected_blob = field(&contract, "contract-git-blob");
    let upstream = root.join("external/sens");

    let head = Command::new("git")
        .arg("-C")
        .arg(&upstream)
        .args(["rev-parse", "HEAD"])
        .output()
        .expect("git rev-parse supported pin")
        .stdout;
    let head = String::from_utf8(head).expect("git SHA must be utf8");
    assert_eq!(
        head.trim(),
        expected_revision,
        "#74 supported-pin movement requires explicit machine-form bridge review"
    );

    let blob = Command::new("git")
        .arg("-C")
        .arg(&upstream)
        .args(["hash-object", "lib/machine/lowering/semantic-x86-64.lisp"])
        .output()
        .expect("git hash-object lowering contract")
        .stdout;
    let blob = String::from_utf8(blob).expect("git blob SHA must be utf8");
    assert_eq!(
        blob.trim(),
        expected_blob,
        "#74 machine lowering contract drift must fail closed"
    );

    let lowering = fs::read_to_string(upstream.join("lib/machine/lowering/semantic-x86-64.lisp"))
        .expect("pinned upstream lowering contract must be readable");

    assert!(lowering.contains("(def x86-lower-add-u64-forms"));
    assert!(
        !lowering.contains("(x86-encode-program\n"),
        "semantic lowering must expose forms, not regain a byte-level bypass"
    );

    assert!(contract.contains("(raw-byte-authority cml-forbidden)"));
    assert!(contract.contains("(drift fail-closed)"));
}
