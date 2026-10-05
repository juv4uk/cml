//! NVIDIA CUDA source emission for admitted CML Compute IR.
//!
//! Like the portable WGSL emitter, this module cannot bypass semantic
//! admission. NVRTC compilation, device transfer, launch, and readback belong
//! to the optional CUDA runtime layer.

use std::collections::{HashMap, VecDeque};
use std::io::Write;

use crate::compute::{
    AdmissionBlocker, BulkOperation, ComputeKernel, F32MapKernel, NumericDomain, ScalarExpr,
    analyze,
};
use crate::ir::Ir;

pub const CML_CUDA_LOWERING_SCHEMA_VERSION: u32 = 2;
pub const CML_CUDA_KERNEL_ABI_VERSION: u32 = 1;
pub const DEFAULT_CUDA_CACHE_CAPACITY: usize = 64;

/// First-class CML compiler target for NVIDIA Driver JIT.
///
/// This identifies only the compilation mechanism boundary; it does not
/// define or reinterpret any SENS semantic identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CudaCompilerTarget {
    NvidiaDriverJit,
}

impl CudaCompilerTarget {
    pub const fn name(self) -> &'static str {
        match self {
            Self::NvidiaDriverJit => "NvidiaDriverJit",
        }
    }
}

/// Deterministic 64-bit FNV-1a digest.
pub fn fnv1a64_digest(bytes: &[u8]) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for &byte in bytes {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("fnv1a64:{hash:016x}")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CudaEmitError {
    NotEligible(Vec<AdmissionBlocker>),
    UnsupportedRegion,
}

/// Target ABI selected by the CUDA lowering boundary.
///
/// This is compiler mechanism metadata, never a SENS semantic identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CudaElementType {
    I32,
    F32,
}

impl CudaElementType {
    pub const fn c_type(self) -> &'static str {
        match self {
            Self::I32 => "int",
            Self::F32 => "float",
        }
    }
}

/// Target architecture / compute capability for CUDA compilation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CudaComputeCapability {
    pub major: i32,
    pub minor: i32,
}

impl CudaComputeCapability {
    pub const fn new(major: i32, minor: i32) -> Self {
        Self { major, minor }
    }

    pub fn nvrtc_arch(self) -> String {
        format!("compute_{}{}", self.major, self.minor)
    }
}

impl From<(i32, i32)> for CudaComputeCapability {
    fn from((major, minor): (i32, i32)) -> Self {
        Self::new(major, minor)
    }
}

/// NVRTC version metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NvrtcVersion {
    pub major: i32,
    pub minor: i32,
}

impl NvrtcVersion {
    pub const fn new(major: i32, minor: i32) -> Self {
        Self { major, minor }
    }
}

/// First-class result of lowering one admitted CML map region for NVIDIA CUDA.
///
/// Keeping identity/domain/ABI beside the source makes the compiler boundary
/// inspectable before NVRTC. The runtime may compile this artifact, but cannot
/// mint a new semantic meaning from it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CudaMapKernel {
    pub identity: sens::Sens8,
    pub numeric_domain: NumericDomain,
    pub element_type: CudaElementType,
    pub parameter_count: usize,
    pub entry_point: &'static str,
    pub source: String,
}

impl CudaMapKernel {
    /// Compute a deterministic digest of the kernel source and its ABI/domain facts.
    pub fn kernel_digest(&self) -> String {
        let mut buf = Vec::new();
        write!(&mut buf, "{}", self.identity).unwrap();
        buf.push(match self.numeric_domain {
            NumericDomain::Exact => 1,
            NumericDomain::FixedWidthInteger => 2,
            NumericDomain::InexactFloat => 3,
            NumericDomain::Unknown => 4,
        });
        buf.push(match self.element_type {
            CudaElementType::I32 => 1,
            CudaElementType::F32 => 2,
        });
        buf.extend_from_slice(&(self.parameter_count as u64).to_be_bytes());
        buf.extend_from_slice(self.entry_point.as_bytes());
        buf.extend_from_slice(self.source.as_bytes());
        fnv1a64_digest(&buf)
    }
}

/// First-class compiled CUDA artifact for the admitted associative bounded-i32 reduction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CudaReduceKernel {
    pub numeric_domain: NumericDomain,
    pub element_type: CudaElementType,
    pub parameter_count: usize,
    pub initial: i32,
    pub entry_point: &'static str,
    pub source: String,
}

