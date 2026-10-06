use cml::gpu_admission::{AdmissionProvenance, GpuAdmissionGuard};
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn provenance(case_id: &str) -> AdmissionProvenance {
    AdmissionProvenance {
        repository: "juv4uk/cml".into(),
        run_id: format!("process-test-{}", std::process::id()),
        job: "gpu-admission-contention".into(),
        case_id: case_id.into(),
    }
}

fn temp_lock() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("cml-gpu-admission-process-{nonce}.lock"))
}

fn now_ns() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos()
}

#[test]
fn two_processes_never_hold_the_same_gpu_admission_concurrently() {
    if std::env::var_os("CML_GPU_ADMISSION_CHILD").is_some() {
        let lock = PathBuf::from(std::env::var("CML_GPU_ADMISSION_LOCK").unwrap());
        let case_id = std::env::var("CML_GPU_ADMISSION_CASE").unwrap();
        let hold_ms: u64 = std::env::var("CML_GPU_ADMISSION_HOLD_MS")
            .unwrap()
            .parse()
            .unwrap();
        let guard = GpuAdmissionGuard::acquire(&lock, "cuda:0", &provenance(&case_id)).unwrap();
        println!(
            "acquired_ns={} wait_ns={} case={case_id}",
            now_ns(),
            guard.lease().wait_ns
        );
        std::thread::sleep(Duration::from_millis(hold_ms));
        println!("released_ns={} case={case_id}", now_ns());
        return;
    }

    let lock = temp_lock();
    let owner = PathBuf::from(format!("{}.owner", lock.display()));
    let exe = std::env::current_exe().unwrap();
    let spawn_child = |case_id: &str| {
        Command::new(&exe)
            .arg("--exact")
            .arg("two_processes_never_hold_the_same_gpu_admission_concurrently")
            .arg("--nocapture")
            .env("CML_GPU_ADMISSION_CHILD", "1")
            .env("CML_GPU_ADMISSION_LOCK", &lock)
            .env("CML_GPU_ADMISSION_CASE", case_id)
            .env("CML_GPU_ADMISSION_HOLD_MS", "200")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap()
    };

    let mut first = spawn_child("first");
    for _ in 0..100 {
        if fs::read_to_string(&owner)
            .map(|s| s.contains("case_id=first"))
            .unwrap_or(false)
        {
            break;
        }
        thread::sleep(Duration::from_millis(5));
    }
    assert!(
        fs::read_to_string(&owner)
            .unwrap()
            .contains("case_id=first")
    );

    let mut second = spawn_child("second");
    let first_out = first.wait_with_output().unwrap();
    let second_out = second.wait_with_output().unwrap();
    assert!(
        first_out.status.success(),
        "first child failed: {}",
        String::from_utf8_lossy(&first_out.stderr)
    );
    assert!(
        second_out.status.success(),
        "second child failed: {}",
        String::from_utf8_lossy(&second_out.stderr)
    );
    let first_stdout = String::from_utf8_lossy(&first_out.stdout);
    let second_stdout = String::from_utf8_lossy(&second_out.stdout);
    let first_release = first_stdout
        .lines()
        .find(|line| line.starts_with("released_ns="))
        .unwrap();
    let second_acquire = second_stdout
        .lines()
        .find(|line| line.starts_with("acquired_ns="))
        .unwrap();
    let release_ns: u128 = first_release
        .split_whitespace()
        .next()
        .unwrap()
        .strip_prefix("released_ns=")
        .unwrap()
        .parse()
        .unwrap();
    let acquire_ns: u128 = second_acquire
        .split_whitespace()
        .next()
        .unwrap()
        .strip_prefix("acquired_ns=")
        .unwrap()
        .parse()
        .unwrap();
    let wait_line = second_stdout
        .lines()
        .find(|line| line.starts_with("acquired_ns="))
        .unwrap();
    let wait_ns: u128 = wait_line
        .split_whitespace()
        .find_map(|part| part.strip_prefix("wait_ns="))
        .unwrap()
        .parse()
        .unwrap();
    assert!(
        acquire_ns >= release_ns,
        "second holder acquired before first released: {acquire_ns} < {release_ns}"
    );
    assert!(
        wait_ns >= 100_000_000,
        "second holder did not materially wait: {wait_ns} ns"
    );

    let _ = fs::remove_file(&lock);
    let _ = fs::remove_file(&owner);
}
