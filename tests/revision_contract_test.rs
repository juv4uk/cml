use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const MY_LISP_SHA: &str = "6943f51afcdcd9bf41bf083e1a7ae2cd5aedbd3c";
const FPGA_LISP_SHA: &str = "36738759f281991646c45487ae6722fea561ea2b";

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

#[test]
fn checked_out_dependencies_match_the_compatibility_contract() {
    let compatibility = fs::read_to_string("compatibility.my")
        .expect("compatibility.my should be readable");
    assert!(compatibility.contains(&format!("(tested-sha . \"{MY_LISP_SHA}\")")));
    assert!(compatibility.contains(&format!("(tested-sha . \"{FPGA_LISP_SHA}\")")));
    assert!(compatibility.contains("(isa . (1 0))"));

    let my_lisp = sibling("my-lisp");
    let fpga_lisp = sibling("fpga-lisp");
    assert_eq!(head(&my_lisp), MY_LISP_SHA, "my-lisp revision drift");
    assert_eq!(head(&fpga_lisp), FPGA_LISP_SHA, "fpga-lisp revision drift");

    let isa = fs::read_to_string(fpga_lisp.join("isa-contract.my"))
        .expect("fpga-lisp ISA contract should be readable");
    assert!(isa.contains("(version . (1 0))"), "fpga-lisp ISA version drift");
    assert!(
        isa.contains("(jf-branches-only-on . (nil))"),
        "fpga-lisp truth/JF contract drift"
    );
}
