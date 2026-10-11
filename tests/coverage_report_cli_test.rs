use cml::coverage::CoverageLedger;
use std::process::Command;

fn run_report() -> Vec<u8> {
    let out = Command::new(env!("CARGO_BIN_EXE_cml-coverage"))
        .output()
        .expect("coverage report CLI must execute");
    assert!(
        out.status.success(),
        "coverage must fail closed rather than emit unverifiable data: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    out.stdout
}

#[test]
fn exact_domain_report_is_deterministic_and_matches_executable_sens_law() {
    let first = run_report();
    assert_eq!(first, run_report());
    let text = String::from_utf8(first).unwrap();
    let ledger = CoverageLedger::try_current_sens().unwrap();
    let summary = ledger.summary();

    assert!(text.starts_with("schema\tcml-coverage/2\n"));
    assert!(text.contains("\nupstream-channel\tcurrent-sens-executable-compiler-law\n"));
    assert!(text.contains(&format!(
        "\nauthority-sha256\t{}\n",
        ledger.authority_sha256
    )));
    assert!(text.contains("\ncompiler-role-identities\t9\n"));
    assert!(text.contains(&format!("\nsource-admitted\t{}\n", summary.source_admitted)));
    assert!(text.contains("\nnot-yet-admitted\t0\n"));
    assert!(text.contains("\nx86-executable\t0\n"));
    assert!(text.contains("\nx86-assembly-witness-only\t0\n"));
    assert!(text.contains("\nx86-representation-limited\t0\n"));

    let rows: Vec<_> = text
        .lines()
        .filter(|line| line.starts_with("row\t"))
        .collect();
    assert_eq!(rows.len(), 9);
    for line in rows {
        let fields: Vec<_> = line.split('\t').collect();
        assert_eq!(
            fields.len(),
            7,
            "schema v2: tag/domain/role/status/proof/native: {line}"
        );
        let coordinate = fields[1];
        if let Some(bits) = coordinate.strip_prefix("D3:") {
            assert_eq!(bits.len(), 3);
        } else if let Some(bits) = coordinate.strip_prefix("D4:") {
            assert_eq!(bits.len(), 4);
        } else {
            panic!("the compiler closure must not contain an unverified domain: {coordinate}");
        }
        assert!(line.contains("\tsource-admitted\t"));
        assert_eq!(
            fields[6], "-",
            "no historical byte-keyed x86 evidence may carry over"
        );
    }
}
