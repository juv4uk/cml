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

pub fn emit_map_kernel(ir: &Ir) -> Result<String, CudaEmitError> {
    let analysis = analyze(ir);
    if !analysis.gpu_eligible() {
        return Err(CudaEmitError::NotEligible(analysis.gpu_blockers));
    }
    let region = analysis.region.ok_or(CudaEmitError::UnsupportedRegion)?;
    if region.operation != BulkOperation::Map {
        return Err(CudaEmitError::UnsupportedRegion);
    }
    let kernel = region.kernel.ok_or(CudaEmitError::UnsupportedRegion)?;

    let (element_type, expression) = match analysis.numeric_domain {
        NumericDomain::FixedWidthInteger => ("int", emit_i32_expr(&kernel.body)?),
        NumericDomain::InexactFloat => {
            let form =
                F32MapKernel::lower(&kernel.body).ok_or(CudaEmitError::UnsupportedRegion)?;
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
            };
            ("float", expression)
        }
        _ => return Err(CudaEmitError::UnsupportedRegion),
    };

    Ok(render_map_kernel(element_type, &expression))
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
        // #368: outside the proven-integer subset; the float path owns
        // these nodes (see `f32_scale_offset`).
        ScalarExpr::Float32(_) | ScalarExpr::Mul(..) => Err(CudaEmitError::UnsupportedRegion),
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
