//! NVIDIA CUDA source emission for admitted CML Compute IR.
//!
//! Like the portable WGSL emitter, this module cannot bypass semantic
//! admission. NVRTC compilation, device transfer, launch, and readback belong
//! to the optional CUDA runtime layer.

use crate::compute::{
    AdmissionBlocker, BulkOperation, NumericDomain, ScalarExpr, analyze, f32_affine_offset,
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
            let offset =
                f32_affine_offset(&kernel.body).ok_or(CudaEmitError::UnsupportedRegion)? as f32;
            ("float", format!("x + {}", cuda_f32(offset)))
        }
        _ => return Err(CudaEmitError::UnsupportedRegion),
    };

    Ok(format!(
        "extern \"C\" __global__ void cml_map(\n\
         const {element_type} *input_data,\n\
         {element_type} *output_data,\n\
         unsigned int length) {{\n\
           unsigned int i = blockIdx.x * blockDim.x + threadIdx.x;\n\
           if (i >= length) return;\n\
           {element_type} x = input_data[i];\n\
           output_data[i] = {expression};\n\
         }}\n"
    ))
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
    }
}

fn cuda_f32(value: f32) -> String {
    if value.fract() == 0.0 {
        format!("{value:.1}f")
    } else {
        format!("{value}f")
    }
}
