use cml::coverage::{AdmissionState, CoverageLedger};

fn field(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            other => out.push(other),
        }
    }
    out
}

fn main() {
    let ledger = CoverageLedger::supported_pin();
    let summary = ledger.summary();

    println!("schema\tcml-coverage/1");
    println!("upstream-channel\t{}", ledger.upstream_channel);
    println!("registry-fnv1a64\t{:016x}", ledger.registry_digest_fnv1a64);
    println!("semantic-identities\t{}", summary.semantic_identities);
    println!("source-admitted\t{}", summary.source_admitted);
    println!("not-yet-admitted\t{}", summary.not_yet_admitted);
    println!("columns\tsemantic-id\tadmission\toperation-status\tevidence\tbackend-evidence");

    for row in &ledger.rows {
        let admission = match row.admission {
            AdmissionState::SourceAdmitted => "source-admitted",
            AdmissionState::NotYetAdmitted => "not-yet-admitted",
        };
        let operation_status = row.operation_status.unwrap_or("-");
        let evidence = row.evidence.map(field).unwrap_or_else(|| "-".to_string());
        let backend_evidence = row
            .backend_evidence
            .map(field)
            .unwrap_or_else(|| "-".to_string());

        println!(
            "row\t{}\t{}\t{}\t{}\t{}",
            row.semantic_id,
            admission,
            field(operation_status),
            evidence,
            backend_evidence
        );
    }
}
