use std::process::Command;

use cml::compiler_passes::pass_manifest;

#[test]
fn cli_emits_the_ordered_machine_readable_pass_manifest() {
    let output = Command::new(env!("CARGO_BIN_EXE_cml-compile"))
        .arg("passes")
        .output()
        .expect("launch cml-compile passes");

    assert!(
        output.status.success(),
        "pass manifest command must succeed"
    );
    let stdout = String::from_utf8(output.stdout).expect("manifest output must be UTF-8");
    let lines: Vec<_> = stdout.lines().collect();

    assert_eq!(lines.len(), pass_manifest().len());

    for (line, pass) in lines.iter().zip(pass_manifest()) {
        assert!(line.starts_with("CML-PASS\t"));
        assert!(line.contains(&format!("\tid={}\t", pass.id)));
        assert!(line.contains(&format!("\tinput={}\t", pass.input)));
        assert!(line.contains(&format!("\toutput={}\t", pass.output)));
        assert!(line.contains(&format!("\tevidence={}\t", pass.current_evidence.as_str())));
        assert!(line.ends_with(&format!("obligation={}", pass.obligation)));
    }
}
