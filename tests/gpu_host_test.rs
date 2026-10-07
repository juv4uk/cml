#![cfg(feature = "gpu-cuda")]

use cml::gpu_host::{CUDA_HOST_SCHEMA, CudaHostCapability, CudaHostCapabilityError};

fn ready_record() -> String {
    [
        "CUDA_HOST_SCHEMA=sens-cuda-host-v1",
        "CUDA_HOST_STATUS=ready",
        "CUDA_HOST_KIND=wsl2",
        "CUDA_ROOT=/usr/local/cuda-12.6",
        "CUDA_TARGET=/usr/local/cuda-12.6/targets/x86_64-linux",
        "CUDA_INCLUDE=/usr/local/cuda-12.6/targets/x86_64-linux/include",
        "CUDA_TOOLKIT_LIB=/usr/local/cuda-12.6/targets/x86_64-linux/lib",
        "CUDA_DRIVER_LIB=/usr/lib/wsl/lib",
        "CUDA_NVRTC_LIB=/usr/local/cuda-12.6/targets/x86_64-linux/lib/libnvrtc.so.12",
        "NVIDIA_SMI=/usr/lib/wsl/lib/nvidia-smi",
        "CUDA_HEADER_PRESENT=true",
        "CUDA_DRIVER_PRESENT=true",
        "CUDA_DEVICE_VISIBLE=true",
        "CUDA_NVRTC_LIBRARY_PRESENT=true",
        "CUDA_DEVICE_NAME=NVIDIA GeForce GTX 1050 Ti",
        "CUDA_COMPUTE_CAPABILITY=6.1",
        "CUDA_DRIVER_VERSION=582.66",
        "CUDA_TOOLKIT_VERSION=12.6",
        "CUDA_NVCC_VERSION=Cuda compilation tools, release 12.6, V12.6.85",
    ]
    .join("\n")
}

#[test]
fn canonical_env_record_parses_without_cuda_hardware() {
    let capability = CudaHostCapability::parse_env_record(&ready_record()).unwrap();

    assert_eq!(capability.schema, CUDA_HOST_SCHEMA);
    assert_eq!(capability.status, "ready");
    assert_eq!(capability.host_kind, "wsl2");
    assert_eq!(capability.device_name, "NVIDIA GeForce GTX 1050 Ti");
    assert_eq!(capability.compute_capability, (6, 1));
    assert_eq!(capability.driver_version, "582.66");
    assert_eq!(capability.toolkit_version, "12.6");
    assert!(capability.nvrtc_library_present);
}

#[test]
fn nvcc_provenance_is_optional_when_nvrtc_is_present() {
    let record = ready_record()
        .replace("CUDA_TOOLKIT_VERSION=12.6\n", "")
        .replace(
            "CUDA_NVCC_VERSION=Cuda compilation tools, release 12.6, V12.6.85",
            "",
        );
    let capability = CudaHostCapability::parse_env_record(&record).unwrap();

    assert!(capability.nvrtc_library_present);
    assert!(capability.toolkit_version.is_empty());
    assert!(capability.nvcc_version.is_empty());
}

#[test]
fn unavailable_host_fails_closed_before_cuda_session() {
    let record = ready_record().replace(
        "CUDA_HOST_STATUS=ready",
        "CUDA_HOST_STATUS=unavailable:libcuda",
    );
    let error = CudaHostCapability::parse_env_record(&record).unwrap_err();

    assert_eq!(
        error,
        CudaHostCapabilityError::HostUnavailable("unavailable:libcuda".into())
    );
}

#[test]
fn missing_nvrtc_has_distinct_diagnostic() {
    let record = ready_record()
        .replace(
            "CUDA_NVRTC_LIB=/usr/local/cuda-12.6/targets/x86_64-linux/lib/libnvrtc.so.12",
            "CUDA_NVRTC_LIB=",
        )
        .replace(
            "CUDA_NVRTC_LIBRARY_PRESENT=true",
            "CUDA_NVRTC_LIBRARY_PRESENT=false",
        );
    let error = CudaHostCapability::parse_env_record(&record).unwrap_err();

    assert_eq!(error, CudaHostCapabilityError::NvrtcLibraryUnavailable);
}

