//! Optional NVIDIA CUDA execution for admitted map regions.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use cudarc::driver::{CudaContext, CudaFunction, CudaSlice, LaunchConfig, PushKernelArg};

use crate::accelerator::{
    AcceleratorApi, AcceleratorClass, AcceleratorDescriptor, AcceleratorVendor,
};
use crate::compute::{
    ComputeBackend, CpuComputeBackend, I32Range, fuse_i32_map_chain, i32_buffer_range,
    prove_i32_map_range,
};
use crate::gpu_cuda::{
    CudaArtifactCache, CudaCacheDiagnosticEvidence, CudaCompilerTarget, CudaComputeCapability,
    CudaDriverJitCacheKey, CudaDriverJitModuleArtifact, CudaElementType, CudaEmitError,
    CudaLatencyBreakdown, CudaMapKernel, CudaPtxArtifact, CudaPtxCacheKey, NvrtcVersion,
    emit_i32_compute_kernel, lower_map_kernel,
};
use crate::ir::{BufferLiteral, Ir};

/// Exact toolchain provenance used to compile and load CUDA artifacts in one session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CudaToolchainProvenance {
    pub nvrtc_version: NvrtcVersion,
    pub driver_version: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CudaDevice {
    pub descriptor: AcceleratorDescriptor,
    pub ordinal: usize,
    pub compute_capability: (i32, i32),
    pub total_memory_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CudaExecution {
    pub output: BufferLiteral,
    pub device: CudaDevice,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CudaChainExecution {
    pub outputs: Vec<BufferLiteral>,
    pub device: CudaDevice,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CudaSelectedChainExecution {
    /// Host-materialized outputs paired with their zero-based chain step.
    pub outputs: Vec<(usize, BufferLiteral)>,
    pub device: CudaDevice,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CudaDriverStage {
    DeviceCount,
    ContextCreate { ordinal: usize },
    ContextBind,
    DeviceEvidence,
    MemoryQuery,
    Allocation,
    HostToDeviceCopy,
    ModuleLoad,
    FunctionLoad,
    Launch,
    DeviceToHostCopy,
    InternalState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CudaCapabilityStatus {
    RuntimePresentNoDevices,
    Live(Vec<CudaDevice>),
}

#[derive(Debug)]
pub enum CudaRuntimeError {
    Emit(CudaEmitError),
    UnsupportedInput,
    InsufficientDeviceMemory {
        required_bytes: usize,
        free_bytes: usize,
        reserve_bytes: usize,
        effective_available_bytes: usize,
    },
    InvalidMaterializationStep {
        step: usize,
        chain_len: usize,
    },
    Nvrtc(String),
    Driver {
        stage: CudaDriverStage,
        message: String,
    },
}

#[derive(Debug)]
pub struct CudaChainRuntimeError {
    pub step: usize,
    pub source: CudaRuntimeError,
}

impl From<CudaEmitError> for CudaRuntimeError {
    fn from(error: CudaEmitError) -> Self {
        Self::Emit(error)
    }
}

impl std::fmt::Display for CudaRuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Emit(err) => write!(f, "CUDA emit error: {err:?}"),
            Self::UnsupportedInput => write!(f, "unsupported input"),
            Self::InsufficientDeviceMemory {
                required_bytes,
                free_bytes,
                reserve_bytes,
                effective_available_bytes,
            } => write!(
                f,
                "insufficient device memory: required={required_bytes} bytes, free={free_bytes} bytes, reserve={reserve_bytes} bytes, effective_available={effective_available_bytes} bytes"
            ),
            Self::InvalidMaterializationStep { step, chain_len } => {
                write!(f, "invalid step {step} for chain of length {chain_len}")
            }
            Self::Nvrtc(msg) => write!(f, "NVRTC error: {msg}"),
            Self::Driver { stage, message } => {
                write!(f, "CUDA driver error at {stage:?}: {message}")
            }
        }
    }
}

impl CudaRuntimeError {
    fn driver(stage: CudaDriverStage, error: impl ToString) -> Self {
        Self::Driver {
            stage,
            message: error.to_string(),
        }
    }
}

pub fn preflight_device_memory(
    required_bytes: usize,
    free_bytes: usize,
    reserve_bytes: usize,
) -> Result<usize, CudaRuntimeError> {
    let effective_available = free_bytes.saturating_sub(reserve_bytes);
    if required_bytes > effective_available {
        Err(CudaRuntimeError::InsufficientDeviceMemory {
            required_bytes,
            free_bytes,
            reserve_bytes,
            effective_available_bytes: effective_available,
        })
    } else {
        Ok(effective_available)
    }
}

pub fn cuda_memory_reserve_from_env() -> usize {
    std::env::var("CML_CUDA_MEMORY_RESERVE_BYTES")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0)
}

fn nvrtc_arch_from_compute_capability((major, minor): (i32, i32)) -> String {
    format!("compute_{major}{minor}")
}

/// NVRTC switch that disables FMA contraction (`a*b+c` stays mul+add).
///
/// Ratified precondition A1 of the sens bitwise f32 witness (sens#1585,
/// 2026-09-28): in the witness slice both sides must round twice, so the
/// CUDA side must not let NVRTC contract separate mul+add into FFMA.
pub const NVRTC_NO_FMA_CONTRACTION: &str = "-fmad=false";

/// Named NVRTC compile mode for map kernels (cml#360).
///
/// The variant name `BitwiseEquality` is the cross-repo contract: the sens
/// witness (sens#1585 E1, `experiments/gpu2-e1e3/witness.py`) references
/// this name as a stable constant and never reads cml's implementation.
/// `Production` keeps NVRTC defaults (FMA contraction allowed); kernels
/// compiled under different modes never share a cache slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CudaKernelMode {
    /// Default production mode: NVRTC defaults, FMA contraction allowed.
    Production,
    /// Witness mode: NVRTC also receives `-fmad=false` so `a*b+c` lowers
    /// to separate mul+add with two roundings, matching CPU semantics.
    BitwiseEquality,
}

impl Default for CudaKernelMode {
    fn default() -> Self {
        Self::Production
    }
}