impl CudaReduceKernel {
    pub fn kernel_digest(&self) -> String {
        let mut buf = Vec::new();
        buf.extend_from_slice(b"reduce-i32");
        buf.extend_from_slice(&self.initial.to_be_bytes());
        buf.push(2);
        buf.push(1);
        buf.extend_from_slice(&(self.parameter_count as u64).to_be_bytes());
        buf.extend_from_slice(self.entry_point.as_bytes());
        buf.extend_from_slice(self.source.as_bytes());
        fnv1a64_digest(&buf)
    }
}

/// Common CUDA lowering artifact for the currently admitted bulk families.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CudaKernelArtifact {
    Map(CudaMapKernel),
    Reduce(CudaReduceKernel),
}

impl CudaKernelArtifact {
    pub fn source(&self) -> &str {
        match self {
            Self::Map(kernel) => &kernel.source,
            Self::Reduce(kernel) => &kernel.source,
        }
    }

    pub fn entry_point(&self) -> &'static str {
        match self {
            Self::Map(kernel) => kernel.entry_point,
            Self::Reduce(kernel) => kernel.entry_point,
        }
    }
}

/// Lower an admitted CML compute region through one CUDA compiler boundary.
pub fn lower_compute_kernel(ir: &Ir) -> Result<CudaKernelArtifact, CudaEmitError> {
    let analysis = analyze(ir);
    let region = analysis
        .region
        .as_ref()
        .ok_or(CudaEmitError::UnsupportedRegion)?;
    match region.operation {
        BulkOperation::Map => Ok(CudaKernelArtifact::Map(lower_map_kernel(ir)?)),
        BulkOperation::Reduce => {
            lower_reduce_kernel_from_analysis(&analysis).map(CudaKernelArtifact::Reduce)
        }
    }
}

fn lower_reduce_kernel_from_analysis(
    analysis: &crate::compute::ComputeAnalysis,
) -> Result<CudaReduceKernel, CudaEmitError> {
    if !analysis.gpu_eligible() {
        return Err(CudaEmitError::NotEligible(analysis.gpu_blockers.clone()));
    }
    if !analysis
        .reduction_proof
        .as_ref()
        .is_some_and(|proof| proof.is_parallel_eligible())
    {
        return Err(CudaEmitError::NotEligible(vec![
            AdmissionBlocker::KernelNotLowerable,
        ]));
    }
    let region = analysis
        .region
        .as_ref()
        .ok_or(CudaEmitError::UnsupportedRegion)?;
    let kernel = region
        .kernel
        .as_ref()
        .ok_or(CudaEmitError::UnsupportedRegion)?;
    let is_checked_add = matches!(
        &kernel.body,
        ScalarExpr::CheckedAdd(left, right)
            if matches!(
                (&**left, &**right),
                (ScalarExpr::Parameter(0), ScalarExpr::Parameter(1))
                    | (ScalarExpr::Parameter(1), ScalarExpr::Parameter(0))
            )
    );
    if region.operation != BulkOperation::Reduce
        || kernel.parameter_count != 2
        || !is_checked_add
        || analysis.numeric_domain != NumericDomain::FixedWidthInteger
    {
        return Err(CudaEmitError::UnsupportedRegion);
    }
    let Ir::Int(initial) = region
        .initial
        .as_ref()
        .ok_or(CudaEmitError::UnsupportedRegion)?
    else {
        return Err(CudaEmitError::UnsupportedRegion);
    };
    let initial = i32::try_from(*initial).map_err(|_| CudaEmitError::UnsupportedRegion)?;
    Ok(CudaReduceKernel {
        numeric_domain: analysis.numeric_domain,
        element_type: CudaElementType::I32,
        parameter_count: 2,
        initial,
        entry_point: "cml_reduce_i32",
        source: render_reduce_kernel(),
    })
}

/// The runtime must zero output_data and launch at least one thread.
/// Thread zero contributes initial, so an empty reduction remains observable.
fn render_reduce_kernel() -> String {
    "extern \"C\" __global__ void cml_reduce_i32(const int *input_data, int *output_data, unsigned int length, int initial) { unsigned int i = blockIdx.x * blockDim.x + threadIdx.x; if (i == 0) atomicAdd(output_data, initial); if (i >= length) return; atomicAdd(output_data, input_data[i]); }\n".to_string()
}

