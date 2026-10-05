//! NVIDIA CUDA source emission for admitted CML Compute IR.
//!
//! Like the portable WGSL emitter, this module cannot bypass semantic
//! admission. NVRTC compilation, device transfer, launch, and readback belong
//! to the optional CUDA runtime layer.

use crate::compute::{
    AdmissionBlocker, BulkOperation, ComputeKernel, F32MapKernel, NumericDomain, ScalarExpr,
    analyze,
};
use crate::ir::Ir;

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

#[cfg(test)]
mod tests {
    use super::*;

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