/// NVRTC options for a kernel compiled in `mode` for `compute_capability`.
/// Pure mechanism: no device access, unit-testable without CUDA hardware.
pub fn nvrtc_options_for(mode: CudaKernelMode, compute_capability: (i32, i32)) -> Vec<String> {
    let arch = nvrtc_arch_from_compute_capability(compute_capability);
    match mode {
        CudaKernelMode::Production => vec![format!("-arch={arch}")],
        CudaKernelMode::BitwiseEquality => {
            vec![
                format!("-arch={arch}"),
                NVRTC_NO_FMA_CONTRACTION.to_string(),
            ]
        }
    }
}

#[derive(Debug)]
struct ReusableBuffers<T> {
    len: usize,
    input: CudaSlice<T>,
    output: CudaSlice<T>,
}

/// Long-lived CUDA mechanism state for one device.
///
/// This owns no language semantics. It only keeps driver objects that are
/// expensive to reconstruct between equivalent admitted executions:
/// one CUDA context and compiled functions keyed by the emitted kernel source.
#[derive(Debug)]
pub struct CudaSession {
    context: Arc<CudaContext>,
    device: CudaDevice,
    toolchain: CudaToolchainProvenance,
    artifact_cache: Mutex<CudaArtifactCache>,
    kernels: Mutex<HashMap<(CudaKernelMode, String), CudaFunction>>,
    i32_buffers: Mutex<Option<ReusableBuffers<i32>>>,
    f32_buffers: Mutex<Option<ReusableBuffers<f32>>>,
    memory_reserve_bytes: AtomicUsize,
}

/// A semantic admission witness tied to the exact immutable IR that was
/// admitted. The borrow prevents the input buffer from being mutated or
/// replaced while repeated CUDA executions reuse this witness.
#[derive(Debug)]
pub struct PreparedCudaMap<'a> {
    session: &'a CudaSession,
    function: CudaFunction,
    buffer: &'a BufferLiteral,
}

impl PreparedCudaMap<'_> {
    pub fn execute(&self) -> Result<CudaExecution, CudaRuntimeError> {
        self.session
            .execute_prepared_buffer(&self.function, self.buffer)
    }
}

impl CudaSession {
    pub fn new(device_ordinal: usize) -> Result<Self, CudaRuntimeError> {
        let context = CudaContext::new(device_ordinal).map_err(|error| {
            CudaRuntimeError::driver(
                CudaDriverStage::ContextCreate {
                    ordinal: device_ordinal,
                },
                error,
            )
        })?;
        let device = device_evidence(&context)?;
        let (nvrtc_major, nvrtc_minor) = query_nvrtc_version()?;
        let toolchain = CudaToolchainProvenance {
            nvrtc_version: NvrtcVersion::new(nvrtc_major as i32, nvrtc_minor as i32),
            driver_version: query_driver_version()?,
        };
        let reserve_bytes = cuda_memory_reserve_from_env();
        Ok(Self {
            context,
            device,
            toolchain,
            artifact_cache: Mutex::new(CudaArtifactCache::default()),
            kernels: Mutex::new(HashMap::new()),
            i32_buffers: Mutex::new(None),
            f32_buffers: Mutex::new(None),
            memory_reserve_bytes: AtomicUsize::new(reserve_bytes),
        })
    }

    pub fn with_memory_reserve(
        device_ordinal: usize,
        reserve_bytes: usize,
    ) -> Result<Self, CudaRuntimeError> {
        let session = Self::new(device_ordinal)?;
        session.set_memory_reserve_bytes(reserve_bytes);
        Ok(session)
    }

    pub fn memory_reserve_bytes(&self) -> usize {
        self.memory_reserve_bytes.load(Ordering::Relaxed)
    }

    pub fn set_memory_reserve_bytes(&self, reserve_bytes: usize) {
        self.memory_reserve_bytes
            .store(reserve_bytes, Ordering::Relaxed);
    }

    pub fn device(&self) -> &CudaDevice {
        &self.device
    }

    /// Exact mechanism/toolchain provenance captured when this session was created.
    pub fn toolchain_provenance(&self) -> CudaToolchainProvenance {
        self.toolchain
    }

    /// Number of distinct executable map kernels currently resident in this
    /// session. Exposed as mechanism evidence for reuse tests/benchmarks.
    pub fn cached_kernel_count(&self) -> Result<usize, CudaRuntimeError> {
        self.kernels
            .lock()
            .map(|kernels| kernels.len())
            .map_err(|error| {
                CudaRuntimeError::driver(
                    CudaDriverStage::InternalState,
                    format!("kernel cache mutex poisoned: {error}"),
                )
            })
    }

    /// Diagnostic evidence for artifact cache hits, misses, and evictions.
    pub fn cache_diagnostics(&self) -> Result<CudaCacheDiagnosticEvidence, CudaRuntimeError> {
        self.artifact_cache
            .lock()
            .map(|cache| cache.diagnostics())
            .map_err(|error| {
                CudaRuntimeError::driver(
                    CudaDriverStage::InternalState,
                    format!("artifact cache mutex poisoned: {error}"),
                )
            })
    }