/// Cache key for portable NVRTC PTX compilation outputs.
///
/// PTX depends only on the kernel definition, target compute capability,
/// compile options, NVRTC version, and schema versions. It does not depend
/// on a physical device ordinal or driver session binding.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CudaPtxCacheKey {
    pub kernel_digest: String,
    pub compute_capability: CudaComputeCapability,
    pub nvrtc_version: Option<NvrtcVersion>,
    pub options: Vec<String>,
    pub kernel_abi_version: u32,
    pub lowering_schema_version: u32,
}

impl CudaPtxCacheKey {
    pub fn new(
        kernel_digest: impl Into<String>,
        compute_capability: impl Into<CudaComputeCapability>,
        nvrtc_version: Option<NvrtcVersion>,
        options: Vec<String>,
    ) -> Self {
        Self {
            kernel_digest: kernel_digest.into(),
            compute_capability: compute_capability.into(),
            nvrtc_version,
            options,
            kernel_abi_version: CML_CUDA_KERNEL_ABI_VERSION,
            lowering_schema_version: CML_CUDA_LOWERING_SCHEMA_VERSION,
        }
    }

    pub fn digest(&self) -> String {
        let mut buf = Vec::new();
        buf.extend_from_slice(self.kernel_digest.as_bytes());
        buf.extend_from_slice(&self.compute_capability.major.to_be_bytes());
        buf.extend_from_slice(&self.compute_capability.minor.to_be_bytes());
        if let Some(v) = self.nvrtc_version {
            buf.extend_from_slice(&v.major.to_be_bytes());
            buf.extend_from_slice(&v.minor.to_be_bytes());
        } else {
            buf.extend_from_slice(&[0xFF; 8]);
        }
        for opt in &self.options {
            buf.extend_from_slice(opt.as_bytes());
            buf.push(0);
        }
        buf.extend_from_slice(&self.kernel_abi_version.to_be_bytes());
        buf.extend_from_slice(&self.lowering_schema_version.to_be_bytes());
        fnv1a64_digest(&buf)
    }
}

/// First-class compiled PTX artifact carrying its NVRTC provenance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CudaPtxArtifact {
    pub key: CudaPtxCacheKey,
    pub ptx: String,
    pub ptx_digest: String,
}

impl CudaPtxArtifact {
    pub fn new(key: CudaPtxCacheKey, ptx: String) -> Self {
        let ptx_digest = fnv1a64_digest(ptx.as_bytes());
        Self {
            key,
            ptx,
            ptx_digest,
        }
    }
}

/// Cache key for driver-JIT-loaded modules on a specific device session.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CudaDriverJitCacheKey {
    pub ptx_digest: String,
    pub device_ordinal: usize,
    pub compute_capability: CudaComputeCapability,
    pub driver_version: Option<i32>,
}

impl CudaDriverJitCacheKey {
    pub fn new(
        ptx_digest: impl Into<String>,
        device_ordinal: usize,
        compute_capability: impl Into<CudaComputeCapability>,
        driver_version: Option<i32>,
    ) -> Self {
        Self {
            ptx_digest: ptx_digest.into(),
            device_ordinal,
            compute_capability: compute_capability.into(),
            driver_version,
        }
    }

    pub fn digest(&self) -> String {
        let mut buf = Vec::new();
        buf.extend_from_slice(self.ptx_digest.as_bytes());
        buf.extend_from_slice(&(self.device_ordinal as u64).to_be_bytes());
        buf.extend_from_slice(&self.compute_capability.major.to_be_bytes());
        buf.extend_from_slice(&self.compute_capability.minor.to_be_bytes());
        if let Some(v) = self.driver_version {
            buf.extend_from_slice(&v.to_be_bytes());
        } else {
            buf.extend_from_slice(&[0xFF; 4]);
        }
        fnv1a64_digest(&buf)
    }
}

/// First-class loaded driver JIT module metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CudaDriverJitModuleArtifact {
    pub key: CudaDriverJitCacheKey,
    pub entry_point: &'static str,
    pub module_digest: String,
}

impl CudaDriverJitModuleArtifact {
    pub fn new(key: CudaDriverJitCacheKey, entry_point: &'static str) -> Self {
        let mut buf = Vec::new();
        buf.extend_from_slice(key.digest().as_bytes());
        buf.extend_from_slice(entry_point.as_bytes());
        let module_digest = fnv1a64_digest(&buf);
        Self {
            key,
            entry_point,
            module_digest,
        }
    }
}

