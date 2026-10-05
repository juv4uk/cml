//! Mechanical adapter for the ecosystem-owned CUDA host capability contract.
//!
//! Authority: juv4uk/ecosystem#58.
//! This module consumes host facts only. It does not perform CML semantic
//! admission and it does not decide whether GPU execution is profitable.

use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const CUDA_HOST_SCHEMA: &str = "sens-cuda-host-v1";
pub const CUDA_HOST_PROBE_ENV: &str = "CML_CUDA_HOST_PROBE";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CudaHostCapability {
    pub schema: String,
    pub status: String,
    pub host_kind: String,
    pub cuda_root: PathBuf,
    pub cuda_target: PathBuf,
    pub cuda_include: PathBuf,
    pub cuda_toolkit_lib: PathBuf,
    pub cuda_driver_lib: PathBuf,
    pub cuda_nvrtc_lib: PathBuf,
    pub nvidia_smi: PathBuf,
    pub header_present: bool,
    pub driver_present: bool,
    pub device_visible: bool,
    pub nvrtc_library_present: bool,
    pub device_name: String,
    pub compute_capability: (i32, i32),
    pub driver_version: String,
    pub toolkit_version: String,
    pub nvcc_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CudaHostCapabilityError {
    ProbeNotConfigured,
    ProbeSpawn { path: PathBuf, message: String },
    ProbeFailed { path: PathBuf, code: Option<i32>, stderr: String },
    InvalidLine(String),
    DuplicateKey(String),
    MissingField(&'static str),
    InvalidBoolean { field: &'static str, value: String },
    InvalidComputeCapability(String),
    WrongSchema(String),
    HostUnavailable(String),
    NvrtcLibraryUnavailable,
    DeviceNameMismatch { host: String, live: String },
    ComputeCapabilityMismatch { host: (i32, i32), live: (i32, i32) },
}

impl CudaHostCapability {
    /// Resolve the canonical ecosystem probe without making CML its owner.
    ///
    /// Priority:
    /// 1. explicit CML_CUDA_HOST_PROBE;
    /// 2. WSM_ECOSYSTEM_ROOT/scripts/cuda-host-profile.sh;
    /// 3. the owner-host default /home/agents/ecosystem/scripts/...
    pub fn configured_probe_path() -> Result<PathBuf, CudaHostCapabilityError> {
        if let Some(path) = env::var_os(CUDA_HOST_PROBE_ENV) {
            return Ok(PathBuf::from(path));
        }

        if let Some(root) = env::var_os("WSM_ECOSYSTEM_ROOT") {
            return Ok(PathBuf::from(root).join("scripts/cuda-host-profile.sh"));
        }

        let default = PathBuf::from("/home/agents/ecosystem/scripts/cuda-host-profile.sh");
        if default.is_file() {
            Ok(default)
        } else {
            Err(CudaHostCapabilityError::ProbeNotConfigured)
        }
    }

    pub fn probe_configured() -> Result<Self, CudaHostCapabilityError> {
        let path = Self::configured_probe_path()?;
        Self::probe_command(path)
    }

    /// Execute the canonical probe in its stable key=value mode.
    pub fn probe_command(path: impl AsRef<Path>) -> Result<Self, CudaHostCapabilityError> {
        let path = path.as_ref();
        let output = Command::new("bash")
            .arg(path)
            .arg("env")
            .output()
            .map_err(|error| CudaHostCapabilityError::ProbeSpawn {
                path: path.to_path_buf(),
                message: error.to_string(),
            })?;

        if !output.status.success() {
            return Err(CudaHostCapabilityError::ProbeFailed {
                path: path.to_path_buf(),
                code: output.status.code(),
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            });
        }

        let text = String::from_utf8_lossy(&output.stdout);
        Self::parse_env_record(&text)
    }

    pub fn parse_env_record(text: &str) -> Result<Self, CudaHostCapabilityError> {
        let mut fields = HashMap::<String, String>::new();
        for raw in text.lines() {
            let line = raw.trim_end();
            if line.is_empty() {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                return Err(CudaHostCapabilityError::InvalidLine(line.to_string()));
            };
            if key.is_empty() {
                return Err(CudaHostCapabilityError::InvalidLine(line.to_string()));
            }
            if fields.insert(key.to_string(), value.to_string()).is_some() {
                return Err(CudaHostCapabilityError::DuplicateKey(key.to_string()));
            }
        }

        fn required(
            fields: &HashMap<String, String>,
            key: &'static str,
        ) -> Result<String, CudaHostCapabilityError> {
            fields
                .get(key)
                .cloned()
                .filter(|value| !value.is_empty())
                .ok_or(CudaHostCapabilityError::MissingField(key))
        }

        fn optional(fields: &HashMap<String, String>, key: &'static str) -> String {
            fields.get(key).cloned().unwrap_or_default()
        }

        fn boolean(
            fields: &HashMap<String, String>,
            key: &'static str,
        ) -> Result<bool, CudaHostCapabilityError> {
            let value = required(fields, key)?;
            match value.as_str() {
                "true" => Ok(true),
                "false" => Ok(false),
                _ => Err(CudaHostCapabilityError::InvalidBoolean { field: key, value }),
            }
        }

        let schema = required(&fields, "CUDA_HOST_SCHEMA")?;
        if schema != CUDA_HOST_SCHEMA {
            return Err(CudaHostCapabilityError::WrongSchema(schema));
        }

        let status = required(&fields, "CUDA_HOST_STATUS")?;
        if status != "ready" {
            return Err(CudaHostCapabilityError::HostUnavailable(status));
        }

        let compute_text = required(&fields, "CUDA_COMPUTE_CAPABILITY")?;
        let Some((major, minor)) = compute_text.split_once('.') else {
            return Err(CudaHostCapabilityError::InvalidComputeCapability(
                compute_text,
            ));
        };
        let compute_capability = (
            major
                .parse::<i32>()
                .map_err(|_| CudaHostCapabilityError::InvalidComputeCapability(compute_text.clone()))?,
            minor
                .parse::<i32>()
                .map_err(|_| CudaHostCapabilityError::InvalidComputeCapability(compute_text.clone()))?,
        );

        let capability = Self {
            schema,
            status,
            host_kind: required(&fields, "CUDA_HOST_KIND")?,
            cuda_root: PathBuf::from(required(&fields, "CUDA_ROOT")?),
            cuda_target: PathBuf::from(required(&fields, "CUDA_TARGET")?),
            cuda_include: PathBuf::from(required(&fields, "CUDA_INCLUDE")?),
            cuda_toolkit_lib: PathBuf::from(required(&fields, "CUDA_TOOLKIT_LIB")?),
            cuda_driver_lib: PathBuf::from(required(&fields, "CUDA_DRIVER_LIB")?),
            cuda_nvrtc_lib: PathBuf::from(required(&fields, "CUDA_NVRTC_LIB")?),
            nvidia_smi: PathBuf::from(required(&fields, "NVIDIA_SMI")?),
            header_present: boolean(&fields, "CUDA_HEADER_PRESENT")?,
            driver_present: boolean(&fields, "CUDA_DRIVER_PRESENT")?,
            device_visible: boolean(&fields, "CUDA_DEVICE_VISIBLE")?,
            nvrtc_library_present: boolean(&fields, "CUDA_NVRTC_LIBRARY_PRESENT")?,
            device_name: required(&fields, "CUDA_DEVICE_NAME")?,
            compute_capability,
            driver_version: required(&fields, "CUDA_DRIVER_VERSION")?,
            toolkit_version: optional(&fields, "CUDA_TOOLKIT_VERSION"),
            nvcc_version: optional(&fields, "CUDA_NVCC_VERSION"),
        };

        if !capability.header_present {
            return Err(CudaHostCapabilityError::HostUnavailable(
                "unavailable:cuda.h".to_string(),
            ));
        }
        if !capability.driver_present {
            return Err(CudaHostCapabilityError::HostUnavailable(
                "unavailable:libcuda".to_string(),
            ));
        }
        if !capability.device_visible {
            return Err(CudaHostCapabilityError::HostUnavailable(
                "unavailable:device".to_string(),
            ));
        }
        if !capability.nvrtc_library_present {
            return Err(CudaHostCapabilityError::NvrtcLibraryUnavailable);
        }

        Ok(capability)
    }

    /// Cross-check canonical host facts against the live cudarc session.
    ///
    /// Driver-version encodings differ between nvidia-smi and CUDA driver API,
    /// so those values are both preserved as provenance rather than compared
    /// numerically. Device identity and compute capability must agree exactly.
    pub fn validate_live_device(
        &self,
        live_name: &str,
        live_compute_capability: (i32, i32),
    ) -> Result<(), CudaHostCapabilityError> {
        if self.device_name != live_name {
            return Err(CudaHostCapabilityError::DeviceNameMismatch {
                host: self.device_name.clone(),
                live: live_name.to_string(),
            });
        }
        if self.compute_capability != live_compute_capability {
            return Err(CudaHostCapabilityError::ComputeCapabilityMismatch {
                host: self.compute_capability,
                live: live_compute_capability,
            });
        }
        Ok(())
    }

    pub fn machine_evidence(&self) -> String {
        format!(
            "schema={} status={} host_kind={} device_name={} compute_capability={}.{} driver_version={} toolkit_version={} nvrtc_library={}",
            self.schema,
            self.status,
            self.host_kind,
            self.device_name,
            self.compute_capability.0,
            self.compute_capability.1,
            self.driver_version,
            self.toolkit_version,
            self.cuda_nvrtc_lib.display(),
        )
    }
}
