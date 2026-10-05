#![cfg(feature = "gpu-cuda")]

use cml::gpu_host::{
    CudaHostCapability, CudaHostCapabilityError, CUDA_HOST_SCHEMA,
};

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
fn unavailable_host_fails_closed_before_cuda_session() {
    let record = ready_record().replace("CUDA_HOST_STATUS=ready", "CUDA_HOST_STATUS=unavailable:libcuda");
    let error = CudaHostCapability::parse_env_record(&record).unwrap_err();

    assert_eq!(
        error,
        CudaHostCapabilityError::HostUnavailable("unavailable:libcuda".into())
    );
}

#[test]
fn missing_nvrtc_has_distinct_diagnostic() {
    let record = ready_record().replace(
        "CUDA_NVRTC_LIBRARY_PRESENT=true",
        "CUDA_NVRTC_LIBRARY_PRESENT=false",
    );
    let error = CudaHostCapability::parse_env_record(&record).unwrap_err();

    assert_eq!(error, CudaHostCapabilityError::NvrtcLibraryUnavailable);
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
fn duplicate_keys_are_rejected() {
    let record = format!("{}\nCUDA_HOST_SCHEMA=sens-cuda-host-v1", ready_record());
    let error = CudaHostCapability::parse_env_record(&record).unwrap_err();

    assert_eq!(
        error,
        CudaHostCapabilityError::DuplicateKey("CUDA_HOST_SCHEMA".into())
    );
}
