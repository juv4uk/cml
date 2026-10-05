//! Optional NVIDIA CUDA execution for admitted map regions.

use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::Hash;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use cudarc::driver::{CudaContext, CudaFunction, CudaSlice, LaunchConfig, PushKernelArg};
use cudarc::nvrtc::Ptx;

use crate::accelerator::{
    AcceleratorApi, AcceleratorClass, AcceleratorDescriptor, AcceleratorVendor,
};
use crate::compute::{I32Range, fuse_i32_map_chain, i32_buffer_range, prove_i32_map_range};
use crate::gpu_cuda::{CudaEmitError, CudaMapKernel, emit_i32_compute_kernel, lower_map_kernel};
use crate::ir::{BufferLiteral, Ir};

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
    DriverVersion,
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

impl CudaRuntimeError {
    fn driver(stage: CudaDriverStage, error: impl ToString) -> Self {
        Self::Driver {
            stage,
            message: error.to_string(),
        }
    }
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

pub const CUDA_KERNEL_ABI_SCHEMA_VERSION: u32 = 1;
pub const CUDA_LOWERING_SCHEMA_VERSION: u32 = 1;
const CUDA_PTX_CACHE_CAPACITY: usize = 64;
const CUDA_LOADED_CACHE_CAPACITY: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CudaPtxCacheKey {
    pub source_sha256: String,
    pub compute_capability: (i32, i32),
    pub nvrtc_version: (i32, i32),
    pub compile_options: Vec<String>,
    pub kernel_abi_schema: u32,
    pub lowering_schema: u32,
    pub cml_version: String,
    pub entry_point: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CudaLoadedCacheKey {
    pub ptx: CudaPtxCacheKey,
    pub device_ordinal: usize,
    pub driver_version: i32,
}

#[derive(Debug, Clone)]
pub struct CudaPtxArtifact {
    pub key: CudaPtxCacheKey,
    pub ptx_sha256: String,
    pub ptx_len_bytes: usize,
    ptx: Ptx,
}

impl CudaPtxArtifact {
    fn ptx(&self) -> Ptx {
        self.ptx.clone()
    }
}

#[derive(Debug, Clone)]
pub struct CudaLoadedKernelArtifact {
    pub key: CudaLoadedCacheKey,
    pub ptx_sha256: String,
    pub entry_point: String,
    function: CudaFunction,
}

impl CudaLoadedKernelArtifact {
    fn function(&self) -> CudaFunction {
        self.function.clone()
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CudaJitCacheStats {
    pub ptx_hits: u64,
    pub ptx_misses: u64,
    pub loaded_hits: u64,
    pub loaded_misses: u64,
}

#[derive(Debug)]
struct BoundedCache<K, V> {
    capacity: usize,
    entries: HashMap<K, V>,
    order: VecDeque<K>,
}

impl<K: Eq + Hash + Clone, V> BoundedCache<K, V> {
    fn new(capacity: usize) -> Self {
        Self {
            capacity,
            entries: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    fn get(&self, key: &K) -> Option<&V> {
        self.entries.get(key)
    }

    fn len(&self) -> usize {
        self.entries.len()
    }

    fn insert(&mut self, key: K, value: V) {
        if self.capacity == 0 {
            return;
        }
        if self.entries.contains_key(&key) {
            self.entries.insert(key, value);
            return;
        }
        if self.entries.len() >= self.capacity
            && let Some(oldest) = self.order.pop_front()
        {
            self.entries.remove(&oldest);
        }
        self.order.push_back(key.clone());
        self.entries.insert(key, value);
    }
}

static CUDA_PTX_CACHE: OnceLock<Mutex<BoundedCache<CudaPtxCacheKey, CudaPtxArtifact>>> =
    OnceLock::new();
static CUDA_PTX_HITS: AtomicU64 = AtomicU64::new(0);
static CUDA_PTX_MISSES: AtomicU64 = AtomicU64::new(0);

fn sha256_hex(bytes: &[u8]) -> String {
    sens::sha256_source(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn nvrtc_version() -> Result<(i32, i32), CudaRuntimeError> {
    let mut major = 0;
    let mut minor = 0;
    unsafe {
        cudarc::nvrtc::sys::nvrtcVersion(&mut major, &mut minor)
            .result()
            .map_err(|error| CudaRuntimeError::Nvrtc(format!("nvrtcVersion: {error}")))?;
    }
    Ok((major, minor))
}

fn cuda_driver_version() -> Result<i32, CudaRuntimeError> {
    let mut version = 0;
    unsafe {
        cudarc::driver::sys::cuDriverGetVersion(&mut version)
            .result()
            .map_err(|error| CudaRuntimeError::driver(CudaDriverStage::DriverVersion, error))?;
    }
    Ok(version)
}

fn ptx_cache_key(
    source: &str,
    compute_capability: (i32, i32),
    nvrtc_version: (i32, i32),
    compile_options: Vec<String>,
    entry_point: &str,
) -> CudaPtxCacheKey {
    CudaPtxCacheKey {
        source_sha256: sha256_hex(source.as_bytes()),
        compute_capability,
        nvrtc_version,
        compile_options,
        kernel_abi_schema: CUDA_KERNEL_ABI_SCHEMA_VERSION,
        lowering_schema: CUDA_LOWERING_SCHEMA_VERSION,
        cml_version: env!("CARGO_PKG_VERSION").to_string(),
        entry_point: entry_point.to_string(),
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
    driver_version: i32,
    loaded_kernels: Mutex<BoundedCache<CudaLoadedCacheKey, CudaLoadedKernelArtifact>>,
    loaded_hits: AtomicU64,
    loaded_misses: AtomicU64,
    i32_buffers: Mutex<Option<ReusableBuffers<i32>>>,
    f32_buffers: Mutex<Option<ReusableBuffers<f32>>>,
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
        let driver_version = cuda_driver_version()?;
        Ok(Self {
            context,
            device,
            driver_version,
            loaded_kernels: Mutex::new(BoundedCache::new(CUDA_LOADED_CACHE_CAPACITY)),
            loaded_hits: AtomicU64::new(0),
            loaded_misses: AtomicU64::new(0),
            i32_buffers: Mutex::new(None),
            f32_buffers: Mutex::new(None),
        })
    }

    pub fn device(&self) -> &CudaDevice {
        &self.device
    }

    /// Number of distinct executable map kernels currently resident in this
    /// session. Exposed as mechanism evidence for reuse tests/benchmarks.
    pub fn cached_kernel_count(&self) -> Result<usize, CudaRuntimeError> {
        self.loaded_kernels
            .lock()
            .map(|kernels| kernels.len())
            .map_err(|error| {
                CudaRuntimeError::driver(
                    CudaDriverStage::InternalState,
                    format!("loaded-kernel cache mutex poisoned: {error}"),
                )
            })
    }

    pub fn jit_cache_stats(&self) -> CudaJitCacheStats {
        CudaJitCacheStats {
            ptx_hits: CUDA_PTX_HITS.load(Ordering::Relaxed),
            ptx_misses: CUDA_PTX_MISSES.load(Ordering::Relaxed),
            loaded_hits: self.loaded_hits.load(Ordering::Relaxed),
            loaded_misses: self.loaded_misses.load(Ordering::Relaxed),
        }
    }

    pub fn driver_version(&self) -> i32 {
        self.driver_version
    }

    pub fn cached_ptx_count(&self) -> Result<usize, CudaRuntimeError> {
        CUDA_PTX_CACHE
            .get_or_init(|| Mutex::new(BoundedCache::new(CUDA_PTX_CACHE_CAPACITY)))
            .lock()
            .map(|cache| cache.len())
            .map_err(|error| {
                CudaRuntimeError::driver(
                    CudaDriverStage::InternalState,
                    format!("PTX cache mutex poisoned: {error}"),
                )
            })
    }

    /// Compile an admitted CML map through NVRTC and return the portable PTX artifact.
    pub fn compile_map_ptx(
        &self,
        ir: &Ir,
        mode: CudaKernelMode,
    ) -> Result<CudaPtxArtifact, CudaRuntimeError> {
        let kernel = lower_map_kernel(ir)?;
        self.ptx_artifact_for_source(mode, &kernel.source, kernel.entry_point)
    }

    /// Compile and Driver-JIT/load an admitted map for this exact CUDA device/session.
    pub fn load_map_artifact(
        &self,
        ir: &Ir,
        mode: CudaKernelMode,
    ) -> Result<CudaLoadedKernelArtifact, CudaRuntimeError> {
        let kernel = lower_map_kernel(ir)?;
        self.loaded_artifact_for_source(mode, &kernel.source, kernel.entry_point)
    }

    pub fn prepare_map<'a>(&'a self, ir: &'a Ir) -> Result<PreparedCudaMap<'a>, CudaRuntimeError> {
        self.prepare_map_with_mode(ir, CudaKernelMode::Production)
    }

    /// Witness variant of [`CudaSession::prepare_map`]: the kernel is
    /// compiled by NVRTC with `-fmad=false` (cml#360, sens#1585 A1).
    pub fn prepare_map_with_mode<'a>(
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
        if required_bytes > free_bytes {
            return Err(fail(
                0,
                CudaRuntimeError::InsufficientDeviceMemory {
                    required_bytes,
                    free_bytes,
                },
            ));
        }

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
            .function_for_source(CudaKernelMode::Production, &source, "cml_map")
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
        if required_bytes > free_bytes {
            return Err(fail(
                0,
                CudaRuntimeError::InsufficientDeviceMemory {
                    required_bytes,
                    free_bytes,
                },
            ));
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

    fn function_for_kernel(
        &self,
        mode: CudaKernelMode,
        kernel: &CudaMapKernel,
    ) -> Result<CudaFunction, CudaRuntimeError> {
        self.function_for_source(mode, &kernel.source, kernel.entry_point)
    }

    fn function_for_source(
        &self,
        mode: CudaKernelMode,
        source: &str,
        entry_point: &str,
    ) -> Result<CudaFunction, CudaRuntimeError> {
        Ok(self
            .loaded_artifact_for_source(mode, source, entry_point)?
            .function())
    }

    fn loaded_artifact_for_source(
        &self,
        mode: CudaKernelMode,
        source: &str,
        entry_point: &str,
    ) -> Result<CudaLoadedKernelArtifact, CudaRuntimeError> {
        let ptx_artifact = self.ptx_artifact_for_source(mode, source, entry_point)?;
        let loaded_key = CudaLoadedCacheKey {
            ptx: ptx_artifact.key.clone(),
            device_ordinal: self.device.ordinal,
            driver_version: self.driver_version,
        };

        {
            let loaded = self.loaded_kernels.lock().map_err(|error| {
                CudaRuntimeError::driver(
                    CudaDriverStage::InternalState,
                    format!("loaded-kernel cache mutex poisoned: {error}"),
                )
            })?;
            if let Some(artifact) = loaded.get(&loaded_key) {
                self.loaded_hits.fetch_add(1, Ordering::Relaxed);
                return Ok(artifact.clone());
            }
        }

        self.loaded_misses.fetch_add(1, Ordering::Relaxed);
        let module = self
            .context
            .load_module(ptx_artifact.ptx())
            .map_err(|error| CudaRuntimeError::driver(CudaDriverStage::ModuleLoad, error))?;
        let function = module
            .load_function(entry_point)
            .map_err(|error| CudaRuntimeError::driver(CudaDriverStage::FunctionLoad, error))?;

        let loaded_artifact = CudaLoadedKernelArtifact {
            key: loaded_key.clone(),
            ptx_sha256: ptx_artifact.ptx_sha256.clone(),
            entry_point: entry_point.to_string(),
            function: function.clone(),
        };
        let mut loaded = self.loaded_kernels.lock().map_err(|error| {
            CudaRuntimeError::driver(
                CudaDriverStage::InternalState,
                format!("loaded-kernel cache mutex poisoned: {error}"),
            )
        })?;
        loaded.insert(loaded_key, loaded_artifact.clone());
        Ok(loaded_artifact)
    }

    fn ptx_artifact_for_source(
        &self,
        mode: CudaKernelMode,
        source: &str,
        entry_point: &str,
    ) -> Result<CudaPtxArtifact, CudaRuntimeError> {
        let options = nvrtc_options_for(mode, self.device.compute_capability);
        let key = ptx_cache_key(
            source,
            self.device.compute_capability,
            nvrtc_version()?,
            options.clone(),
            entry_point,
        );

        let cache =
            CUDA_PTX_CACHE.get_or_init(|| Mutex::new(BoundedCache::new(CUDA_PTX_CACHE_CAPACITY)));
        {
            let cache = cache.lock().map_err(|error| {
                CudaRuntimeError::driver(
                    CudaDriverStage::InternalState,
                    format!("PTX cache mutex poisoned: {error}"),
                )
            })?;
            if let Some(artifact) = cache.get(&key) {
                CUDA_PTX_HITS.fetch_add(1, Ordering::Relaxed);
                return Ok(artifact.clone());
            }
        }

        CUDA_PTX_MISSES.fetch_add(1, Ordering::Relaxed);
        let ptx = cudarc::nvrtc::compile_ptx_with_opts(
            source,
            cudarc::nvrtc::CompileOptions {
                options,
                ..Default::default()
            },
        )
        .map_err(|error| CudaRuntimeError::Nvrtc(error.to_string()))?;
        let ptx_bytes = ptx
            .as_bytes()
            .map(<[u8]>::to_vec)
            .unwrap_or_else(|| ptx.to_src().into_bytes());
        let artifact = CudaPtxArtifact {
            key: key.clone(),
            ptx_sha256: sha256_hex(&ptx_bytes),
            ptx_len_bytes: ptx_bytes.len(),
            ptx,
        };

        let mut cache = cache.lock().map_err(|error| {
            CudaRuntimeError::driver(
                CudaDriverStage::InternalState,
                format!("PTX cache mutex poisoned: {error}"),
            )
        })?;
        cache.insert(key, artifact.clone());
        Ok(artifact)
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
    fn sha256_evidence_is_stable() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn ptx_cache_key_changes_when_target_or_options_change() {
        let base = ptx_cache_key(
            "kernel",
            (6, 1),
            (12, 6),
            vec!["-arch=compute_61".to_string()],
            "cml_map",
        );
        let different_arch = ptx_cache_key(
            "kernel",
            (7, 5),
            (12, 6),
            vec!["-arch=compute_75".to_string()],
            "cml_map",
        );
        let different_mode = ptx_cache_key(
            "kernel",
            (6, 1),
            (12, 6),
            vec![
                "-arch=compute_61".to_string(),
                NVRTC_NO_FMA_CONTRACTION.to_string(),
            ],
            "cml_map",
        );
        assert_ne!(base, different_arch);
        assert_ne!(base, different_mode);
    }

    #[test]
    fn bounded_cache_evicts_oldest_entry() {
        let mut cache = BoundedCache::new(2);
        cache.insert("a", 1);
        cache.insert("b", 2);
        cache.insert("c", 3);
        assert!(cache.get(&"a").is_none());
        assert_eq!(cache.get(&"b"), Some(&2));
        assert_eq!(cache.get(&"c"), Some(&3));
        assert_eq!(cache.len(), 2);
    }
}
