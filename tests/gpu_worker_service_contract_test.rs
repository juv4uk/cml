use std::fs;
use std::path::PathBuf;

fn unit_text() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("systemd")
        .join("cml-gpu-worker.service");
    fs::read_to_string(path).expect("read cml-gpu-worker.service")
}

#[test]
fn persistent_worker_unit_matches_live_lane_contract() {
    let unit = unit_text();

    assert!(unit.contains("User=agents"));
    assert!(unit.contains("RuntimeDirectory=cml-gpu-worker"));
    assert!(unit.contains("CML_GPU_WORKER_SOCKET=/run/cml-gpu-worker/worker.sock"));
    assert!(unit.contains(
        "CML_CUDA_HOST_PROBE=/home/agents/ecosystem/scripts/cuda-host-profile.sh"
    ));
    assert!(unit.contains("CUDA_HOME=/usr/local/cuda-12.6"));
    assert!(unit.contains(
        "LD_LIBRARY_PATH=/usr/lib/wsl/lib:/usr/local/cuda-12.6/targets/x86_64-linux/lib:/usr/local/lib/wsm-gcc-runtime"
    ));
    assert!(unit.contains("ExecStart=/home/agents/.local/bin/cml-gpu-worker serve"));
}

#[test]
fn persistent_worker_unit_is_restartable_and_fail_closed_on_missing_dependencies() {
    let unit = unit_text();

    assert!(unit.contains(
        "ConditionFileIsExecutable=/home/agents/.local/bin/cml-gpu-worker"
    ));
    assert!(unit.contains(
        "ConditionPathExists=/home/agents/ecosystem/scripts/cuda-host-profile.sh"
    ));
    assert!(unit.contains("Restart=always"));
    assert!(unit.contains("KillSignal=SIGINT"));
    assert!(unit.contains("After=network-online.target"));
}

#[test]
fn persistent_worker_unit_contains_no_registration_or_repository_secret() {
    let unit = unit_text().to_lowercase();

    for forbidden in [
        "registration_token",
        "registration-token",
        "github_token=",
        "github_pat_",
        "ghp_",
        "sens_read_token",
    ] {
        assert!(
            !unit.contains(forbidden),
            "system unit must not embed secret marker {forbidden}"
        );
    }
}