#[test]
fn claimed_nvrtc_presence_requires_a_library_path() {
    let record = ready_record().replace(
        "CUDA_NVRTC_LIB=/usr/local/cuda-12.6/targets/x86_64-linux/lib/libnvrtc.so.12",
        "CUDA_NVRTC_LIB=",
    );
    let error = CudaHostCapability::parse_env_record(&record).unwrap_err();

    assert_eq!(
        error,
        CudaHostCapabilityError::MissingField("CUDA_NVRTC_LIB")
    );
}

#[test]
fn canonical_host_and_live_driver_must_name_same_device() {
    let capability = CudaHostCapability::parse_env_record(&ready_record()).unwrap();
    let error = capability
        .validate_live_device("Different GPU", (6, 1))
        .unwrap_err();

    assert_eq!(
        error,
        CudaHostCapabilityError::DeviceNameMismatch {
            host: "NVIDIA GeForce GTX 1050 Ti".into(),
            live: "Different GPU".into(),
        }
    );
}

#[test]
fn canonical_host_and_live_driver_must_have_same_compute_capability() {
    let capability = CudaHostCapability::parse_env_record(&ready_record()).unwrap();
    let error = capability
        .validate_live_device("NVIDIA GeForce GTX 1050 Ti", (7, 5))
        .unwrap_err();

    assert_eq!(
        error,
        CudaHostCapabilityError::ComputeCapabilityMismatch {
            host: (6, 1),
            live: (7, 5),
        }
    );
}

#[test]
fn host_capability_error_has_runtime_display_text() {
    use cml::gpu_cuda_runtime::CudaRuntimeError;

    let error =
        CudaRuntimeError::HostCapability(CudaHostCapabilityError::WrongSchema("old-schema".into()));
    let text = error.to_string();
    assert!(text.contains("CUDA host capability error"));
    assert!(text.contains("old-schema"));
}

#[test]
fn failed_strict_bootstrap_removes_stale_worker_socket_before_cuda_admission() {
    use std::fs;
    use std::os::unix::net::UnixListener;
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos();
    let base = std::env::temp_dir().join(format!(
        "cml-gpu-stale-socket-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&base).expect("create temp directory");
    let socket = base.join("worker.sock");
    let probe = base.join("fail-probe.sh");

    let listener = UnixListener::bind(&socket).expect("create stale unix socket path");
    drop(listener);
    assert!(socket.exists(), "precondition: stale socket path exists");

    fs::write(
        &probe,
        "#!/bin/sh\nprintf '%s\\n' 'CUDA_HOST_SCHEMA=sens-cuda-host-v1' 'CUDA_HOST_STATUS=unavailable:test'\n",
    )
    .expect("write failing host probe");

    let output = Command::new(env!("CARGO_BIN_EXE_cml-gpu-worker"))
        .arg("serve")
        .env("CML_GPU_WORKER_SOCKET", &socket)
        .env("CML_CUDA_HOST_PROBE", &probe)
        .output()
        .expect("run exact worker binary");

    assert!(
        !output.status.success(),
        "strict bootstrap must fail for unavailable host capability"
    );
    assert!(
        !socket.exists(),
        "failed strict bootstrap must not leave the stale worker socket path behind; stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let _ = fs::remove_dir_all(&base);
}

#[test]
fn duplicate_keys_are_rejected() {
    let record = format!("{}\nCUDA_HOST_SCHEMA=sens-cuda-host-v1", ready_record());
    let error = CudaHostCapability::parse_env_record(&record).unwrap_err();

    assert_eq!(
        error,
        CudaHostCapabilityError::DuplicateKey("CUDA_HOST_SCHEMA".into())
    );
}
