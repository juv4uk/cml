//! #116 cross-repo witness bridge.
//!
//! Upstream channel: supported-pin.
//! CML consumes the committed my-lisp conformance corpus from external/sens.
//! This adapter deliberately does not contain an expected value: semantic truth
//! stays in my-lisp's Lisp-authored witness row.

#![cfg(all(target_os = "linux", target_arch = "x86_64"))]

use std::fs;
use std::path::PathBuf;

use cml::sens_current_lowering::lower_current_sens_source;
use cml::witness_bridge::{execute_x86_actual, execute_x86_predicate_bit_actual};
use cml::x86_freestanding::X86FreestandingBackend;
use cml::{lower, parser};
use sens::{Session, eval_program, load_core_library};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn upstream_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("external/sens")
        .join(relative)
}

fn upstream_corpus() -> String {
    let path = upstream_path("tests/fixtures/conformance.lisp");
    fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "#116 requires the external/sens submodule's conformance corpus at {}: {error}",
            path.display()
        )
    })
}

fn first_compiler_witness_expr(corpus: &str) -> String {
    corpus
        .lines()
        .filter(|line| !line.trim_start().starts_with(';'))
        .find(|line| line.contains("(compiler-corpus . t)"))
        .and_then(|line| line.split_once("((expr . \"").map(|(_, tail)| tail))
        .and_then(|tail| tail.split_once("\")").map(|(expr, _)| expr.to_string()))
        .expect("#116 requires at least one compiler-corpus witness with an expr")
}

fn compiler_witness_row<'a>(corpus: &'a str, expr: &str) -> &'a str {
    let marker = format!("((expr . \"{expr}\")");
    corpus
        .lines()
        .filter(|line| !line.trim_start().starts_with(';'))
        .find(|line| line.contains(&marker) && line.contains("(compiler-corpus . t)"))
        .unwrap_or_else(|| panic!("#46 requires compiler-corpus witness {expr}"))
}

/// Obtain the current nine-role proof-carrying export from the pinned SENS
/// submodule. CML never derives a mechanism from a surface spelling or SID.
fn pinned_current_compiler_export() -> String {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let manifest = root.join("external/sens/Cargo.toml");
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos();
    let target = std::env::temp_dir().join(format!(
        "cml-d1-bridge-export-{}-{nonce}",
        std::process::id()
    ));
    let output = Command::new("cargo")
        .current_dir(root.join("external/sens"))
        .env("CARGO_TARGET_DIR", &target)
        .args([
            "run",
            "--quiet",
            "--manifest-path",
            manifest.to_str().expect("UTF-8 SENS manifest"),
            "-p",
            "xtask",
            "--",
            "compiler-export",
        ])
        .output()
        .expect("pinned SENS compiler-export must execute");
    let _ = fs::remove_dir_all(&target);
    assert!(
        output.status.success(),
        "pinned SENS compiler-export failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("SENS export is UTF-8")
}

fn lisp_owned_verdict_passes(row: &str, actual: &str) {
    let runner_path = upstream_path("tests/fixtures/witness-runner.lisp");
    let runner = fs::read_to_string(&runner_path).unwrap_or_else(|error| {
        panic!(
            "#46 requires Lisp-owned witness verdict protocol at {}: {error}",
            runner_path.display()
        )
    });

    let mut session = Session::default();
    load_core_library(&mut session).expect("pinned my-lisp core library must load");
    eval_program(&runner, &mut session).expect("pinned witness-runner.lisp must load");

    let verdict = eval_program(
        &format!("(witness-status (witness-verdict (quote {row}) (quote {actual})))"),
        &mut session,
    )
    .expect("Lisp-owned witness verdict must execute")
    .value
    .to_string();

    assert_eq!(
        verdict, "pass",
        "semantic PASS must come from Lisp-owned witness-status, actual={actual}"
    );
}

#[test]
fn cml_consumes_upstream_lisp_witness_without_own_expected_answer() {
    let corpus = upstream_corpus();
    let source = first_compiler_witness_expr(&corpus);

    let expressions = parser::parse(&source).unwrap_or_else(|error| {
        panic!("CML could not parse upstream witness `{source}`: {error:?}")
    });
    let program = lower::lower_program(&expressions)
        .unwrap_or_else(|error| panic!("CML could not lower upstream witness `{source}`: {error}"));
    let assembly = X86FreestandingBackend::new()
        .compile_program(&program)
        .unwrap_or_else(|error| {
            panic!("CML could not compile upstream witness `{source}`: {error:?}")
        });

    assert!(
        assembly.contains(".globl wsm_entry"),
        "upstream witness must reach the real x86 freestanding backend"
    );
}

#[test]
fn executed_cml_actual_is_judged_only_by_lisp_owned_witness_logic() {
    // Select the committed compiler-corpus row by source expression only.
    // Its expected outcome is never copied into this Rust test.
    let corpus = upstream_corpus();
    let source = "(00000010 (quote radio))";
    let row = compiler_witness_row(&corpus, source);

    // The exact source is parsed and lowered by pinned SENS itself. Its
    // Ukrainian surface identifies the current D3 ATOM/QUOTE forms. We do
    // NOT execute the corpus's historical eight-bit spelling as a SID.
    let current_source = "(атом? (як-є radio))";
    let export = pinned_current_compiler_export();
    let current = lower_current_sens_source(current_source, &export)
        .expect("SENS-verified current ATOM source must lower in CML");
    let assembly = X86FreestandingBackend::new()
        .compile_program(&current.ir)
        .expect("current D1 ATOM mechanism must compile to freestanding x86");

    let actual = execute_x86_predicate_bit_actual(&assembly)
        .expect("target runtime must verify the exact D1 PredicateBit carrier");
    lisp_owned_verdict_passes(row, &actual);

    // Negative control: the old SID/truthiness path must NOT be accepted as
    // current D1, even if its historical result is the symbol t.
    let expressions =
        parser::parse(source).expect("historical compiler-corpus witness must remain parseable");
    let program = lower::lower_program(&expressions)
        .expect("historical compiler-corpus witness must remain lowerable");
    let old_assembly = X86FreestandingBackend::new()
        .compile_program(&program)
        .expect("historical witness still compiles as compatibility evidence");
    assert!(
        execute_x86_predicate_bit_actual(&old_assembly).is_err(),
        "historical Symbol(t) is not an exact D1 PredicateBit"
    );
}

#[test]
fn current_d1_atom_no_is_judged_by_lisp_owned_witness() {
    // A structural pair is not an atom. Both the compiler mechanism and
    // the outcome remain owned by pinned SENS; CML only transports exact D1.
    let corpus = upstream_corpus();
    let source = "(00000010 (quote (radio antenna)))";
    let row = compiler_witness_row(&corpus, source);
    let current_source = "(атом? (як-є (radio antenna)))";
    let export = pinned_current_compiler_export();
    let current = lower_current_sens_source(current_source, &export)
        .expect("verified SENS D1 ATOM must admit a structural list operand");
    let assembly = X86FreestandingBackend::new()
        .compile_program(&current.ir)
        .expect("verified D1 ATOM on structural pair must compile");
    let actual = execute_x86_predicate_bit_actual(&assembly)
        .expect("target runtime must validate D1:0 without coercing NIL or t");
    lisp_owned_verdict_passes(row, &actual);
}
