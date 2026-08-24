use std::fs;

#[test]
fn machine_readable_compute_contract_matches_analysis_m0() {
    let contract = fs::read_to_string("compute-contract.my").unwrap();
    for required in [
        "(version . (0 13))",
        "(status . experimental-runtime)",
        "(unknown-facts . reject)",
        "(fallback . cpu)",
        "(implicit-exact-to-inexact-conversion . forbidden)",
        "(typed-buffer-ir . present-from-my-lisp-contract-2.2)",
        "(kernel-ir . (parameter exact-integer checked-add))",
        "(required-i32-overflow-proof . true)",
        "(f32-rounding-contract . affine-x-plus-integer-per-literal-bit-equivalence)",
        "(cpu-compute-backend . (present-reference i32-map-range-proven f32-map-affine-proven))",
        "(differential-oracle . \"my-lisp path dev-dependency\")",
        "(gpu-emitter . (wgsl-source-only admitted-map-regions))",
        "(cuda-emitter . (cuda-c-source-only admitted-map-regions))",
        "(cuda-runtime . (optional-feature gpu-cuda",
        "(gpu-runtime . (optional-feature gpu-wgpu",
        "default-policy-gpu-only",
        "explicit-software-adapter-probe",
        "(runtime-live-evidence . llvmpipe-cpu-vulkan-pass)",
        "(physical-gpu-evidence . (nvidia-gtx-1050-ti",
        "(accelerator-selection . (live-descriptors-only",
        "(nvidia . (wgpu cuda-live))",
        "(amd . (wgpu rocm-planned))",
        "(intel . (wgpu oneapi-level-zero-planned))",
        "(accelerator-planner . \"src/accelerator.rs\")",
        "(wgpu-runtime . \"src/gpu_wgpu_runtime.rs\")",
        "(cuda-emitter . \"src/gpu_cuda.rs\")",
        "(cuda-runtime . \"src/gpu_cuda_runtime.rs\")",
    ] {
        assert!(contract.contains(required), "compute contract lost {required}");
    }
}
