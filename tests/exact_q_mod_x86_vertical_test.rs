#![cfg(all(target_os = "linux", target_arch = "x86_64"))]

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use cml::elf64::Elf64Executable;
use cml::ir::Ir;
use cml::lisp_asm_vertical::select_arithmetic_slice;
use cml::machine_inst::{MachineInst, MachineItem, assemble_program};
use cml::{lower, parser};

fn upstream_corpus() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("external/my-lisp/tests/fixtures/conformance.lisp");
    fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "#111 requires pinned external/my-lisp conformance corpus at {}: {error}",
            path.display()
        )
    })
}

fn quoted_field(line: &str, key: &str) -> String {
    let marker = format!(r#"({key} . ""#);
    let tail = line
        .split_once(&marker)
        .unwrap_or_else(|| panic!("#111 upstream fixture is missing {key:?}: {line}"))
        .1;
    let end = tail
        .find(r#"")"#)
        .unwrap_or_else(|| panic!("#111 upstream fixture has malformed {key:?}: {line}"));
    tail[..end].to_string()
}

fn first_upstream_mod_witness() -> (String, i64) {
    let corpus = upstream_corpus();
    let line = corpus
        .lines()
        .find(|line| line.contains(r#"((expr . "(mod "#) && line.contains(r#"(expected . ""#))
        .expect("#111 requires at least one upstream mod fixture with an expected value");

    let source = quoted_field(line, "expr");
    let expected_text = quoted_field(line, "expected");
    let expected = expected_text.parse::<i64>().unwrap_or_else(|error| {
        panic!("#111 bounded integer mod witness expected must be an integer, got {expected_text:?}: {error}")
    });
    (source, expected)
}

fn lower_one(source: &str) -> Vec<Ir> {
    let expressions = parser::parse(source).unwrap_or_else(|error| {
        panic!("CML could not parse upstream/source witness `{source}`: {error:?}")
    });
    lower::lower_program(&expressions).unwrap_or_else(|error| {
        panic!("CML could not lower upstream/source witness `{source}`: {error}")
    })
}

#[test]
fn upstream_bounded_mod_reaches_native_x86_through_semantics_neutral_divreg() {
    let (source, upstream_expected) = first_upstream_mod_witness();
    let ir = lower_one(&source);

    let [Ir::App { func, args }] = ir.as_slice() else {
        panic!(
            "#111 semantic 1007 witness must lower as one generic builtin application, got {ir:?}"
        );
    };
    assert_eq!(func.as_ref(), &Ir::Builtin("mod".to_string()));
    assert_eq!(args.len(), 2);

    let machine_items = select_arithmetic_slice(&ir).unwrap_or_else(|error| {
        panic!("#111 bounded semantic-1007 witness must reach x86 target selection: {error}")
    });

    let div = machine_items.iter().find_map(|item| match item {
        MachineItem::Inst(MachineInst::DivReg { provenance, .. }) => Some(provenance),
        _ => None,
    });
    let div_provenance = div.expect("#111 executable mod slice must use the #101 DivReg mechanism");
    assert_eq!(
        div_provenance.semantic_id, None,
        "x86 divq remains a compiler mechanism and must not claim semantic identity 1007"
    );

    let direct_bytes =
        assemble_program(&machine_items).expect("#111 machine program must assemble");
    let elf = Elf64Executable::new(direct_bytes);

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos();
    let path = std::env::temp_dir().join(format!("cml-mod-x86-111-{nonce}"));
    elf.write_executable(&path)
        .expect("#111 must write native ELF witness");
    let output = Command::new(&path)
        .output()
        .expect("#111 must execute native ELF witness");
    let _ = fs::remove_file(&path);

    assert!(
        output.status.success(),
        "native mod witness failed: {output:?}"
    );
    assert_eq!(
        output.stdout.len(),
        8,
        "native witness must emit one tagged target word"
    );
    let tagged = u64::from_le_bytes(output.stdout[..8].try_into().unwrap());
    let actual = wsm_os_target::decode_fixnum(tagged)
        .expect("#111 bounded mod result must remain an exact target fixnum");

    assert_eq!(
        actual, upstream_expected,
        "CML result must match the expected value read from the pinned Lisp-owned fixture"
    );
}

#[test]
fn bounded_mod_specialization_fails_closed_outside_proven_domain() {
    for source in ["(mod -1 5)", "(mod 17 0)", "(mod (/ 1 2) 1)"] {
        let ir = lower_one(source);
        assert!(
            select_arithmetic_slice(&ir).is_err(),
            "#111 must not specialize unproven mod domain `{source}`"
        );
    }
}
