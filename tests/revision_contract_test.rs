use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const FPGA_LISP_SHA: &str = "d1cb7eb79f675e8f2cc128c3b12918d6b08b9413";

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
    assert!(compatibility.contains("(isa . (1 4))"));
    assert!(
        compatibility
            .contains("evidence/FPGA-SHARED-ORACLE-PARITY-1/hardware-readback-2026-09-11.md")
    );
    assert!(
        compatibility
            .contains("evidence/FPGA-SHARED-ORACLE-PARITY-1/flash-cold-boot-2026-09-11.md")
    );

    let fpga_lisp = sibling("fpga-lisp");
    let fpga_lisp_head = head(&fpga_lisp);
    assert_eq!(
        fpga_lisp_head, FPGA_LISP_SHA,
        "fpga-lisp checkout must exactly match the reviewed CML target compatibility pin"
    );

    let isa_file = if fpga_lisp.join("isa-contract.lisp").exists() {
        fpga_lisp.join("isa-contract.lisp")
    } else {
        fpga_lisp.join("isa-contract.my")
    };
    let isa = fs::read_to_string(isa_file).expect("fpga-lisp ISA contract should be readable");
    assert!(
        isa.contains("(version . (1 4))"),
        "fpga-lisp ISA version must match the reviewed CML target contract"
    );
    assert!(
        isa.contains("(jf-branches-only-on . (nil))"),
        "fpga-lisp truth/JF contract drift"
    );

    let control = fs::read_to_string(fpga_lisp.join("fpga/rtl/control.sv"))
        .expect("reviewed fpga-lisp control RTL should be readable");
    assert!(
        control.contains("atom -> canonical Symbol(\"t\")")
            && control.contains("eq -> canonical Symbol(\"t\")")
            && control.matches("reg_wr_data.tag = TAG_SYMBOL;").count() >= 2
            && control.matches("reg_wr_data.value = 28'd79;").count() >= 2,
        "reviewed ATOM/EQ result representation drifted from canonical Symbol(t)"
    );

    let physical = fs::read_to_string(
        fpga_lisp.join("evidence/FPGA-SHARED-ORACLE-PARITY-1/hardware-readback-2026-09-11.md"),
    )
    .expect("reviewed physical ATOM/EQ evidence should be readable");
    assert!(
        physical.contains("(atom (quote radio))")
            && physical.contains("(eq (quote radio) (quote radio))")
            && physical.matches("SYMBOL(79) [0x2000004F]").count() >= 2,
        "physical CML-produced ATOM/EQ evidence no longer proves canonical Symbol(t)"
    );

    let cold_boot = fs::read_to_string(
        fpga_lisp.join("evidence/FPGA-SHARED-ORACLE-PARITY-1/flash-cold-boot-2026-09-11.md"),
    )
    .expect("reviewed FPGA cold-boot evidence should be readable");
    assert!(
        cold_boot.contains("corpus-02.bin")
            && cold_boot.contains("R15 = SYMBOL(79)  [0x2000004F]")
            && cold_boot.contains("ERR: no error"),
        "permanent-Flash cold-boot evidence drifted"
    );
}

/// Keep CML's supported contract separate from the contract observed through
/// the explicit observed-current revision channel.
#[test]
fn compatibility_my_contract_version_matches_observed_current_language_contract() {
    let my_lisp = sibling("sens");
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
