//! Shared physical-GPU admission primitive for CML and external bounded clients.
//!
//! The lease is a kernel-backed advisory lock. It does not execute commands and
//! does not carry semantic authority. A crashed holder releases the lock with
//! the owning file descriptor, while the small owner record is overwritten by
//! the next admitted client.

#![cfg(unix)]

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::time::Instant;

const LOCK_EX: i32 = 2;
const LOCK_UN: i32 = 8;
const MAX_FIELD_BYTES: usize = 128;

unsafe extern "C" {
    fn flock(fd: i32, operation: i32) -> i32;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdmissionProvenance {
    pub repository: String,
    pub run_id: String,
    pub job: String,
    pub case_id: String,
}

impl AdmissionProvenance {
    pub fn validate(&self) -> Result<(), String> {
        validate_field("repository", &self.repository)?;
        validate_field("run_id", &self.run_id)?;
        validate_field("job", &self.job)?;
        validate_field("case_id", &self.case_id)
    }
}

fn validate_field(name: &str, value: &str) -> Result<(), String> {
    if value.is_empty() {
        return Err(format!("admission {name} must not be empty"));
    }
    if value.len() > MAX_FIELD_BYTES {
        return Err(format!("admission {name} exceeds {MAX_FIELD_BYTES} bytes"));
    }
    if !value.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b':' | b'/' | b'-')) {
        return Err(format!("admission {name} contains unsupported characters"));
    }
    Ok(())
}

#[derive(Debug)]
pub struct GpuAdmissionGuard {
    file: File,
    lock_path: PathBuf,
    owner_path: PathBuf,
    lease: AdmissionLease,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdmissionLease {
    pub wait_ns: u64,
    pub resource_key: String,
}

impl GpuAdmissionGuard {
    pub fn acquire(
        lock_path: impl Into<PathBuf>,
        resource_key: impl Into<String>,
        provenance: &AdmissionProvenance,
    ) -> Result<Self, String> {
        provenance.validate()?;
        let lock_path = lock_path.into();
        if let Some(parent) = lock_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("create admission directory {}: {error}", parent.display()))?;
        }
        let owner_path = PathBuf::from(format!("{}.owner", lock_path.display()));
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(&lock_path)
            .map_err(|error| format!("open GPU admission lock {}: {error}", lock_path.display()))?;
        let started = Instant::now();
        let rc = unsafe { flock(file.as_raw_fd(), LOCK_EX) };
        if rc != 0 {
            return Err(format!("acquire GPU admission lock {} failed: {}", lock_path.display(), std::io::Error::last_os_error()));
        }
        let wait_ns = started.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64;
        let resource_key = resource_key.into();
        validate_field("resource_key", &resource_key)?;
        let owner_record = format!(
            "repository={}\nrun_id={}\njob={}\ncase_id={}\nresource_key={}\nwait_ns={}\n",
            provenance.repository, provenance.run_id, provenance.job, provenance.case_id, resource_key, wait_ns
        );
        fs::write(&owner_path, owner_record)
            .map_err(|error| format!("write GPU admission owner {}: {error}", owner_path.display()))?;
        Ok(Self {
            file,
            lock_path,
            owner_path,
            lease: AdmissionLease { wait_ns, resource_key },
        })
    }

    pub fn lease(&self) -> &AdmissionLease {
        &self.lease
    }

    pub fn lock_path(&self) -> &Path {
        &self.lock_path
    }
}

impl Drop for GpuAdmissionGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.owner_path);
        let _ = unsafe { flock(self.file.as_raw_fd(), LOCK_UN) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_lock() -> PathBuf {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        std::env::temp_dir().join(format!("cml-gpu-admission-{nonce}.lock"))
    }

    fn provenance(id: &str) -> AdmissionProvenance {
        AdmissionProvenance {
            repository: "juv4uk/cml".into(),
            run_id: "test-run".into(),
            job: "admission".into(),
            case_id: id.into(),
        }
    }

    #[test]
    fn lock_is_exclusive_and_reports_wait() {
        let path = temp_lock();
        let first = GpuAdmissionGuard::acquire(&path, "cuda:0", &provenance("first")).unwrap();
        let path2 = path.clone();
        let handle = thread::spawn(move || GpuAdmissionGuard::acquire(&path2, "cuda:0", &provenance("second")).unwrap().lease().wait_ns);
        thread::sleep(std::time::Duration::from_millis(20));
        drop(first);
        let wait_ns = handle.join().unwrap();
        assert!(wait_ns >= 1_000_000);
        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(format!("{}.owner", path.display()));
    }

    #[test]
    fn provenance_is_bounded() {
        let mut p = provenance("ok");
        p.job = "x".repeat(MAX_FIELD_BYTES + 1);
        assert!(p.validate().is_err());
    }
}
