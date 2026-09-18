use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const FPGA_LISP_SHA: &str = "0351a6d535504e0790f0f4e115b69518605b142e";

fn sibling(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("cml should have a parent directory")
        .join(name)
}

fn head(path: &Path) -> String {
    let output = Command::new("git")
        .arg("-c")
        .arg(format!("safe.directory={}", path.display()))
        .arg("-C")
        .arg(path)
        .args(["rev-parse", "HEAD"])
        .output()
        .expect("git should be available for the revision contract");
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

// my-lisp revision identity is owned by upstream-revisions.lisp and checked by
// upstream_revision_channels_test.rs. This test keeps the language/ISA contract
// boundary honest without owning a second my-lisp SHA constant.
#[test]
fn checked_out_dependencies_match_the_compatibility_contract() {
    let compatibility = fs::read_to_string("compatibility.lisp")
        .expect("compatibility contract should be readable");

    assert!(
        compatibility.contains("(supported-revision-channel . supported-pin)"),
        "compatibility must name the canonical supported revision channel"
    );
    assert!(
        compatibility.contains("(observed-revision-channel . observed-current)"),
        "compatibility must name the canonical observed revision channel"
    );
    assert!(
        compatibility.contains(&format!("(tested-sha . \"{FPGA_LISP_SHA}\")")),
        "FPGA_LISP_SHA constant does not match the compatibility contract"
    );
    assert!(compatibility.contains("(isa . (1 1))"));

    let fpga_lisp = sibling("fpga-lisp");
    let fpga_lisp_head = head(&fpga_lisp);
    if fpga_lisp_head != FPGA_LISP_SHA {
        eprintln!(
            "note: fpga-lisp has moved since compatibility contract was last verified/pinned \
             (pinned {FPGA_LISP_SHA}, checked out {fpga_lisp_head})"
        );
    }

    let isa_file = if fpga_lisp.join("isa-contract.lisp").exists() {
        fpga_lisp.join("isa-contract.lisp")
    } else {
        fpga_lisp.join("isa-contract.my")
    };
    let isa = fs::read_to_string(isa_file).expect("fpga-lisp ISA contract should be readable");
    assert!(
        isa.contains("(version . (1 1))")
            || isa.contains("(version . (1 2))")
            || isa.contains("(version . (1 3))"),
        "fpga-lisp ISA version drift"
    );
    assert!(
        isa.contains("(jf-branches-only-on . (nil))"),
        "fpga-lisp truth/JF contract drift"
    );
}

/// Keep CML's supported contract separate from the contract observed through
/// the explicit observed-current revision channel.
#[test]
fn compatibility_my_contract_version_matches_observed_current_language_contract() {
    let my_lisp = sibling("my-lisp");
    let lang_file = if my_lisp.join("language-contract.lisp").exists() {
        my_lisp.join("language-contract.lisp")
    } else {
        my_lisp.join("language-contract.my")
    };
    let language_contract =
        fs::read_to_string(lang_file).expect("my-lisp language contract should be readable");

    let major = extract_field(&language_contract, "major")
        .expect("language contract should have a (major . N) field");
    let minor = extract_field(&language_contract, "minor")
        .expect("language contract should have a (minor . N) field");

    let compatibility = fs::read_to_string("compatibility.lisp")
        .expect("compatibility contract should be readable");
    let observed = format!("(observed-upstream-contract . ({major} {minor}))");
    assert!(
        compatibility.contains(&observed),
        "compatibility observed upstream contract does not match observed-current my-lisp: \
         expected (major . {major}) (minor . {minor})"
    );

    assert!(
        compatibility.contains("(contract . (2 0))"),
        "CML supported contract changed without updating this executable boundary"
    );
    assert!(
        compatibility.contains("(status . upgrade-required)"),
        "a supported/upstream contract mismatch must be represented explicitly"
    );
}

fn extract_field(text: &str, name: &str) -> Option<i64> {
    let marker = format!("({name} . ");
    let start = text.find(&marker)? + marker.len();
    let end = text[start..].find(')')? + start;
    text[start..end].trim().parse().ok()
}
