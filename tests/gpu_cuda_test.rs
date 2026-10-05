use cml::compute::{AdmissionBlocker, NumericDomain};
use cml::gpu_cuda::{
    CudaArtifactCache, CudaComputeCapability, CudaDriverJitCacheKey, CudaDriverJitModuleArtifact,
    CudaElementType, CudaEmitError, CudaPtxArtifact, CudaPtxCacheKey, NvrtcVersion,
    emit_map_kernel, lower_map_kernel,
};
use cml::ir::{BufferLiteral, Ir, Params};
use cml::{lower, parser};

fn lower_one(source: &str) -> Ir {
    let expressions = parser::parse(source).unwrap();
    lower::lower_program(&expressions).unwrap().remove(0)
}

fn f32_map_ir(values: &[f32], body: Ir) -> Ir {
    Ir::App {
        func: Box::new(Ir::Sid(sens::sens!(01011001))),
        args: vec![
            Ir::Lambda {
                params: Params::Fixed(vec!["X".to_string()]),
                body: Box::new(body),
            },
            Ir::Buffer(BufferLiteral::F32(
                values.iter().map(|value| value.to_bits()).collect(),
            )),
        ],
    }
}

#[test]
fn admitted_i32_map_lowers_to_typed_cuda_artifact() {
    let artifact = lower_map_kernel(&lower_one(
        "(numeric-buffer-map (lambda (x) (+ x 1)) #i32(1 2 3))",
    ))
    .unwrap();

    assert_eq!(artifact.identity, sens::sens!(01011001));
    assert_eq!(artifact.numeric_domain, NumericDomain::FixedWidthInteger);
    assert_eq!(artifact.element_type, CudaElementType::I32);
    assert_eq!(artifact.parameter_count, 1);
    assert_eq!(artifact.entry_point, "cml_map");
    assert!(
        artifact
            .source
            .contains("extern \"C\" __global__ void cml_map")
    );
    assert!(artifact.source.contains("const int *input_data"));
    assert!(artifact.source.contains("if (i >= length) return;"));
    assert!(artifact.source.contains("output_data[i] = (x + 1);"));
}

#[test]
fn compatibility_emitter_preserves_lowered_source() {
    let ir = lower_one("(numeric-buffer-map (lambda (x) (+ x 1)) #i32(1 2 3))");
    assert_eq!(
        emit_map_kernel(&ir).unwrap(),
        lower_map_kernel(&ir).unwrap().source
    );
}

#[test]
fn dormant_f32_ir_emits_one_binary32_add() {
    // Source-level F32 зараз не admitted. Явний IR не послаблює цю межу,
    // а лише зберігає перевірку вже наявного CUDA emitter-а.
    let source = emit_map_kernel(&f32_map_ir(
        &[1.0, 2.0],
        Ir::App {
            func: Box::new(Ir::Sid(sens::sens!(00001100))),
            args: vec![Ir::Var("X".to_string()), Ir::Int(3)],
        },
    ))
    .unwrap();
    assert!(source.contains("const float *input_data"));
    assert!(source.contains("output_data[i] = x + 3.0f;"));
    assert!(!source.contains("(x + 1) + 2"));
}

#[test]
fn cuda_emitter_cannot_bypass_semantic_admission() {
    let error = emit_map_kernel(&lower_one(
        "(numeric-buffer-map (lambda (x) (+ x 1)) #i32(2147483647))",
    ))
    .unwrap_err();
    assert!(matches!(
        error,
        CudaEmitError::NotEligible(blockers)
            if blockers.contains(&AdmissionBlocker::IntegerOverflowNotProven)
    ));
}

#[test]
fn ptx_cache_key_deterministic_and_sensitive_to_exact_provenance() {
    let kernel = lower_map_kernel(&lower_one(
        "(numeric-buffer-map (lambda (x) (+ x 1)) #i32(1 2 3))",
    ))
    .unwrap();
    let digest = kernel.kernel_digest();

    let base_key = CudaPtxCacheKey::new(
        digest.clone(),
        CudaComputeCapability::new(7, 5),
        Some(NvrtcVersion::new(12, 2)),
        vec!["-arch=compute_75".to_string()],
    );

    // Identical inputs produce identical key and digest.
    let identical_key = CudaPtxCacheKey::new(
        digest.clone(),
        CudaComputeCapability::new(7, 5),
        Some(NvrtcVersion::new(12, 2)),
        vec!["-arch=compute_75".to_string()],
    );
    assert_eq!(base_key, identical_key);
    assert_eq!(base_key.digest(), identical_key.digest());

    // Compile options mismatch (e.g. -fmad=false witness mode) causes cache miss.
    let fmad_key = CudaPtxCacheKey::new(
        digest.clone(),
        CudaComputeCapability::new(7, 5),
        Some(NvrtcVersion::new(12, 2)),
        vec!["-arch=compute_75".to_string(), "-fmad=false".to_string()],
    );
    assert_ne!(base_key, fmad_key);
    assert_ne!(base_key.digest(), fmad_key.digest());

    // Compute capability mismatch causes cache miss.
    let cc80_key = CudaPtxCacheKey::new(
        digest.clone(),
        CudaComputeCapability::new(8, 0),
        Some(NvrtcVersion::new(12, 2)),
        vec!["-arch=compute_80".to_string()],
    );
    assert_ne!(base_key, cc80_key);
    assert_ne!(base_key.digest(), cc80_key.digest());

    // NVRTC version change causes cache miss.
    let nvrtc123_key = CudaPtxCacheKey::new(
        digest.clone(),
        CudaComputeCapability::new(7, 5),
        Some(NvrtcVersion::new(12, 3)),
        vec!["-arch=compute_75".to_string()],
    );
    assert_ne!(base_key, nvrtc123_key);
    assert_ne!(base_key.digest(), nvrtc123_key.digest());

    // Different kernel semantics/digest causes cache miss.
    let diff_kernel = lower_map_kernel(&lower_one(
        "(numeric-buffer-map (lambda (x) (+ x 2)) #i32(1 2 3))",
    ))
    .unwrap();
    let diff_key = CudaPtxCacheKey::new(
        diff_kernel.kernel_digest(),
        CudaComputeCapability::new(7, 5),
        Some(NvrtcVersion::new(12, 2)),
        vec!["-arch=compute_75".to_string()],
    );
    assert_ne!(base_key, diff_key);
    assert_ne!(base_key.digest(), diff_key.digest());
}

