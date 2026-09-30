//! Sens GPU-2 witness adapter, cml leg (sens#1585 E1–E3; cml#360, cml#368).
//!
//! Mechanism only: this adapter owns no Lisp expected answers and checks none.
//! It implements the witness protocol of sens `experiments/gpu2-e1e3/witness.py`:
//!
//! * reads `WITNESS_CASE` (E1–E3 case name) and `WITNESS_EXECUTOR` (`cpu`,
//!   default, or `cuda`);
//! * on success prints `BITS=xxxxxxxx` — the stored IEEE-754 binary32 bits of
//!   the first output element — and exits 0;
//! * on any stage failure (parse, lower, admission, executor) prints a
//!   `WITNESS-UNAVAILABLE <stage>: <reason>` line to stderr and exits 1:
//!   a named state, never a silent green.
//!
//! Today the compute region is additive (cml#368), so the E1–E3 cases stop at
//! a named stage. When the multiplication slice lands, the same commands go
//! green without changes to this file.

use std::env;
use std::process::ExitCode;

use cml::compute::{ComputeBackend, CpuComputeBackend};
use cml::ir::{BufferLiteral, Ir};
use cml::{lower, parser};

/// Best-effort cml-surface translations of the sens witness case templates
/// (sens#1585 E1–E3; constants are the ratified §1 triple).
fn case_source(case: &str) -> Option<&'static str> {
    Some(match case {
        // E1 (A1): a*b+c as separate mul+add.
        "e1_muladd" => {
            "(numeric-buffer-map (lambda (x) (+ (* x -1.0399841) 0.31713876)) #f32(0.3047171))"
        }
        // E2 (B1): NaN tag; payload is not compared.
        "e2_qnan_0div0" => "(numeric-buffer-map (lambda (x) (/ x 0.0)) #f32(0.0))",
        "e2_qnan_inf_minus_inf" => {
            "(numeric-buffer-map (lambda (x) (- (/ 1.0 0.0) (/ 1.0 0.0))) #f32(1.0))"
        }
        "e2_qnan_sqrt_neg" => "(numeric-buffer-map (lambda (x) (sqrt x)) #f32(-1.0))",
        // E3: signed zeros.
        "e3_add_pos0_neg0" => "(numeric-buffer-map (lambda (x) (+ x -0.0)) #f32(0.0))",
        "e3_mul_neg1_pos0" => "(numeric-buffer-map (lambda (x) (* x 0.0)) #f32(-1.0))",
        _ => return None,
    })
}

fn main() -> ExitCode {
    let case = env::var("WITNESS_CASE").unwrap_or_default();
    let executor = env::var("WITNESS_EXECUTOR").unwrap_or_else(|_| "cpu".to_string());

    let Some(source) = case_source(&case) else {
        eprintln!("WITNESS-UNAVAILABLE protocol: unknown WITNESS_CASE '{case}'");
        return ExitCode::FAILURE;
    };

    let expressions = match parser::parse(source) {
        Ok(expressions) => expressions,
        Err(error) => {
            eprintln!("WITNESS-UNAVAILABLE parse: {error}");
            return ExitCode::FAILURE;
        }
    };
    let ir = match lower::lower_expr(&expressions[0]) {
        Ok(ir) => ir,
        Err(error) => {
            eprintln!("WITNESS-UNAVAILABLE lower: {error}");
            return ExitCode::FAILURE;
        }
    };

    let output = match executor.as_str() {
        "cpu" => match CpuComputeBackend.execute(&ir) {
            Ok(output) => output,
            Err(error) => {
                eprintln!("WITNESS-UNAVAILABLE cpu-admission: {error:?}");
                return ExitCode::FAILURE;
            }
        },
        "cuda" => match cuda_execute(&ir) {
            Ok(output) => output,
            Err(reason) => {
                eprintln!("WITNESS-UNAVAILABLE {reason}");
                return ExitCode::FAILURE;
            }
        },
        other => {
            eprintln!("WITNESS-UNAVAILABLE protocol: unknown WITNESS_EXECUTOR '{other}'");
            return ExitCode::FAILURE;
        }
    };

    let bits = match output {
        BufferLiteral::F32(bits) if !bits.is_empty() => bits[0],
        BufferLiteral::F32(_) => {
            eprintln!("WITNESS-UNAVAILABLE protocol: empty f32 output buffer");
            return ExitCode::FAILURE;
        }
        BufferLiteral::I32(_) => {
            eprintln!("WITNESS-UNAVAILABLE protocol: case produced i32 output, expected f32");
            return ExitCode::FAILURE;
        }
    };

    println!("BITS={bits:08x}");
    ExitCode::SUCCESS
}

/// CUDA leg: device 0, witness mode (NVRTC `-fmad=false`, cml#360 / sens#1585 A1).
#[cfg(feature = "gpu-cuda")]
fn cuda_execute(ir: &Ir) -> Result<BufferLiteral, String> {
    let session = cml::gpu_cuda_runtime::CudaSession::new(0)
        .map_err(|error| format!("cuda-session: {error:?}"))?;
    let execution = session
        .execute_map_with_mode(ir, cml::gpu_cuda_runtime::CudaKernelMode::BitwiseEquality)
        .map_err(|error| format!("cuda-admission: {error:?}"))?;
    Ok(execution.output)
}

#[cfg(not(feature = "gpu-cuda"))]
fn cuda_execute(_ir: &Ir) -> Result<BufferLiteral, String> {
    Err(
        "cuda-executor: cml was built without the gpu-cuda feature; rebuild with --features gpu-cuda"
            .to_string(),
    )
}
