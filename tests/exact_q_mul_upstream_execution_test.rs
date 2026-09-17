use std::fs;
use std::path::PathBuf;
use std::process::Command;

use cml::c_backend::CBackend;
use cml::{lower, parser};

fn upstream_mul_witness() -> (String, String) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("external/my-lisp/tests/fixtures/conformance.lisp");
    let corpus = fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "#103 requires the pinned my-lisp conformance corpus at {}: {error}",
            path.display()
        )
    });

    let line = corpus
        .lines()
        .find(|line| line.contains("(* (/ 2 3) (/ 9 4))"))
        .expect("#103 requires the Lisp-owned exact multiplication witness");

    fn field(line: &str, name: &str) -> String {
        let marker = format!("({name} . \"");
        line.split_once(&marker)
            .and_then(|(_, tail)| tail.split_once("\")").map(|(value, _)| value.to_string()))
            .unwrap_or_else(|| panic!("#103 witness row must contain quoted field {name:?}"))
    }

    (field(line, "expr"), field(line, "expected"))
}

fn compile_and_run(source: &str) -> String {
    let expressions = parser::parse(source).expect("upstream multiplication witness must parse");
    let program = lower::lower_program_with_first_class_builtins(&expressions)
        .expect("upstream multiplication witness must lower");
    let c_source = CBackend::new()
        .compile_program(&program)
        .expect("C backend must compile the admitted multiplication witness");

    let stem = format!("exact_q_mul_upstream_{}", std::process::id());
    let c_path = format!("{stem}.c");
    let bin_path = stem;
    fs::write(&c_path, c_source).expect("write generated C");

    let compile = Command::new("gcc")
        .arg(&c_path)
        .arg("-o")
        .arg(&bin_path)
        .output()
        .expect("run gcc");
    if !compile.status.success() {
        let _ = fs::remove_file(&c_path);
        panic!("gcc failed: {}", String::from_utf8_lossy(&compile.stderr));
    }

    let run = Command::new(format!("./{bin_path}"))
        .output()
        .expect("run generated multiplication witness");
    let _ = fs::remove_file(&c_path);
    let _ = fs::remove_file(&bin_path);
    assert!(
        run.status.success(),
        "compiled multiplication witness failed: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    String::from_utf8(run.stdout)
        .expect("witness output must be UTF-8")
        .trim()
        .to_string()
}

#[test]
fn admitted_semantic_1002_reuses_existing_exact_c_mechanism_against_upstream_witness() {
    let (source, expected) = upstream_mul_witness();
    let actual = compile_and_run(&source);
    assert_eq!(
        actual, expected,
        "CML must not own the multiplication answer; actual output is compared only with the pinned Lisp-authored witness"
    );
}
