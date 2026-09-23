use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn quoted_field(text: &str, name: &str) -> Option<String> {
    let marker = format!("({name} . \\\"");
    let tail = text.split_once(&marker)?.1;
    Some(tail.split_once("\\\")")?.0.to_string())
}

fn git_head(path: &Path) -> Option<String> {
    let output = Command::new("git")
        .arg("-c")
        .arg(format!("safe.directory={}", path.display()))
        .arg("-C")
        .arg(path)
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()?;
    output.status.success().then(|| {
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    })
}

/// Exact root for tests that claim upstream-channel: supported-pin.
///
/// CI supplies CML_SUPPORTED_MY_LISP_ROOT from the independent supported
/// checkout. Local fallback is allowed only when external/my-lisp actually
/// matches the declared supported SHA; otherwise fail closed rather than
/// silently testing build-source while claiming supported compatibility.
pub fn supported_my_lisp_root() -> PathBuf {
    if let Some(root) = std::env::var_os("CML_SUPPORTED_MY_LISP_ROOT") {
        return PathBuf::from(root);
    }

    let manifest = fs::read_to_string("upstream-revisions.lisp")
        .expect("supported-path resolution requires upstream-revisions.lisp");
    let supported = quoted_field(&manifest, "supported-pin-sha")
        .expect("revision manifest must contain supported-pin-sha");
    let external = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("external/my-lisp");
    let actual = git_head(&external)
        .expect("external/my-lisp must be a git checkout or CML_SUPPORTED_MY_LISP_ROOT must be set");
    assert_eq!(
        actual, supported,
        "external/my-lisp is build-source, not the declared supported-pin; set CML_SUPPORTED_MY_LISP_ROOT to the exact supported checkout"
    );
    external
}

pub fn supported_path(relative: &str) -> PathBuf {
    supported_my_lisp_root().join(relative)
}