/// Diagnostic evidence for artifact cache hits, misses, and evictions.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CudaCacheDiagnosticEvidence {
    pub ptx_hits: usize,
    pub ptx_misses: usize,
    pub ptx_evictions: usize,
    pub module_hits: usize,
    pub module_misses: usize,
    pub module_evictions: usize,
}

/// Bounded cache for PTX artifacts and driver-JIT module artifacts.
#[derive(Debug)]
pub struct CudaArtifactCache {
    capacity: usize,
    ptx_cache: HashMap<CudaPtxCacheKey, CudaPtxArtifact>,
    ptx_order: VecDeque<CudaPtxCacheKey>,
    module_cache: HashMap<CudaDriverJitCacheKey, CudaDriverJitModuleArtifact>,
    module_order: VecDeque<CudaDriverJitCacheKey>,
    diagnostics: CudaCacheDiagnosticEvidence,
}

impl CudaArtifactCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            ptx_cache: HashMap::new(),
            ptx_order: VecDeque::new(),
            module_cache: HashMap::new(),
            module_order: VecDeque::new(),
            diagnostics: CudaCacheDiagnosticEvidence::default(),
        }
    }

    pub fn get_ptx(&mut self, key: &CudaPtxCacheKey) -> Option<&CudaPtxArtifact> {
        if let Some(artifact) = self.ptx_cache.get(key) {
            self.diagnostics.ptx_hits += 1;
            Some(artifact)
        } else {
            self.diagnostics.ptx_misses += 1;
            None
        }
    }

    pub fn insert_ptx(&mut self, artifact: CudaPtxArtifact) {
        let key = artifact.key.clone();
        if !self.ptx_cache.contains_key(&key) {
            if self.ptx_cache.len() >= self.capacity {
                if let Some(oldest) = self.ptx_order.pop_front() {
                    self.ptx_cache.remove(&oldest);
                    self.diagnostics.ptx_evictions += 1;
                }
            }
            self.ptx_order.push_back(key.clone());
        }
        self.ptx_cache.insert(key, artifact);
    }

    pub fn get_module(
        &mut self,
        key: &CudaDriverJitCacheKey,
    ) -> Option<&CudaDriverJitModuleArtifact> {
        if let Some(artifact) = self.module_cache.get(key) {
            self.diagnostics.module_hits += 1;
            Some(artifact)
        } else {
            self.diagnostics.module_misses += 1;
            None
        }
    }

    pub fn insert_module(&mut self, artifact: CudaDriverJitModuleArtifact) {
        let key = artifact.key.clone();
        if !self.module_cache.contains_key(&key) {
            if self.module_cache.len() >= self.capacity {
                if let Some(oldest) = self.module_order.pop_front() {
                    self.module_cache.remove(&oldest);
                    self.diagnostics.module_evictions += 1;
                }
            }
            self.module_order.push_back(key.clone());
        }
        self.module_cache.insert(key, artifact);
    }

    pub fn record_ptx_hit(&mut self) {
        self.diagnostics.ptx_hits += 1;
    }

    pub fn record_module_hit(&mut self) {
        self.diagnostics.module_hits += 1;
    }

    pub fn diagnostics(&self) -> CudaCacheDiagnosticEvidence {
        self.diagnostics
    }

    pub fn clear(&mut self) {
        self.ptx_cache.clear();
        self.ptx_order.clear();
        self.module_cache.clear();
        self.module_order.clear();
    }
}

impl Default for CudaArtifactCache {
    fn default() -> Self {
        Self::new(DEFAULT_CUDA_CACHE_CAPACITY)
    }
}

