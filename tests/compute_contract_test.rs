use std::fs;

#[test]
fn machine_readable_compute_contract_matches_implementation() {
    let contract = fs::read_to_string("compute-contract.my").unwrap();
    for required in [
        "(version . (0 19))",
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
        "live-device-discovery",
        "planner-descriptor",
        "(gpu-runtime . (optional-feature gpu-wgpu",
        "default-policy-gpu-only",
        "explicit-software-adapter-probe",
        "(runtime-live-evidence . llvmpipe-cpu-vulkan-pass)",
        "(physical-gpu-evidence . (nvidia-gtx-1050-ti",
        "(accelerator-selection . (live-descriptors-only",
        "capability-status-before-selection",
        "(nvidia . (wgpu cuda-live))",
        "(amd . (wgpu rocm-planned))",
        "(intel . (wgpu oneapi-level-zero-planned))",
        "(accelerator-planner . \"src/accelerator.rs\")",
        "(execution-graph . ((status . registered-backends-m1)",
        "(targets . (cpu gpu fpga))",
        "(implemented-executors . (cpu gpu-cuda gpu-wgpu))",
        "(backend-registration . explicit-name-to-node-executor)",
        "(physical-evidence . (nvidia-gtx-1050-ti cuda-device-zero graph-map-pass))",
        "(unregistered-targets . fail-closed)",
        "(publication . atomic-on-whole-graph-success)",
        "(raw-cross-device-pointers . forbidden)",
        "(values . (buffer lisp-word))",
        "(operations . (numeric-buffer-map fpga-program))",
        "(fpga-job-protocol . ((version . 1)",
        "(status . host-contract-m2a)",
        "(wire-authority . fpga-lisp-isa-1.0)",
        "(execution-graph-attachment . mock-transport-confirmed)",
        "(physical-transport . pending)",
        "(fpga-job-protocol . \"src/fpga_transport.rs\")",
        "(execution-graph . \"src/execution.rs\")",
        "(wgpu-runtime . \"src/gpu_wgpu_runtime.rs\")",
        "(cuda-emitter . \"src/gpu_cuda.rs\")",
        "(cuda-runtime . \"src/gpu_cuda_runtime.rs\")",
    ] {
        assert!(
            contract.contains(required),
            "compute contract lost {required}"
        );
    }

    let compatibility = fs::read_to_string("compatibility.my").unwrap();
    assert!(
        compatibility.contains("(compute-analysis . ((contract . (0 19))"),
        "compatibility.my compute contract version drifted from compute-contract.my"
    );
}