    /// Compile an admitted IR through an explicit CML compiler target.
    ///
    /// The target selects a compilation mechanism only; semantic admission
    /// remains in the canonical IR/CML analysis layer.
    pub fn compile_target<'a>(
        &'a self,
        target: CudaCompilerTarget,
        ir: &'a Ir,
        mode: CudaKernelMode,
    ) -> Result<PreparedCudaMap<'a>, CudaRuntimeError> {
        match target {
            CudaCompilerTarget::NvidiaDriverJit => self.prepare_map_with_mode_inner(ir, mode),
        }
    }

    pub fn prepare_map<'a>(&'a self, ir: &'a Ir) -> Result<PreparedCudaMap<'a>, CudaRuntimeError> {
        self.compile_target(
            CudaCompilerTarget::NvidiaDriverJit,
            ir,
            CudaKernelMode::Production,
        )
    }

    /// Witness variant of [`CudaSession::prepare_map`]: the kernel is
    /// compiled by NVRTC with `-fmad=false` (cml#360, sens#1585 A1).
    pub fn prepare_map_with_mode<'a>(
        &'a self,
        ir: &'a Ir,
        mode: CudaKernelMode,
    ) -> Result<PreparedCudaMap<'a>, CudaRuntimeError> {
        self.compile_target(CudaCompilerTarget::NvidiaDriverJit, ir, mode)
    }

    fn prepare_map_with_mode_inner<'a>(
        &'a self,
        ir: &'a Ir,
        mode: CudaKernelMode,
    ) -> Result<PreparedCudaMap<'a>, CudaRuntimeError> {
        let kernel = lower_map_kernel(ir)?;
        let buffer = map_input(ir).ok_or(CudaRuntimeError::UnsupportedInput)?;
        if matches!(buffer, BufferLiteral::I32(values) if values.is_empty())
            || matches!(buffer, BufferLiteral::F32(values) if values.is_empty())
        {
            return Err(CudaRuntimeError::UnsupportedInput);
        }

        let function = self.function_for_kernel(mode, &kernel)?;
        Ok(PreparedCudaMap {
            session: self,
            function,
            buffer,
        })
    }

    pub fn execute_map(&self, ir: &Ir) -> Result<CudaExecution, CudaRuntimeError> {
        self.prepare_map(ir)?.execute()
    }

    /// Witness variant of [`CudaSession::execute_map`]: the kernel is
    /// compiled by NVRTC with `-fmad=false` (cml#360, sens#1585 A1).
    pub fn execute_map_with_mode(
        &self,
        ir: &Ir,
        mode: CudaKernelMode,
    ) -> Result<CudaExecution, CudaRuntimeError> {
        self.prepare_map_with_mode(ir, mode)?.execute()
    }

    fn execute_prepared_buffer(
        &self,
        function: &CudaFunction,
        buffer: &BufferLiteral,
    ) -> Result<CudaExecution, CudaRuntimeError> {
        let stream = self.context.default_stream();

        let output = match buffer {
            BufferLiteral::I32(input) => {
                let length =
                    u32::try_from(input.len()).map_err(|_| CudaRuntimeError::UnsupportedInput)?;
                let mut cache = self.i32_buffers.lock().map_err(|error| {
                    CudaRuntimeError::driver(
                        CudaDriverStage::InternalState,
                        format!("i32 buffer cache mutex poisoned: {error}"),
                    )
                })?;
                if cache.as_ref().map(|buffers| buffers.len) != Some(input.len()) {
                    *cache = Some(ReusableBuffers {
                        len: input.len(),
                        input: unsafe { stream.alloc::<i32>(input.len()) }.map_err(|error| {
                            CudaRuntimeError::driver(CudaDriverStage::Allocation, error)
                        })?,
                        output: unsafe { stream.alloc::<i32>(input.len()) }.map_err(|error| {
                            CudaRuntimeError::driver(CudaDriverStage::Allocation, error)
                        })?,
                    });
                }
                let buffers = cache.as_mut().expect("buffer cache initialized");
                stream
                    .memcpy_htod(input.as_slice(), &mut buffers.input)
                    .map_err(|error| {
                        CudaRuntimeError::driver(CudaDriverStage::HostToDeviceCopy, error)
                    })?;
                unsafe {
                    stream
                        .launch_builder(function)
                        .arg(&buffers.input)
                        .arg(&mut buffers.output)
                        .arg(&length)
                        .launch(LaunchConfig::for_num_elems(length))
                }
                .map_err(|error| CudaRuntimeError::driver(CudaDriverStage::Launch, error))?;
                let mut output = vec![0i32; input.len()];
                stream
                    .memcpy_dtoh(&buffers.output, &mut output)
                    .map_err(|error| {
                        CudaRuntimeError::driver(CudaDriverStage::DeviceToHostCopy, error)
                    })?;
                BufferLiteral::I32(output)
            }
            BufferLiteral::F32(bits) => {
                let input: Vec<f32> = bits.iter().copied().map(f32::from_bits).collect();
                let length =
                    u32::try_from(input.len()).map_err(|_| CudaRuntimeError::UnsupportedInput)?;
                let mut cache = self.f32_buffers.lock().map_err(|error| {
                    CudaRuntimeError::driver(
                        CudaDriverStage::InternalState,
                        format!("f32 buffer cache mutex poisoned: {error}"),
                    )
                })?;
                if cache.as_ref().map(|buffers| buffers.len) != Some(input.len()) {
                    *cache = Some(ReusableBuffers {
                        len: input.len(),
                        input: unsafe { stream.alloc::<f32>(input.len()) }.map_err(|error| {
                            CudaRuntimeError::driver(CudaDriverStage::Allocation, error)
                        })?,
                        output: unsafe { stream.alloc::<f32>(input.len()) }.map_err(|error| {
                            CudaRuntimeError::driver(CudaDriverStage::Allocation, error)
                        })?,
                    });
                }
                let buffers = cache.as_mut().expect("buffer cache initialized");
                stream
                    .memcpy_htod(input.as_slice(), &mut buffers.input)
                    .map_err(|error| {
                        CudaRuntimeError::driver(CudaDriverStage::HostToDeviceCopy, error)
                    })?;
                unsafe {
                    stream
                        .launch_builder(function)
                        .arg(&buffers.input)
                        .arg(&mut buffers.output)
                        .arg(&length)
                        .launch(LaunchConfig::for_num_elems(length))
                }
                .map_err(|error| CudaRuntimeError::driver(CudaDriverStage::Launch, error))?;
                let mut output = vec![0f32; input.len()];
                stream
                    .memcpy_dtoh(&buffers.output, &mut output)
                    .map_err(|error| {
                        CudaRuntimeError::driver(CudaDriverStage::DeviceToHostCopy, error)
                    })?;
                BufferLiteral::F32(output.into_iter().map(f32::to_bits).collect())
            }
        };

        Ok(CudaExecution {
            output,
            device: self.device.clone(),
        })
    }

    /// Execute a linear i32 map chain while intermediate buffers remain on the
    /// selected CUDA device. Every step is re-admitted from a carried range
    /// proof; no kernel is allowed to bypass the checked-add overflow guard.
    ///
    /// Compatibility path: every chain step is materialized on the host.
    pub fn execute_map_chain_i32(
        &self,
        functions: &[Ir],
        input: &BufferLiteral,
    ) -> Result<CudaChainExecution, CudaChainRuntimeError> {
        let requested_steps: Vec<usize> = (0..functions.len()).collect();
        let selected = self.execute_map_chain_i32_selected(functions, input, &requested_steps)?;
        Ok(CudaChainExecution {
            outputs: selected
                .outputs
                .into_iter()
                .map(|(_, output)| output)
                .collect(),
            device: selected.device,
        })
    }

    /// Execute a linear i32 map chain and materialize only selected chain steps.
    ///
    /// Device storage is bounded to the original input plus two ping-pong
    /// scratch buffers, independent of chain length. requested_steps is a host
    /// execution policy only; it does not alter SENS semantics or admission.
    pub fn execute_map_chain_i32_selected(
        &self,
        functions: &[Ir],
        input: &BufferLiteral,
        requested_steps: &[usize],
    ) -> Result<CudaSelectedChainExecution, CudaChainRuntimeError> {
        let fail = |step, source| CudaChainRuntimeError { step, source };
        let BufferLiteral::I32(input_values) = input else {
            return Err(fail(0, CudaRuntimeError::UnsupportedInput));
        };
        if functions.is_empty() || input_values.is_empty() {
            return Err(fail(0, CudaRuntimeError::UnsupportedInput));
        }

        let requested: HashSet<usize> = requested_steps.iter().copied().collect();
        if let Some(&step) = requested.iter().find(|&&step| step >= functions.len()) {
            return Err(fail(
                step,
                CudaRuntimeError::InvalidMaterializationStep {
                    step,
                    chain_len: functions.len(),
                },
            ));
        }

        let final_step = functions.len() - 1;
        if functions.len() > 1
            && requested.len() == 1
            && requested.contains(&final_step)
            && let Some(fused) = self.try_execute_fused_i32_final(functions, input)?
        {
            return Ok(fused);
        }

        let length = u32::try_from(input_values.len())
            .map_err(|_| fail(0, CudaRuntimeError::UnsupportedInput))?;
        let element_bytes = input_values
            .len()
            .checked_mul(std::mem::size_of::<i32>())
            .ok_or_else(|| fail(0, CudaRuntimeError::UnsupportedInput))?;
        // One immutable uploaded input plus two ping-pong scratch buffers.
        let required_bytes = element_bytes
            .checked_mul(3)
            .ok_or_else(|| fail(0, CudaRuntimeError::UnsupportedInput))?;

        self.context.bind_to_thread().map_err(|error| {
            fail(
                0,
                CudaRuntimeError::driver(CudaDriverStage::ContextBind, error),
            )
        })?;
        let (free_bytes, _) = cudarc::driver::result::mem_get_info().map_err(|error| {
            fail(
                0,
                CudaRuntimeError::driver(CudaDriverStage::MemoryQuery, error),
            )
        })?;
        preflight_device_memory(required_bytes, free_bytes, self.memory_reserve_bytes())
            .map_err(|error| fail(0, error))?;

        // Re-admit the complete chain before the first kernel launch.
        let mut range =
            i32_buffer_range(input).ok_or_else(|| fail(0, CudaRuntimeError::UnsupportedInput))?;
        let mut prepared = Vec::with_capacity(functions.len());
        for (step, function_ir) in functions.iter().enumerate() {
            let next_range = prove_i32_map_range(function_ir, range)
                .ok_or_else(|| fail(step, CudaRuntimeError::UnsupportedInput))?;
            let probe = i32_range_probe_ir(function_ir, range);
            let kernel = lower_map_kernel(&probe)
                .map_err(|error| fail(step, CudaRuntimeError::Emit(error)))?;
            let function = self
                .function_for_kernel(CudaKernelMode::Production, &kernel)
                .map_err(|error| fail(step, error))?;
            prepared.push(function);
            range = next_range;
        }

        let stream = self.context.default_stream();
        let input_device = stream
            .clone_htod(input_values.as_slice())
            .map_err(|error| {
                fail(
                    0,
                    CudaRuntimeError::driver(CudaDriverStage::HostToDeviceCopy, error),
                )
            })?;
        let mut ping = unsafe { stream.alloc::<i32>(input_values.len()) }.map_err(|error| {
            fail(
                0,
                CudaRuntimeError::driver(CudaDriverStage::Allocation, error),
            )
        })?;
        let mut pong = unsafe { stream.alloc::<i32>(input_values.len()) }.map_err(|error| {
            fail(
                0,
                CudaRuntimeError::driver(CudaDriverStage::Allocation, error),
            )
        })?;

        let mut outputs = Vec::with_capacity(requested.len());
        for (step, function) in prepared.iter().enumerate() {
            if step == 0 {
                unsafe {
                    stream
                        .launch_builder(function)
                        .arg(&input_device)
                        .arg(&mut ping)
                        .arg(&length)
                        .launch(LaunchConfig::for_num_elems(length))
                }
                .map_err(|error| {
                    fail(
                        step,
                        CudaRuntimeError::driver(CudaDriverStage::Launch, error),
                    )
                })?;
                if requested.contains(&step) {
                    let mut output = vec![0i32; input_values.len()];
                    stream.memcpy_dtoh(&ping, &mut output).map_err(|error| {
                        fail(
                            step,
                            CudaRuntimeError::driver(CudaDriverStage::DeviceToHostCopy, error),
                        )
                    })?;
                    outputs.push((step, BufferLiteral::I32(output)));
                }
            } else if step % 2 == 1 {
                unsafe {
                    stream
                        .launch_builder(function)
                        .arg(&ping)
                        .arg(&mut pong)
                        .arg(&length)
                        .launch(LaunchConfig::for_num_elems(length))
                }
                .map_err(|error| {
                    fail(
                        step,
                        CudaRuntimeError::driver(CudaDriverStage::Launch, error),
                    )
                })?;
                if requested.contains(&step) {
                    let mut output = vec![0i32; input_values.len()];
                    stream.memcpy_dtoh(&pong, &mut output).map_err(|error| {
                        fail(
                            step,
                            CudaRuntimeError::driver(CudaDriverStage::DeviceToHostCopy, error),
                        )
                    })?;
                    outputs.push((step, BufferLiteral::I32(output)));
                }
            } else {
                unsafe {
                    stream
                        .launch_builder(function)
                        .arg(&pong)
                        .arg(&mut ping)
                        .arg(&length)
                        .launch(LaunchConfig::for_num_elems(length))
                }
                .map_err(|error| {
                    fail(
                        step,
                        CudaRuntimeError::driver(CudaDriverStage::Launch, error),
                    )
                })?;
                if requested.contains(&step) {
                    let mut output = vec![0i32; input_values.len()];
                    stream.memcpy_dtoh(&ping, &mut output).map_err(|error| {
                        fail(
                            step,
                            CudaRuntimeError::driver(CudaDriverStage::DeviceToHostCopy, error),
                        )
                    })?;
                    outputs.push((step, BufferLiteral::I32(output)));
                }
            }
        }

        Ok(CudaSelectedChainExecution {
            outputs,
            device: self.device.clone(),
        })
    }

    fn try_execute_fused_i32_final(
        &self,
        functions: &[Ir],
        input: &BufferLiteral,
    ) -> Result<Option<CudaSelectedChainExecution>, CudaChainRuntimeError> {
        let fail = |step, source| CudaChainRuntimeError { step, source };
        let BufferLiteral::I32(input_values) = input else {
            return Ok(None);
        };
        let Some(input_range) = i32_buffer_range(input) else {
            return Ok(None);
        };
        let Some((kernel, _output_range)) = fuse_i32_map_chain(functions, input_range) else {
            return Ok(None);
        };

        let final_step = functions.len() - 1;
        let source = emit_i32_compute_kernel(&kernel)
            .map_err(|error| fail(final_step, CudaRuntimeError::Emit(error)))?;
        let function = self
            .function_for_source(CudaKernelMode::Production, source)
            .map_err(|error| fail(final_step, error))?;

        let length = u32::try_from(input_values.len())
            .map_err(|_| fail(0, CudaRuntimeError::UnsupportedInput))?;
        let element_bytes = input_values
            .len()
            .checked_mul(std::mem::size_of::<i32>())
            .ok_or_else(|| fail(0, CudaRuntimeError::UnsupportedInput))?;
        let required_bytes = element_bytes
            .checked_mul(2)
            .ok_or_else(|| fail(0, CudaRuntimeError::UnsupportedInput))?;

        self.context.bind_to_thread().map_err(|error| {
            fail(
                0,
                CudaRuntimeError::driver(CudaDriverStage::ContextBind, error),
            )
        })?;
        let (free_bytes, _) = cudarc::driver::result::mem_get_info().map_err(|error| {
            fail(
                0,
                CudaRuntimeError::driver(CudaDriverStage::MemoryQuery, error),
            )
        })?;
        preflight_device_memory(required_bytes, free_bytes, self.memory_reserve_bytes())
            .map_err(|error| fail(0, error))?;

        let stream = self.context.default_stream();
        let input_device = stream
            .clone_htod(input_values.as_slice())
            .map_err(|error| {
                fail(
                    0,
                    CudaRuntimeError::driver(CudaDriverStage::HostToDeviceCopy, error),
                )
            })?;
        let mut output_device =
            unsafe { stream.alloc::<i32>(input_values.len()) }.map_err(|error| {
                fail(
                    final_step,
                    CudaRuntimeError::driver(CudaDriverStage::Allocation, error),
                )
            })?;

        unsafe {
            stream
                .launch_builder(&function)
                .arg(&input_device)
                .arg(&mut output_device)
                .arg(&length)
                .launch(LaunchConfig::for_num_elems(length))
        }
        .map_err(|error| {
            fail(
                final_step,
                CudaRuntimeError::driver(CudaDriverStage::Launch, error),
            )
        })?;

        let mut output = vec![0i32; input_values.len()];
        stream
            .memcpy_dtoh(&output_device, &mut output)
            .map_err(|error| {
                fail(
                    final_step,
                    CudaRuntimeError::driver(CudaDriverStage::DeviceToHostCopy, error),
                )
            })?;

        Ok(Some(CudaSelectedChainExecution {
            outputs: vec![(final_step, BufferLiteral::I32(output))],
            device: self.device.clone(),
        }))
    }

    /// Compile and load an admitted kernel artifact, consulting the PTX and module caches.
    pub fn function_for_kernel(
        &self,
        mode: CudaKernelMode,
        kernel: &CudaMapKernel,
    ) -> Result<CudaFunction, CudaRuntimeError> {
        let mut kernels = self.kernels.lock().map_err(|error| {
            CudaRuntimeError::driver(
                CudaDriverStage::InternalState,
                format!("kernel cache mutex poisoned: {error}"),
            )
        })?;
        let cache_key = (mode, kernel.source.clone());
        if let Some(function) = kernels.get(&cache_key) {
            let mut cache = self.artifact_cache.lock().map_err(|error| {
                CudaRuntimeError::driver(
                    CudaDriverStage::InternalState,
                    format!("artifact cache mutex poisoned: {error}"),
                )
            })?;
            cache.record_ptx_hit();
            cache.record_module_hit();
            return Ok(function.clone());
        }

        let options = nvrtc_options_for(mode, self.device.compute_capability);
        let ptx_key = CudaPtxCacheKey::new(
            kernel.kernel_digest(),
            self.device.compute_capability,
            Some(self.toolchain.nvrtc_version),
            options.clone(),
        );

        let mut cache = self.artifact_cache.lock().map_err(|error| {
            CudaRuntimeError::driver(
                CudaDriverStage::InternalState,
                format!("artifact cache mutex poisoned: {error}"),
            )
        })?;

        let (ptx_src, ptx_digest) = if let Some(cached_ptx) = cache.get_ptx(&ptx_key) {
            (cached_ptx.ptx.clone(), cached_ptx.ptx_digest.clone())
        } else {
            let compiled = cudarc::nvrtc::compile_ptx_with_opts(
                kernel.source.clone(),
                cudarc::nvrtc::CompileOptions {
                    options,
                    ..Default::default()
                },
            )
            .map_err(|error| CudaRuntimeError::Nvrtc(error.to_string()))?;
            let artifact = CudaPtxArtifact::new(ptx_key, compiled.to_src());
            let ptx_src = artifact.ptx.clone();
            let ptx_digest = artifact.ptx_digest.clone();
            cache.insert_ptx(artifact);
            (ptx_src, ptx_digest)
        };

        let driver_key = CudaDriverJitCacheKey::new(
            ptx_digest,
            self.device.ordinal,
            self.device.compute_capability,
            Some(self.toolchain.driver_version),
        );

        let module = self
            .context
            .load_module(cudarc::nvrtc::Ptx::from_src(ptx_src))
            .map_err(|error| CudaRuntimeError::driver(CudaDriverStage::ModuleLoad, error))?;
        let function = module
            .load_function(kernel.entry_point)
            .map_err(|error| CudaRuntimeError::driver(CudaDriverStage::FunctionLoad, error))?;

        let module_artifact = CudaDriverJitModuleArtifact::new(driver_key, kernel.entry_point);
        cache.insert_module(module_artifact);

        kernels.insert(cache_key, function.clone());
        Ok(function)
    }

    fn function_for_source(
        &self,
        mode: CudaKernelMode,
        source: String,
    ) -> Result<CudaFunction, CudaRuntimeError> {
        let kernel = CudaMapKernel {
            identity: sens::sens!(01011001),
            numeric_domain: crate::compute::NumericDomain::FixedWidthInteger,
            element_type: CudaElementType::I32,
            parameter_count: 1,
            entry_point: "cml_map",
            source,
        };
        self.function_for_kernel(mode, &kernel)
    }

    /// Measure fine-grained compile-to-execution latency breakdown for an admitted IR (#491).
    pub fn measure_latency_breakdown(
        &self,
        ir: &Ir,
        mode: CudaKernelMode,
    ) -> Result<CudaLatencyBreakdown, CudaRuntimeError> {
        let t_lowering = std::time::Instant::now();
        let kernel = lower_map_kernel(ir)?;
        let cml_ir_lowering_ns = t_lowering.elapsed().as_nanos() as u64;

        let t_cpu = std::time::Instant::now();
        let cpu_output = CpuComputeBackend.execute(ir).map_err(|error| {
            CudaRuntimeError::driver(CudaDriverStage::InternalState, format!("{error:?}"))
        })?;
        let cpu_reference_ns = t_cpu.elapsed().as_nanos() as u64;

        let options = nvrtc_options_for(mode, self.device.compute_capability);
        let t_nvrtc = std::time::Instant::now();
        let compiled = cudarc::nvrtc::compile_ptx_with_opts(
            kernel.source.clone(),
            cudarc::nvrtc::CompileOptions {
                options,
                ..Default::default()
            },
        )
        .map_err(|error| CudaRuntimeError::Nvrtc(error.to_string()))?;
        let nvrtc_compile_ns = t_nvrtc.elapsed().as_nanos() as u64;
        let ptx_src = compiled.to_src();

        let t_driver = std::time::Instant::now();
        let module = self
            .context
            .load_module(cudarc::nvrtc::Ptx::from_src(ptx_src))
            .map_err(|error| CudaRuntimeError::driver(CudaDriverStage::ModuleLoad, error))?;
        let driver_jit_load_ns = t_driver.elapsed().as_nanos() as u64;

        let t_func = std::time::Instant::now();
        let function = module
            .load_function(kernel.entry_point)
            .map_err(|error| CudaRuntimeError::driver(CudaDriverStage::FunctionLoad, error))?;
        let function_lookup_ns = t_func.elapsed().as_nanos() as u64;

        let buffer = map_input(ir).ok_or(CudaRuntimeError::UnsupportedInput)?;
        let stream = self.context.default_stream();

        let (htod_transfer_ns, kernel_execution_ns, dtoh_transfer_ns, gpu_output) = match buffer {
            BufferLiteral::I32(values) => {
                let length =
                    u32::try_from(values.len()).map_err(|_| CudaRuntimeError::UnsupportedInput)?;
                let mut dev_in = unsafe { stream.alloc::<i32>(values.len()) }.map_err(|error| {
                    CudaRuntimeError::driver(CudaDriverStage::Allocation, error)
                })?;
                let mut dev_out =
                    unsafe { stream.alloc::<i32>(values.len()) }.map_err(|error| {
                        CudaRuntimeError::driver(CudaDriverStage::Allocation, error)
                    })?;

                let t_htod = std::time::Instant::now();
                stream
                    .memcpy_htod(values.as_slice(), &mut dev_in)
                    .map_err(|error| {
                        CudaRuntimeError::driver(CudaDriverStage::HostToDeviceCopy, error)
                    })?;
                let htod_transfer_ns = t_htod.elapsed().as_nanos() as u64;

                let t_exec = std::time::Instant::now();
                unsafe {
                    stream
                        .launch_builder(&function)
                        .arg(&dev_in)
                        .arg(&mut dev_out)
                        .arg(&length)
                        .launch(LaunchConfig::for_num_elems(length))
                }
                .map_err(|error| CudaRuntimeError::driver(CudaDriverStage::Launch, error))?;
                stream
                    .synchronize()
                    .map_err(|error| CudaRuntimeError::driver(CudaDriverStage::Launch, error))?;
                let kernel_execution_ns = t_exec.elapsed().as_nanos() as u64;

                let mut out_vec = vec![0i32; values.len()];
                let t_dtoh = std::time::Instant::now();
                stream
                    .memcpy_dtoh(&dev_out, &mut out_vec)
                    .map_err(|error| {
                        CudaRuntimeError::driver(CudaDriverStage::DeviceToHostCopy, error)
                    })?;
                let dtoh_transfer_ns = t_dtoh.elapsed().as_nanos() as u64;

                (
                    htod_transfer_ns,
                    kernel_execution_ns,
                    dtoh_transfer_ns,
                    BufferLiteral::I32(out_vec),
                )
            }
            BufferLiteral::F32(bits) => {
                let values: Vec<f32> = bits.iter().copied().map(f32::from_bits).collect();
                let length =
                    u32::try_from(values.len()).map_err(|_| CudaRuntimeError::UnsupportedInput)?;
                let mut dev_in = unsafe { stream.alloc::<f32>(values.len()) }.map_err(|error| {
                    CudaRuntimeError::driver(CudaDriverStage::Allocation, error)
                })?;
                let mut dev_out =
                    unsafe { stream.alloc::<f32>(values.len()) }.map_err(|error| {
                        CudaRuntimeError::driver(CudaDriverStage::Allocation, error)
                    })?;

                let t_htod = std::time::Instant::now();
                stream
                    .memcpy_htod(values.as_slice(), &mut dev_in)
                    .map_err(|error| {
                        CudaRuntimeError::driver(CudaDriverStage::HostToDeviceCopy, error)
                    })?;
                let htod_transfer_ns = t_htod.elapsed().as_nanos() as u64;

                let t_exec = std::time::Instant::now();
                unsafe {
                    stream
                        .launch_builder(&function)
                        .arg(&dev_in)
                        .arg(&mut dev_out)
                        .arg(&length)
                        .launch(LaunchConfig::for_num_elems(length))
                }
                .map_err(|error| CudaRuntimeError::driver(CudaDriverStage::Launch, error))?;
                stream
                    .synchronize()
                    .map_err(|error| CudaRuntimeError::driver(CudaDriverStage::Launch, error))?;
                let kernel_execution_ns = t_exec.elapsed().as_nanos() as u64;

                let mut out_vec = vec![0.0f32; values.len()];
                let t_dtoh = std::time::Instant::now();
                stream
                    .memcpy_dtoh(&dev_out, &mut out_vec)
                    .map_err(|error| {
                        CudaRuntimeError::driver(CudaDriverStage::DeviceToHostCopy, error)
                    })?;
                let dtoh_transfer_ns = t_dtoh.elapsed().as_nanos() as u64;

                (
                    htod_transfer_ns,
                    kernel_execution_ns,
                    dtoh_transfer_ns,
                    BufferLiteral::F32(out_vec.iter().map(|f| f.to_bits()).collect()),
                )
            }
        };

        if gpu_output != cpu_output {
            return Err(CudaRuntimeError::driver(
                CudaDriverStage::InternalState,
                "GPU observable diverged from CPU reference in benchmark",
            ));
        }

        let total_cold_ns = cml_ir_lowering_ns
            + nvrtc_compile_ns
            + driver_jit_load_ns
            + function_lookup_ns
            + htod_transfer_ns
            + kernel_execution_ns
            + dtoh_transfer_ns;

        // Warm run using cached session mechanisms
        let _ = self.execute_map_with_mode(ir, mode)?;
        let t_warm = std::time::Instant::now();
        let warm_exec = self.execute_map_with_mode(ir, mode)?;
        let total_warm_ns = t_warm.elapsed().as_nanos() as u64;
        if warm_exec.output != cpu_output {
            return Err(CudaRuntimeError::driver(
                CudaDriverStage::InternalState,
                "Warm GPU observable diverged from CPU reference",
            ));
        }

        Ok(CudaLatencyBreakdown {
            cml_ir_lowering_ns,
            nvrtc_compile_ns,
            driver_jit_load_ns,
            function_lookup_ns,
            htod_transfer_ns,
            kernel_execution_ns,
            dtoh_transfer_ns,
            total_cold_ns,
            total_warm_ns,
            cpu_reference_ns,
        })
    }
}