/// Lower canonical CML IR into one bounded CUDA map-kernel artifact.
///
/// Semantic admission happens before target emission and unsupported regions
/// fail closed. NVRTC/Driver JIT belongs to the runtime layer.
pub fn lower_map_kernel(ir: &Ir) -> Result<CudaMapKernel, CudaEmitError> {
    let analysis = analyze(ir);
    if !analysis.gpu_eligible() {
        return Err(CudaEmitError::NotEligible(analysis.gpu_blockers));
    }
    let region = analysis.region.ok_or(CudaEmitError::UnsupportedRegion)?;
    if region.operation != BulkOperation::Map {
        return Err(CudaEmitError::UnsupportedRegion);
    }
    let identity = region.identity.clone();
    let kernel = region.kernel.ok_or(CudaEmitError::UnsupportedRegion)?;
    if kernel.parameter_count != 1 {
        return Err(CudaEmitError::UnsupportedRegion);
    }

    let (element_type, expression) = match analysis.numeric_domain {
        NumericDomain::FixedWidthInteger => (CudaElementType::I32, emit_i32_expr(&kernel.body)?),
        NumericDomain::InexactFloat => {
            let form = F32MapKernel::lower(&kernel.body).ok_or(CudaEmitError::UnsupportedRegion)?;
            // GPU-2-E1 / #368: two plain operators. FFMA contraction is
            // controlled by the kernel mode (`-fmad=false` in
            // `CudaKernelMode::BitwiseEquality`, PR #366), not by the
            // emitter; the production-mode FMA policy is a separate
            // language-owner decision (sens#1585).
            let expression = match form {
                F32MapKernel::AffineAdd(offset) | F32MapKernel::Add(offset) => {
                    format!("x + {}", cuda_f32(offset))
                }
                F32MapKernel::Mul(scale) => format!("x * {}", cuda_f32(scale)),
                F32MapKernel::MulAdd(scale, offset) => {
                    format!("x * {} + {}", cuda_f32(scale), cuda_f32(offset))
                }
                // GPU-2 E2/E3 / #379: division and sqrt are IEEE operations
                // in NVRTC's default mode; a closed constant is emitted as
                // its exact stored bits, so canonical == backend bitwise
                // even for NaN/Inf.
                F32MapKernel::Div(divisor) => format!("x / {}", cuda_f32(divisor)),
                F32MapKernel::Sqrt => "sqrtf(x)".to_string(),
                F32MapKernel::Constant(bits) => format!("__int_as_float(0x{bits:08x})"),
            };
            (CudaElementType::F32, expression)
        }
        _ => return Err(CudaEmitError::UnsupportedRegion),
    };

    let source = render_map_kernel(element_type.c_type(), &expression);
    Ok(CudaMapKernel {
        identity,
        numeric_domain: analysis.numeric_domain,
        element_type,
        parameter_count: kernel.parameter_count,
        entry_point: "cml_map",
        source,
    })
}

/// Compatibility source-emission API.
///
/// New compiler/runtime code should prefer `lower_map_kernel` so target ABI
/// metadata is not discarded before NVRTC.
pub fn emit_map_kernel(ir: &Ir) -> Result<String, CudaEmitError> {
    Ok(lower_map_kernel(ir)?.source)
}

/// Emit CUDA source from an already admitted unary i32 compute kernel.
/// Admission and fusion remain in the backend-neutral compute layer.
pub(crate) fn emit_i32_compute_kernel(kernel: &ComputeKernel) -> Result<String, CudaEmitError> {
    if kernel.parameter_count != 1 {
        return Err(CudaEmitError::UnsupportedRegion);
    }
    let expression = emit_i32_expr(&kernel.body)?;
    Ok(render_map_kernel("int", &expression))
}

fn render_map_kernel(element_type: &str, expression: &str) -> String {
    format!(
        "extern \"C\" __global__ void cml_map(\n\
         const {element_type} *input_data,\n\
         {element_type} *output_data,\n\
         unsigned int length) {{\n\
           unsigned int i = blockIdx.x * blockDim.x + threadIdx.x;\n\
           if (i >= length) return;\n\
           {element_type} x = input_data[i];\n\
           output_data[i] = {expression};\n\
         }}\n"
    )
}

fn emit_i32_expr(expression: &ScalarExpr) -> Result<String, CudaEmitError> {
    match expression {
        ScalarExpr::Parameter(0) => Ok("x".into()),
        ScalarExpr::Parameter(_) => Err(CudaEmitError::UnsupportedRegion),
        ScalarExpr::ExactInteger(value) => Ok(value.to_string()),
        ScalarExpr::CheckedAdd(left, right) => Ok(format!(
            "({} + {})",
            emit_i32_expr(left)?,
            emit_i32_expr(right)?
        )),
        // #368/#379: outside the proven-integer subset; the float path
        // owns these nodes (see `F32MapKernel`).
        ScalarExpr::Float32(_)
        | ScalarExpr::Mul(..)
        | ScalarExpr::Sub(..)
        | ScalarExpr::Div(..)
        | ScalarExpr::Sqrt(_) => Err(CudaEmitError::UnsupportedRegion),
    }
}

fn cuda_f32(value: f32) -> String {
    if value.fract() == 0.0 {
        format!("{value:.1}f")
    } else {
        format!("{value}f")
    }
}

