//! Portable GPU source emission for admitted CML Compute IR.
//!
//! This module emits WGSL only. Device selection, buffer transfer, dispatch,
//! and readback belong to the later `wgpu` runtime slice.

use crate::compute::{
    analyze, f32_affine_offset, AdmissionBlocker, BulkOperation, NumericDomain, ScalarExpr,
};
use crate::ir::Ir;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WgslError {
    NotEligible(Vec<AdmissionBlocker>),
    UnsupportedRegion,
}

pub fn emit_map_shader(ir: &Ir) -> Result<String, WgslError> {
    let analysis = analyze(ir);
    if !analysis.gpu_eligible() {
        return Err(WgslError::NotEligible(analysis.gpu_blockers));
    }
    let region = analysis.region.ok_or(WgslError::UnsupportedRegion)?;
    if region.operation != BulkOperation::Map {
        return Err(WgslError::UnsupportedRegion);
    }
    let kernel = region.kernel.ok_or(WgslError::UnsupportedRegion)?;

    let (element_type, expression) = match analysis.numeric_domain {
        NumericDomain::FixedWidthInteger => ("i32", emit_i32_expr(&kernel.body)?),
        NumericDomain::InexactFloat => {
            let offset = f32_affine_offset(&kernel.body).ok_or(WgslError::UnsupportedRegion)?;
            ("f32", format!("x + {}", wgsl_f32(offset as f32)))
        }
        _ => return Err(WgslError::UnsupportedRegion),
    };

    Ok(format!(
        "@group(0) @binding(0) var<storage, read> input_data: array<{element_type}>;\n\
         @group(0) @binding(1) var<storage, read_write> output_data: array<{element_type}>;\n\n\
         @compute @workgroup_size(64)\n\
         fn main(@builtin(global_invocation_id) gid: vec3<u32>) {{\n\
           let i = gid.x;\n\
           if (i >= arrayLength(&input_data)) {{ return; }}\n\
           let x = input_data[i];\n\
           output_data[i] = {expression};\n\
         }}\n"
    ))
}

fn emit_i32_expr(expression: &ScalarExpr) -> Result<String, WgslError> {
    match expression {
        ScalarExpr::Parameter(0) => Ok("x".to_string()),
        ScalarExpr::Parameter(_) => Err(WgslError::UnsupportedRegion),
        ScalarExpr::ExactInteger(value) => Ok(format!("{value}i")),
        ScalarExpr::CheckedAdd(left, right) => Ok(format!(
            "({} + {})",
            emit_i32_expr(left)?,
            emit_i32_expr(right)?
        )),
    }
}

fn wgsl_f32(value: f32) -> String {
    if value.fract() == 0.0 {
        format!("{value:.1}")
    } else {
        value.to_string()
    }
}