pub fn query_driver_version() -> Result<i32, CudaRuntimeError> {
    let mut version = 0;
    let res = unsafe { cudarc::driver::sys::cuDriverGetVersion(&mut version) };
    if res == cudarc::driver::sys::CUresult::CUDA_SUCCESS {
        Ok(version)
    } else {
        Err(CudaRuntimeError::driver(
            CudaDriverStage::DeviceEvidence,
            format!("cuDriverGetVersion failed with code {res:?}"),
        ))
    }
}

pub fn query_nvrtc_version() -> Result<(usize, usize), CudaRuntimeError> {
    let mut major = 0;
    let mut minor = 0;
    let res = unsafe { cudarc::nvrtc::sys::nvrtcVersion(&mut major, &mut minor) };
    if res == cudarc::nvrtc::sys::nvrtcResult::NVRTC_SUCCESS {
        Ok((major as usize, minor as usize))
    } else {
        Err(CudaRuntimeError::Nvrtc(format!(
            "nvrtcVersion failed with code {res:?}"
        )))
    }
}

static CUDA_SESSIONS: OnceLock<Mutex<HashMap<usize, Arc<CudaSession>>>> = OnceLock::new();

pub fn discover_devices() -> Result<Vec<CudaDevice>, CudaRuntimeError> {
    let count = CudaContext::device_count()
        .map_err(|error| CudaRuntimeError::driver(CudaDriverStage::DeviceCount, error))?;
    (0..count)
        .map(|ordinal| {
            let ordinal = ordinal as usize;
            let context = CudaContext::new(ordinal).map_err(|error| {
                CudaRuntimeError::driver(CudaDriverStage::ContextCreate { ordinal }, error)
            })?;
            device_evidence(&context)
        })
        .collect()
}