/// Breakdown of time spent across compilation, JIT loading, memory transfers,
/// and kernel execution on the concrete GPU target (#491).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CudaLatencyBreakdown {
    /// Time to lower CML IR to CudaMapKernel representation.
    pub cml_ir_lowering_ns: u64,
    /// Time for NVRTC to compile CUDA C++ source to PTX.
    pub nvrtc_compile_ns: u64,
    /// Time for CUDA Driver API to JIT/load PTX into a device module.
    pub driver_jit_load_ns: u64,
    /// Time to look up the kernel function entry point in the loaded module.
    pub function_lookup_ns: u64,
    /// Time to transfer input buffer from Host to Device.
    pub htod_transfer_ns: u64,
    /// Time to execute the kernel on device (including stream sync).
    pub kernel_execution_ns: u64,
    /// Time to transfer output buffer from Device to Host (materialization).
    pub dtoh_transfer_ns: u64,
    /// Total cold latency (lowering + NVRTC + driver JIT + lookup + HtoD + launch + DtoH).
    pub total_cold_ns: u64,
    /// Total warm execution latency with pre-compiled kernel and reusable context/buffers.
    pub total_warm_ns: u64,
    /// Reference execution time on CPU compute backend.
    pub cpu_reference_ns: u64,
}

impl CudaLatencyBreakdown {
    /// Ratio of cold compile+JIT+execution over warm reuse latency.
    pub fn cold_over_warm_ratio(&self) -> f64 {
        if self.total_warm_ns == 0 {
            0.0
        } else {
            self.total_cold_ns as f64 / self.total_warm_ns as f64
        }
    }

    /// Speedup of warm GPU execution compared to CPU reference (CPU / warm GPU).
    pub fn warm_speedup_over_cpu(&self) -> f64 {
        if self.total_warm_ns == 0 {
            0.0
        } else {
            self.cpu_reference_ns as f64 / self.total_warm_ns as f64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compiler_target_exposes_nvidia_driver_jit() {
        assert_eq!(
            CudaCompilerTarget::NvidiaDriverJit.name(),
            "NvidiaDriverJit"
        );
    }

    #[test]
    fn lower_compute_kernel_accepts_map_and_reduce_through_one_boundary() {
        let map_exprs = crate::parser::parse("(numeric-buffer-map (lambda (x) (+ x 1)) #i32(10 20 30))").unwrap();
        let map_ir = crate::lower::lower_program(&map_exprs).unwrap().remove(0);
        assert!(matches!(
            lower_compute_kernel(&map_ir),
            Ok(CudaKernelArtifact::Map(_))
        ));
    }

    #[test]
    fn lower_compute_kernel_accepts_bounded_associative_i32_reduce() {
        let expressions = crate::parser::parse("(reduce + 7 #i32(10 20 30))").unwrap();
        let ir = crate::lower::lower_program(&expressions).unwrap().remove(0);
        let artifact = lower_compute_kernel(&ir).expect("admitted reduce must lower");
        match artifact {
            CudaKernelArtifact::Reduce(kernel) => {
                assert_eq!(kernel.initial, 7);
                assert!(kernel.source.contains("atomicAdd(output_data, initial);"));
            }
            CudaKernelArtifact::Map(_) => panic!("reduce lowered as map"),
        }
    }

    #[test]
    fn lower_compute_kernel_rejects_non_associative_reduce_before_emission() {
        let expressions =
            crate::parser::parse("(reduce (lambda (acc x) (+ (+ acc acc) x)) 0 #i32(1 2 3))")
                .unwrap();
        let ir = crate::lower::lower_program(&expressions).unwrap().remove(0);
        assert!(matches!(
            lower_compute_kernel(&ir),
            Err(CudaEmitError::NotEligible(_))
        ));
    }

    #[test]
    fn fused_i32_emitter_preserves_scalar_grouping() {
        let kernel = ComputeKernel {
            parameter_count: 1,
            body: ScalarExpr::CheckedAdd(
                Box::new(ScalarExpr::CheckedAdd(
                    Box::new(ScalarExpr::Parameter(0)),
                    Box::new(ScalarExpr::ExactInteger(1)),
                )),
                Box::new(ScalarExpr::ExactInteger(2)),
            ),
        };
        let source = emit_i32_compute_kernel(&kernel).expect("fused kernel should emit");
        assert!(source.contains("output_data[i] = ((x + 1) + 2);"));
    }
}
