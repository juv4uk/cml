use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn quoted_field(text: &str, name: &str) -> Option<String> {
    let marker = format!("({name} . \"");
    let tail = text.split_once(&marker)?.1;
    Some(tail.split_once("\")")?.0.to_string())
}

fn head(path: &Path) -> String {
    let output = Command::new("git")
        .arg("-c")
        .arg(format!("safe.directory={}", path.display()))
        .arg("-C")
        .arg(path)
        .args(["rev-parse", "HEAD"])
        .output()
        .expect("git should be available for upstream revision-channel checks");
    assert!(
        output.status.success(),
        "git rev-parse failed for {}: {}",
        path.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("git SHA should be UTF-8")
        .trim()
        .to_owned()
}

fn sibling(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("cml should have a parent directory")
        .join(name)
}

#[test]
fn upstream_revision_channels_are_explicit_and_checkout_aligned() {
    let manifest = fs::read_to_string("upstream-revisions.lisp")
        .expect("#84 requires machine-readable upstream-revisions.lisp");

    assert!(
        manifest.contains("(kind . cml-upstream-revision-channels)"),
        "#84 revision manifest must declare its kind"
    );
    assert!(
        manifest.contains("(supported-pin-source . external/my-lisp-gitlink)"),
        "#84 supported-pin must be the checked-in external/my-lisp gitlink"
    );
    assert!(
        manifest.contains("(observed-current-source . exact-github-commit)"),
        "#84 observed-current must be an exact reproducible upstream commit"
    );

    let supported = quoted_field(&manifest, "supported-pin-sha")
        .expect("#84 manifest must declare supported-pin-sha");
    let observed = quoted_field(&manifest, "observed-current-sha")
        .expect("#84 manifest must declare observed-current-sha");
    assert_eq!(\n        supported.len(),\n        40,\n        "supported-pin-sha must be a full git SHA"\n    );
    assert_eq!(\n        observed.len(),\n        40,\n        "observed-current-sha must be a full git SHA"\n    );

    let supported_checkout = Path::new(env!("CARGO_MANIFEST_DIR")).join("external/my-lisp");
    assert_eq!(
        head(&supported_checkout),
        supported,
        "#84 supported-pin declaration must match the external/my-lisp gitlink checkout"
    );

    let observed_checkout = sibling("my-lisp");
    assert_eq!(
        head(&observed_checkout),
        observed,
        "#84 CI sibling my-lisp checkout must be the declared observed-current revision"
    );

    let compatibility = fs::read_to_string("compatibility.lisp")
        .expect("compatibility.lisp should be readable");
    assert!(
        compatibility.contains(&format!("(tested-sha . \"{supported}\")")),
        "#84 compatibility tested-sha must consume the canonical supported-pin"
    );
    assert!(
        compatibility.contains(&format!("(observed-upstream-sha . \"{observed}\")")),
        "#84 compatibility observed-upstream-sha must consume the canonical observed-current revision"
    );
}

#[test]
fn every_upstream_workload_names_its_observed_current_channel_and_exact_revision() {
    let workloads = fs::read_to_string("benchmarks/upstream-workloads.lisp")
        .expect("upstream workload manifest should be readable");

    let records = workloads.matches("(kind . cml-upstream-workload)").count();
    let channels = workloads
        .matches("(upstream-channel . observed-current)")
        .count();
    let exact_refs =
        workloads.matches("(upstream-ref . \"").count() + workloads.matches("(observed-ref . \"").count();

    assert!(\n        records > 0,\n        "expected at least one upstream workload record"\n    );
    assert_eq!(
        channels, records,
        "#84 every #80 workload record must name upstream-channel=observed-current"
    );
    assert_eq!(
        exact_refs, records,
        "#84 every #80 workload record must retain an exact upstream revision"
    );
}

#[test]
fn exact_rational_authority_witness_declares_supported_pin_channel() {
    let witness = fs::read_to_string("tests/exact_rational_upstream_authority_test.rs")
        .expect("exact-rational upstream authority witness should be readable");
    assert!(
        witness.contains("upstream-channel: supported-pin"),
        "#84 #105/#123 authority witness must state that it consumes supported-pin evidence"
    );
}