pub fn probe_capability() -> Result<CudaCapabilityStatus, CudaRuntimeError> {
    let devices = discover_devices()?;
    if devices.is_empty() {
        Ok(CudaCapabilityStatus::RuntimePresentNoDevices)
    } else {
        Ok(CudaCapabilityStatus::Live(devices))
    }
}

/// Compatibility entrypoint. Repeated calls for the same ordinal now share a
/// long-lived CUDA session and its compiled-kernel cache.
pub fn execute_map(ir: &Ir, device_ordinal: usize) -> Result<CudaExecution, CudaRuntimeError> {
    session_for_device(device_ordinal)?.execute_map(ir)
}

pub fn execute_map_chain_i32(
    functions: &[Ir],
    input: &BufferLiteral,
    device_ordinal: usize,
) -> Result<CudaChainExecution, CudaChainRuntimeError> {
    let session = session_for_device(device_ordinal)
        .map_err(|source| CudaChainRuntimeError { step: 0, source })?;
    session.execute_map_chain_i32(functions, input)
}

pub fn execute_map_chain_i32_selected(
    functions: &[Ir],
    input: &BufferLiteral,
    requested_steps: &[usize],
    device_ordinal: usize,
) -> Result<CudaSelectedChainExecution, CudaChainRuntimeError> {
    let session = session_for_device(device_ordinal)
        .map_err(|source| CudaChainRuntimeError { step: 0, source })?;
    session.execute_map_chain_i32_selected(functions, input, requested_steps)
}