#[test]
fn driver_jit_cache_key_sensitive_to_device_and_driver_provenance() {
    let ptx_digest = "fnv1a64:0123456789abcdef";

    let base_key =
        CudaDriverJitCacheKey::new(ptx_digest, 0, CudaComputeCapability::new(7, 5), Some(12020));

    // Identical inputs produce identical key and digest.
    let identical_key =
        CudaDriverJitCacheKey::new(ptx_digest, 0, CudaComputeCapability::new(7, 5), Some(12020));
    assert_eq!(base_key, identical_key);
    assert_eq!(base_key.digest(), identical_key.digest());

    // Different device ordinal causes miss.
    let dev1_key =
        CudaDriverJitCacheKey::new(ptx_digest, 1, CudaComputeCapability::new(7, 5), Some(12020));
    assert_ne!(base_key, dev1_key);
    assert_ne!(base_key.digest(), dev1_key.digest());

    // Driver version change causes miss.
    let driver_new_key =
        CudaDriverJitCacheKey::new(ptx_digest, 0, CudaComputeCapability::new(7, 5), Some(12040));
    assert_ne!(base_key, driver_new_key);
    assert_ne!(base_key.digest(), driver_new_key.digest());

    // Module artifact creation is deterministic.
    let module_art = CudaDriverJitModuleArtifact::new(base_key.clone(), "cml_map");
    assert_eq!(module_art.entry_point, "cml_map");
    assert!(!module_art.module_digest.is_empty());
}

#[test]
fn artifact_cache_tracks_ptx_and_driver_artifacts_with_bounded_eviction() {
    let mut cache = CudaArtifactCache::new(2);

    let k1 = CudaPtxCacheKey::new("k1", (7, 5), None, vec![]);
    let k2 = CudaPtxCacheKey::new("k2", (7, 5), None, vec![]);
    let k3 = CudaPtxCacheKey::new("k3", (7, 5), None, vec![]);

    // Initially miss.
    assert!(cache.get_ptx(&k1).is_none());
    assert_eq!(cache.diagnostics().ptx_misses, 1);

    // Insert 1 & 2 up to capacity.
    cache.insert_ptx(CudaPtxArtifact::new(
        k1.clone(),
        ".version 7.5\n// ptx1".into(),
    ));
    cache.insert_ptx(CudaPtxArtifact::new(
        k2.clone(),
        ".version 7.5\n// ptx2".into(),
    ));

    assert_eq!(cache.get_ptx(&k1).unwrap().ptx, ".version 7.5\n// ptx1");
    assert_eq!(cache.diagnostics().ptx_hits, 1);

    // Insert 3 exceeds capacity 2 -> evicts k1 (oldest).
    cache.insert_ptx(CudaPtxArtifact::new(
        k3.clone(),
        ".version 7.5\n// ptx3".into(),
    ));
    assert_eq!(cache.diagnostics().ptx_evictions, 1);

    // k1 is now evicted (miss), while k2 and k3 are present.
    assert!(cache.get_ptx(&k1).is_none());
    assert_eq!(cache.diagnostics().ptx_misses, 2);
    assert!(cache.get_ptx(&k2).is_some());
    assert!(cache.get_ptx(&k3).is_some());
    assert_eq!(cache.diagnostics().ptx_hits, 3);

    // Test driver module cache eviction on the same cache instance.
    let m1 = CudaDriverJitCacheKey::new("ptx1", 0, (7, 5), None);
    let m2 = CudaDriverJitCacheKey::new("ptx2", 0, (7, 5), None);
    let m3 = CudaDriverJitCacheKey::new("ptx3", 0, (7, 5), None);

    cache.insert_module(CudaDriverJitModuleArtifact::new(m1.clone(), "cml_map"));
    cache.insert_module(CudaDriverJitModuleArtifact::new(m2.clone(), "cml_map"));
    assert!(cache.get_module(&m1).is_some());
    assert_eq!(cache.diagnostics().module_hits, 1);

    cache.insert_module(CudaDriverJitModuleArtifact::new(m3.clone(), "cml_map"));
    assert_eq!(cache.diagnostics().module_evictions, 1);
    assert!(cache.get_module(&m1).is_none());
    assert_eq!(cache.diagnostics().module_misses, 1);
    assert!(cache.get_module(&m2).is_some());
    assert!(cache.get_module(&m3).is_some());
    assert_eq!(cache.diagnostics().module_hits, 3);
}

#[test]
fn unadmitted_region_fails_closed_before_artifact_or_cache_key() {
    // Overflow not proven cannot lower to CudaMapKernel.
    let ir = lower_one("(numeric-buffer-map (lambda (x) (+ x 1)) #i32(2147483647))");
    assert!(lower_map_kernel(&ir).is_err());
    // Since lower_map_kernel fails closed, no CudaMapKernel exists,
    // so no valid kernel_digest or CudaPtxCacheKey can be created for it.
}
