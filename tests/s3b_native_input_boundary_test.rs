use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use cml::{lower, parser, x86_freestanding::X86FreestandingBackend};

#[test]
fn s3b_native_artifact_accepts_post_compile_input_word() {
    let expressions =
        parser::parse("(def bootstrap-entry (lambda (input) input))").expect("fixture must parse");
    let program = lower::lower_program(&expressions).expect("fixture must lower");
    let assembly = X86FreestandingBackend::new()
        .compile_program_with_input_entry(&program, "BOOTSTRAP-ENTRY")
        .expect("bounded unary wrapper must compile with explicit native input entry");

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let base = std::env::temp_dir().join(format!("cml-s3b-input-{}-{nonce}", std::process::id()));
    let source = base.with_extension("s");
    let harness = base.with_extension("c");
    let executable = base.with_extension("bin");

    fs::write(&source, assembly).unwrap();
    fs::write(
        &harness,
        r#"#include <stdint.h>
extern uint64_t wsm_entry_with_input(void *ctx, uint64_t input_word);

int main(void) {
    uint64_t first = wsm_entry_with_input(0, 339);  /* fixnum 42 */
    uint64_t second = wsm_entry_with_input(0, 59);  /* fixnum 7 */
    return (first == 339 && second == 59) ? 0 : 1;
}
"#,
    )
    .unwrap();

    let nucleus =
        cml::x86_freestanding::resolve_nucleus_asm_path().expect("resolve pinned nucleus");
    let linked = Command::new("cc")
        .arg("-no-pie")
        .arg(&harness)
        .arg(&source)
        .arg(&nucleus)
        .arg("-o")
        .arg(&executable)
        .output()
        .unwrap();

    let _ = fs::remove_file(&source);
    let _ = fs::remove_file(&harness);

    assert!(
        linked.status.success(),
        "S3b requires an explicit post-compile input entry boundary; linker stderr={}",
        String::from_utf8_lossy(&linked.stderr)
    );

    let run = Command::new(&executable).output().unwrap();
    let _ = fs::remove_file(&executable);

    assert!(
        run.status.success(),
        "the same native artifact must accept two distinct input values without recompilation; stdout={} stderr={}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
}
