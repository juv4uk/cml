use std::fs;

#[test]
fn machine_readable_compute_contract_matches_implementation() {
    let contract = fs::read_to_string("compute-contract.my").unwrap();
    for required in [
        "(version . (0 28))",
        "(status . experimental-runtime)",
        "(unknown-facts . reject)",
        "(fallback . cpu)",
        "(implicit-exact-to-inexact-conversion . forbidden)",
        "(typed-buffer-ir . present-from-my-lisp-contract-2.2)",
        "(kernel-ir . (parameter exact-integer checked-add primitive-and-first-class-add))",
        "(required-i32-overflow-proof . true)",
        "(f32-rounding-contract . affine-x-plus-integer-per-literal-bit-equivalence)",
        "(cpu-compute-backend . (present-reference i32-map-range-proven f32-map-affine-proven))",
        "(c-backend-reference . (i32-buffer-literal source-level-i32-map cpu-differential checked-overflow checked-allocation))",
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
        "(implemented-executors . (cpu gpu-cuda gpu-wgpu fpga-command-bridge))",
        "(backend-registration . explicit-name-to-node-executor)",
        "(gpu . (nvidia-gtx-1050-ti cuda-device-zero graph-map-pass))",
        "(fpga . (gw5a-25a windows-com4 graph-program-pass tagged-word-7))",
        "(heterogeneous . (cpu cuda fpga ordered-live-graph-pass))",
        "(execution-order . (cpu cuda fpga))",
        "(fpga . one-program-path-not-blanket-conformance)",
        "(heterogeneous . scheduler-order-results-and-host-staged-fpga-input-not-direct-device-transfer)",
        "(cross-device-payload-transfer . (direct-device-to-device-absent host-staged-buffer-to-register-proven))",
        "(data . typed-value-with-explicit-producer-dependency)",
        "(control . dependency-only)",
        "(buffer-transfer . host-staged-materialized-graph-value)",
        "(fpga-input-edge . (host-staged-register-input-explicit control-dependency-only-for-jobs-without-input))",
        "(implicit-source-order-dataflow . forbidden)",
        "(unregistered-targets . fail-closed)",
        "(publication . atomic-on-whole-graph-success)",
        "(raw-cross-device-pointers . forbidden)",
        "(values . (buffer lisp-word))",
        "(operations . (numeric-buffer-map fpga-program fpga-program-with-buffer-input))",
        "(fpga-job-protocol . ((version . 1)",
        "(status . command-transport-m2c)",
        "(wire-authority . fpga-lisp-isa-1.1)",
        "(extended-register-inputs . (rtl-simulation-proven host-bridge-targeted-tested physical-com4-confirmed-one-path))",
        "(cml-register-input-encoding . present-validated)",
        "(register-input-contract . (maximum-16 unique-registers-r0-r15 opaque-u32-tagged-words))",
        "(execution-graph-attachment . (mock-transport-confirmed physical-com4-confirmed-typed-buffer-input))",
        "(physical-bridge . (windows-python-pyserial-com4))",
        "(physical-device-evidence . (gw5a-25a windows-com4 graph-program-pass tagged-word-7 no-hardware-error))",
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
        compatibility.contains("(compute-analysis . ((contract . (0 28))"),
        "compatibility.my compute contract version drifted from compute-contract.my"
    );
}
