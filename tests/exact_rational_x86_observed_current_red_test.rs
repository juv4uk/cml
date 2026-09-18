#![cfg(all(target_os = "linux", target_arch = "x86_64"))]

// upstream-channel: observed-current
//
// #105 RED only: this witness proves the next native-x86 gap after hosted C
// exact-rational execution became evidence-backed. It deliberately carries no
// CML-authored expected Lisp value. Final execution/result parity is a later
// slice after representation admission is GREEN.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use cml::x86_freestanding::X86FreestandingBackend;
use cml::{lower, parser};

fn quoted_field(text: &str, name: &str) -> Option<String> {
    let marker = format!("({name} . \"");
    let tail = text.split_once(&marker)?.1;
    Some(tail.split_once("\")")?.0.to_string())
}

fn sibling(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("cml should have a parent directory")
        .join(name)
}

fn git_head(path: &Path) -> String {
    let output = Command::new("git")
        .arg("-c")
        .arg(format!("safe.directory={}", path.display()))
        .arg("-C")
        .arg(path)
        .args(["rev-parse", "HEAD"])
        .output()
        .expect("git should be available for #105 observed-current witness");
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

fn observed_current_exact_rational_sum_source() -> String {
    let manifest = fs::read_to_string("upstream-revisions.lisp")
        .expect("#105 requires #84 upstream revision channels");
    let declared_sha = quoted_field(&manifest, "observed-current-sha")
        .expect("#84 manifest must declare observed-current-sha");

    let observed_repo = sibling("my-lisp");
    assert_eq!(
        git_head(&observed_repo),
        declared_sha,
        "#105 must consume the exact #84 observed-current revision"
    );

    let fixture_path = observed_repo.join("tests/fixtures/mathematical-result-v1.lisp");
    let fixture = fs::read_to_string(&fixture_path).unwrap_or_else(|error| {
        panic!(
            "#105 requires observed-current mathematical-result fixture at {}: {error}",
            fixture_path.display()
        )
    });

    let block = fixture
        .split("\n\n")
        .find(|block| block.contains("(case . exact-rational-sum)"))
        .expect("#105 observed-current must expose the exact-rational-sum witness");

    quoted_field(block, "expr")
        .expect("#105 exact-rational-sum witness must contain an expr field")
}

#[test]
fn observed_current_exact_rational_sum_reaches_native_x86_representation() {
    let source = observed_current_exact_rational_sum_source();
    let expressions = parser::parse(&source)
        .unwrap_or_else(|error| panic!("parse observed-current source {source:?}: {error:?}"));
    let program = lower::lower_program(&expressions)
        .unwrap_or_else(|error| panic!("lower observed-current source {source:?}: {error}"));

    let assembly = X86FreestandingBackend::new()
        .compile_program(&program)
        .expect("#105 native x86 must admit an exact-rational representation");

    assert!(
        assembly.contains(".globl wsm_entry"),
        "#105 admitted exact-rational source must reach the real freestanding entrypoint"
    );
}