fn session_for_device(device_ordinal: usize) -> Result<Arc<CudaSession>, CudaRuntimeError> {
    let sessions = CUDA_SESSIONS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut sessions = sessions.lock().map_err(|error| {
        CudaRuntimeError::driver(
            CudaDriverStage::InternalState,
            format!("CUDA session cache mutex poisoned: {error}"),
        )
    })?;
    if let Some(session) = sessions.get(&device_ordinal) {
        return Ok(session.clone());
    }

    let session = Arc::new(CudaSession::new(device_ordinal)?);
    sessions.insert(device_ordinal, session.clone());
    Ok(session)
}

fn device_evidence(context: &CudaContext) -> Result<CudaDevice, CudaRuntimeError> {
    let name = context
        .name()
        .map_err(|error| CudaRuntimeError::driver(CudaDriverStage::DeviceEvidence, error))?;
    let compute_capability = context
        .compute_capability()
        .map_err(|error| CudaRuntimeError::driver(CudaDriverStage::DeviceEvidence, error))?;
    let total_memory_bytes = context
        .total_mem()
        .map_err(|error| CudaRuntimeError::driver(CudaDriverStage::DeviceEvidence, error))?;
    Ok(CudaDevice {
        descriptor: AcceleratorDescriptor {
            name,
            vendor: AcceleratorVendor::Nvidia,
            api: AcceleratorApi::Cuda,
            class: AcceleratorClass::DiscreteGpu,
        },
        ordinal: context.ordinal(),
        compute_capability,
        total_memory_bytes,
    })
}

