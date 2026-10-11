use cml::coverage::{AdmissionState, BackendEvidenceState, CoverageLedger};

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

fn backend_state(state: BackendEvidenceState) -> &'static str {
    match state {
        BackendEvidenceState::AssemblyWitness => "assembly-witness",
        BackendEvidenceState::Executable => "executable",
        BackendEvidenceState::ExecutableRepresentationLimited => {
            "executable-representation-limited"
        }
    }
}

fn main() {
    let ledger = CoverageLedger::build_source();
    let summary = ledger.summary();

    println!("schema\tcml-coverage/2");
    println!("upstream-channel\t{}", ledger.upstream_channel);
    println!("authority-sha256\t{}", ledger.authority_sha256);
    println!(
        "compiler-role-identities\t{}",
        summary.compiler_role_identities
    );
    println!("source-admitted\t{}", summary.source_admitted);
    println!("not-yet-admitted\t{}", summary.not_yet_admitted);
    println!("x86-executable\t{}", summary.x86_executable);
    println!(
        "x86-assembly-witness-only\t{}",
        summary.x86_assembly_witness_only
    );
    println!(
        "x86-representation-limited\t{}",
        summary.x86_representation_limited
    );
    println!(
        "columns\tdomain-identity\tcompiler-role\tadmission\toperation-status\tproof-ref\tbackend-evidence"
    );

    for row in &ledger.rows {
        let admission = match row.admission {
            AdmissionState::SourceAdmitted => "source-admitted",
            AdmissionState::NotYetAdmitted => "not-yet-admitted",
        };
        let operation_status = row.operation_status;
        let evidence = field(row.evidence);
        let backend_evidence = if row.backend_evidence.is_empty() {
            "-".to_string()
        } else {
            row.backend_evidence
                .iter()
                .map(|evidence| {
                    let limit = evidence
                        .representation_limit
                        .map(field)
                        .unwrap_or_else(|| "-".to_string());
                    format!(
                        "{}:{}:{}:{}",
                        field(evidence.backend),
                        backend_state(evidence.state),
                        field(evidence.evidence),
                        limit
                    )
                })
                .collect::<Vec<_>>()
                .join("|")
        };

        println!(
            "row\tD{}:{:0width$b}\t{:?}\t{}\t{}\t{}\t{}",
            row.identity.width(),
            row.identity.packed_bits(),
            row.execution_role,
            admission,
            field(operation_status),
            evidence,
            backend_evidence,
            width = row.identity.width() as usize
        );
    }
}
