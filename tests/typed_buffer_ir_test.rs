use cml::compute::{analyze, EffectClass, ExecutionShape, NumericDomain, StorageClass};
use cml::ir::{BufferLiteral, Ir};
use cml::{c_backend::CBackend, compiler::Compiler, lower, parser};

fn lower_one(source: &str) -> Ir {
    let expressions = parser::parse(source).unwrap();
    lower::lower_program(&expressions).unwrap().remove(0)
}

#[test]
fn canonical_i32_buffer_lowers_without_losing_width_or_order() {
    assert_eq!(
        lower_one("#i32(1 -2 2147483647)"),
        Ir::Buffer(BufferLiteral::I32(vec![1, -2, i32::MAX]))
    );
}

#[test]
fn canonical_f32_buffer_preserves_binary32_bits() {
    assert_eq!(
        lower_one("#f32(-0.0 0.1 3.0)"),
        Ir::Buffer(BufferLiteral::F32(vec![
            (-0.0_f32).to_bits(),
            0.1_f32.to_bits(),
            3.0_f32.to_bits(),
        ]))
    );
}

#[test]
fn ratified_buffer_makes_map_a_fail_closed_gpu_candidate() {
    let analysis = analyze(&lower_one(
        "(map (lambda (x) (+ x 1)) #i32(1 2 3))",
    ));
    assert_eq!(analysis.shape, ExecutionShape::ElementWise);
    assert_eq!(analysis.effect, EffectClass::Pure);
    assert_eq!(analysis.storage, StorageClass::ContiguousBuffer);
    assert_eq!(analysis.numeric_domain, NumericDomain::FixedWidthInteger);
    assert!(analysis.gpu_eligible());
}

#[test]
fn existing_backends_reject_buffers_explicitly_until_their_abis_exist() {
    let program = vec![lower_one("#i32(1 2 3)")];
    let fpga_error = Compiler::new().compile(&program).unwrap_err().to_string();
    assert!(fpga_error.contains("not supported by the fpga-lisp backend"));

    let c_error = CBackend::new()
        .compile_program(&program)
        .unwrap_err()
        .to_string();
    assert!(c_error.contains("CPU ComputeBackend not implemented yet"));
}

#[test]
fn malformed_or_non_finite_buffer_literals_fail_in_the_frontend() {
    assert!(parser::parse("#i32(2147483648)").is_err());
    assert!(parser::parse("#f32(inf)").is_err());
    assert!(parser::parse("#i32(1/2)").is_err());
}