fn map_input(ir: &Ir) -> Option<&BufferLiteral> {
    let Ir::App { args, .. } = ir else {
        return None;
    };
    let [_, Ir::Buffer(buffer)] = args.as_slice() else {
        return None;
    };
    Some(buffer)
}

fn i32_range_probe_ir(function: &Ir, range: I32Range) -> Ir {
    let values = if range.min == range.max {
        vec![range.min]
    } else {
        vec![range.min, range.max]
    };
    Ir::App {
        func: Box::new(Ir::Sid(sens::sens!(01011001))),
        args: vec![function.clone(), Ir::Buffer(BufferLiteral::I32(values))],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nvrtc_arch_from_compute_capability_known_archs() {
        assert_eq!(nvrtc_arch_from_compute_capability((6, 1)), "compute_61");
        assert_eq!(nvrtc_arch_from_compute_capability((7, 0)), "compute_70");
        assert_eq!(nvrtc_arch_from_compute_capability((7, 5)), "compute_75");
        assert_eq!(nvrtc_arch_from_compute_capability((8, 0)), "compute_80");
        assert_eq!(nvrtc_arch_from_compute_capability((8, 6)), "compute_86");
        assert_eq!(nvrtc_arch_from_compute_capability((8, 7)), "compute_87");
        assert_eq!(nvrtc_arch_from_compute_capability((8, 9)), "compute_89");
        assert_eq!(nvrtc_arch_from_compute_capability((9, 0)), "compute_90");
    }

    #[test]
    fn bitwise_equality_mode_compiles_nvrtc_without_fma_contraction() {
        let options = nvrtc_options_for(CudaKernelMode::BitwiseEquality, (6, 1));
        assert!(
            options.contains(&NVRTC_NO_FMA_CONTRACTION.to_string()),
            "witness mode must pass {NVRTC_NO_FMA_CONTRACTION} to NVRTC, got {options:?}"
        );
        assert!(options.contains(&"-arch=compute_61".to_string()));
    }

    #[test]
    fn production_mode_keeps_nvrtc_defaults() {
        let options = nvrtc_options_for(CudaKernelMode::Production, (6, 1));
        assert_eq!(
            options,
            vec!["-arch=compute_61".to_string()],
            "production mode must not change the NVRTC options it had before cml#360"
        );
    }

    #[test]
    fn nvrtc_arch_format_is_correct() {
        let arch = nvrtc_arch_from_compute_capability((6, 1));
        assert!(arch.starts_with("compute_"));
        assert_eq!(arch.len(), "compute_61".len());
        assert!(!arch.contains('.'));
    }

    #[test]
    fn driver_failure_stage_is_machine_readable() {
        let error = CudaRuntimeError::driver(CudaDriverStage::Launch, "boom");
        assert!(matches!(
            error,
            CudaRuntimeError::Driver {
                stage: CudaDriverStage::Launch,
                ..
            }
        ));
    }

    #[test]
    fn preflight_zero_reserve_preserves_exact_fit() {
        assert_eq!(preflight_device_memory(512, 1024, 0).unwrap(), 1024);
        assert_eq!(preflight_device_memory(1024, 1024, 0).unwrap(), 1024);
    }

    #[test]
    fn preflight_reserve_reduces_effective_available() {
        assert_eq!(preflight_device_memory(700, 1000, 300).unwrap(), 700);
        let error = preflight_device_memory(701, 1000, 300).unwrap_err();
        assert!(matches!(
            error,
            CudaRuntimeError::InsufficientDeviceMemory {
                required_bytes: 701,
                free_bytes: 1000,
                reserve_bytes: 300,
                effective_available_bytes: 700,
            }
        ));
    }

    #[test]
    fn preflight_reserve_saturates_at_zero() {
        let error = preflight_device_memory(1, 200, 500).unwrap_err();
        assert!(matches!(
            error,
            CudaRuntimeError::InsufficientDeviceMemory {
                required_bytes: 1,
                free_bytes: 200,
                reserve_bytes: 500,
                effective_available_bytes: 0,
            }
        ));
    }

    #[test]
    fn memory_error_display_reports_resource_accounting() {
        let error = CudaRuntimeError::InsufficientDeviceMemory {
            required_bytes: 4096,
            free_bytes: 2048,
            reserve_bytes: 1024,
            effective_available_bytes: 1024,
        };
        let text = error.to_string();
        assert!(text.contains("required=4096"));
        assert!(text.contains("free=2048"));
        assert!(text.contains("reserve=1024"));
        assert!(text.contains("effective_available=1024"));
    }
}
