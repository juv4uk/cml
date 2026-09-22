use std::process::Command;

use cml::coverage::CoverageLedger;

fn run_report() -> Vec<u8> {
    let output = Command::new(env!("CARGO_BIN_EXE_cml-coverage"))
        .output()
        .expect("cml-coverage binary must execute");
    assert!(
        output.status.success(),
        "cml-coverage must exit successfully: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

#[test]
fn report_is_deterministic_and_matches_the_merged_ledger() {
    let first = run_report();
    let second = run_report();
    assert_eq!(first, second, "coverage report must be byte-deterministic");

    let text = String::from_utf8(first).expect("coverage report must be UTF-8");
    let ledger = CoverageLedger::supported_pin();
    let summary = ledger.summary();

    assert!(text.starts_with("schema\tcml-coverage/1\n"));
    assert!(text.contains("\nupstream-channel\tsupported-pin\n"));
    assert!(text.contains(&format!(
        "\nsemantic-identities\t{}\n",
        summary.semantic_identities
    )));
    assert!(text.contains(&format!(
        "\nsource-admitted\t{}\n",
        summary.source_admitted
    )));
    assert!(text.contains(&format!(
        "\nnot-yet-admitted\t{}\n",
        summary.not_yet_admitted
    )));

    let row_lines: Vec<_> = text.lines().filter(|line| line.starts_with("row\t")).collect();
    assert_eq!(row_lines.len(), ledger.rows.len());

    for line in &row_lines {
        let fields: Vec<_> = line.split('\t').collect();
        assert_eq!(
            fields.len(),
            6,
            "row must expose exactly schema tag + 5 stable columns: {line}"
        );
    }

    assert!(
        row_lines
            .iter()
            .any(|line| line.contains("\tnot-yet-admitted\t")),
        "report must preserve unsupported upstream identities"
    );
    assert!(
        row_lines.iter().all(|line| line.ends_with("\t-")),
        "foundation report must not fabricate backend evidence"
    );
}
