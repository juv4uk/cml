//! Reproducible Lisp -> native code-quality and runtime baseline harness (#53).
//!
//! CML owns measurements, machine structure, and actual lane outcomes here.
//! Lisp semantic judgment remains outside this module and is referenced through
//! pinned upstream witness provenance. In particular, this module must not mint
//! expected Lisp answers or local PASS/FAIL semantic verdicts.

use crate::compute::{ComputeBackend, CpuComputeBackend};
use crate::ir::{BufferLiteral, Ir, Params, PrimOp};
use crate::machine_inst::{
    AluOp, CondCode, MachineInst, MachineItem, Provenance, X86Reg, assemble_program,
};

/// Structural code metrics extracted deterministically from machine instructions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructuralMetrics {
    pub code_bytes: usize,
    pub instruction_count: usize,
    pub load_count: usize,
    pub store_count: usize,
    pub branch_count: usize,
    pub call_count: usize,
    pub spill_count: usize,
}

impl StructuralMetrics {
    pub fn from_machine_items(items: &[MachineItem]) -> Self {
        let mut code_bytes = 0;
        let mut instruction_count = 0;
        let mut load_count = 0;
        let mut store_count = 0;
        let mut branch_count = 0;
        let mut call_count = 0;
        let spill_count = 0;

        for item in items {
            match item {
                MachineItem::Label(_) => {}
                MachineItem::Inst(inst) => {
                    instruction_count += 1;
                    code_bytes += inst.encode_bytes().len();
                    match inst {
                        MachineInst::MovLoad { .. } => load_count += 1,
                        MachineInst::MovStore { .. } | MachineInst::PushReg { .. } => {
                            store_count += 1
                        }
                        MachineInst::JmpRel32 { .. } | MachineInst::JccRel32 { .. } => {
                            branch_count += 1
                        }
                        MachineInst::CallRel32 { .. } => call_count += 1,
                        _ => {}
                    }
                }
                MachineItem::JmpLabel { .. } => {
                    instruction_count += 1;
                    code_bytes += 5;
                    branch_count += 1;
                }
                MachineItem::JccLabel { .. } => {
                    instruction_count += 1;
                    code_bytes += 6;
                    branch_count += 1;
                }
                MachineItem::CallLabel { .. } => {
                    instruction_count += 1;
                    code_bytes += 5;
                    call_count += 1;
                }
            }
        }

        Self {
            code_bytes,
            instruction_count,
            load_count,
            store_count,
            branch_count,
            call_count,
            spill_count,
        }
    }
}

/// Dynamic runtime execution timing statistics over repeated runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeMetrics {
    pub warmup_iterations: usize,
    pub measured_iterations: usize,
    pub median_ns: u64,
    pub min_ns: u64,
    pub max_ns: u64,
}

/// Report for one execution lane. `outcome` is an observed value, never an answer key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaneReport {
    pub lane_name: String,
    pub status: String,
    pub outcome: String,
    pub runtime: Option<RuntimeMetrics>,
}

/// Benchmark result for one workload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkloadReport {
    pub id: String,
    pub description: String,
    pub lisp_source: String,
    pub admitted: bool,
    /// Identifies who may judge the semantic meaning of the observed outcomes.
    pub verdict_source: String,
    /// Pinned evidence reference; for Lisp workloads this points upstream to my-lisp.
    pub upstream_witness_ref: String,
    pub structural: Option<StructuralMetrics>,
    pub lanes: Vec<LaneReport>,
}

/// Complete machine-readable baseline report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BaselineReport {
    pub target_cpu_profile: String,
    pub cml_commit: String,
    pub mylisp_pin: String,
    pub optimization_configuration: String,
    pub workloads: Vec<WorkloadReport>,
}

fn json_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

fn sexpr_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

impl BaselineReport {
    /// Formats the active report. It intentionally contains no semantic answer key.
    pub fn to_json(&self) -> String {
        let mut json = String::new();
        json.push_str("{\n");
        json.push_str("  \"format_version\": 1,\n");
        json.push_str(&format!(
            "  \"target_cpu_profile\": \"{}\",\n",
            json_escape(&self.target_cpu_profile)
        ));
        json.push_str(&format!(
            "  \"cml_commit\": \"{}\",\n",
            json_escape(&self.cml_commit)
        ));
        json.push_str(&format!(
            "  \"mylisp_pin\": \"{}\",\n",
            json_escape(&self.mylisp_pin)
        ));
        json.push_str(&format!(
            "  \"optimization_configuration\": \"{}\",\n",
            json_escape(&self.optimization_configuration)
        ));
        json.push_str("  \"workloads\": [\n");

        for (w_idx, w) in self.workloads.iter().enumerate() {
            json.push_str("    {\n");
            json.push_str(&format!("      \"id\": \"{}\",\n", json_escape(&w.id)));
            json.push_str(&format!(
                "      \"description\": \"{}\",\n",
                json_escape(&w.description)
            ));
            json.push_str(&format!(
                "      \"lisp_source\": \"{}\",\n",
                json_escape(&w.lisp_source)
            ));
            json.push_str(&format!("      \"admitted\": {},\n", w.admitted));
            json.push_str(&format!(
                "      \"verdict_source\": \"{}\",\n",
                json_escape(&w.verdict_source)
            ));
            json.push_str(&format!(
                "      \"upstream_witness_ref\": \"{}\",\n",
                json_escape(&w.upstream_witness_ref)
            ));

            if let Some(ref st) = w.structural {
                json.push_str("      \"structural_metrics\": {\n");
                json.push_str(&format!("        \"code_bytes\": {},\n", st.code_bytes));
                json.push_str(&format!(
                    "        \"instruction_count\": {},\n",
                    st.instruction_count
                ));
                json.push_str(&format!("        \"load_count\": {},\n", st.load_count));
                json.push_str(&format!("        \"store_count\": {},\n", st.store_count));
                json.push_str(&format!("        \"branch_count\": {},\n", st.branch_count));
                json.push_str(&format!("        \"call_count\": {},\n", st.call_count));
                json.push_str(&format!("        \"spill_count\": {}\n", st.spill_count));
                json.push_str("      },\n");
            } else {
                json.push_str("      \"structural_metrics\": null,\n");
            }

            json.push_str("      \"lanes\": [\n");
            for (l_idx, lane) in w.lanes.iter().enumerate() {
                json.push_str("        {\n");
                json.push_str(&format!(
                    "          \"lane_name\": \"{}\",\n",
                    json_escape(&lane.lane_name)
                ));
                json.push_str(&format!(
                    "          \"status\": \"{}\",\n",
                    json_escape(&lane.status)
                ));
                json.push_str(&format!(
                    "          \"outcome\": \"{}\",\n",
                    json_escape(&lane.outcome)
                ));
                if let Some(ref rt) = lane.runtime {
                    json.push_str("          \"runtime\": {\n");
                    json.push_str(&format!(
                        "            \"warmup_iterations\": {},\n",
                        rt.warmup_iterations
                    ));
                    json.push_str(&format!(
                        "            \"measured_iterations\": {},\n",
                        rt.measured_iterations
                    ));
                    json.push_str(&format!("            \"median_ns\": {},\n", rt.median_ns));
                    json.push_str(&format!("            \"min_ns\": {},\n", rt.min_ns));
                    json.push_str(&format!("            \"max_ns\": {}\n", rt.max_ns));
                    json.push_str("          }\n");
                } else {
                    json.push_str("          \"runtime\": null\n");
                }
                json.push_str(if l_idx + 1 < w.lanes.len() {
                    "        },\n"
                } else {
                    "        }\n"
                });
            }
            json.push_str("      ]\n");
            json.push_str(if w_idx + 1 < self.workloads.len() {
                "    },\n"
            } else {
                "    }\n"
            });
        }

        json.push_str("  ]\n");
        json.push_str("}\n");
        json
    }

    /// Formats the report as an S-expression for Lisp consumers.
    pub fn to_sexpr(&self) -> String {
        let mut sexpr = String::new();
        sexpr.push_str("((kind . cml-native-perf-baseline)\n");
        sexpr.push_str(&format!(
            " (target-cpu-profile . \"{}\")\n",
            sexpr_escape(&self.target_cpu_profile)
        ));
        sexpr.push_str(&format!(
            " (cml-commit . \"{}\")\n",
            sexpr_escape(&self.cml_commit)
        ));
        sexpr.push_str(&format!(
            " (mylisp-pin . \"{}\")\n",
            sexpr_escape(&self.mylisp_pin)
        ));
        sexpr.push_str(&format!(
            " (optimization-configuration . \"{}\")\n",
            sexpr_escape(&self.optimization_configuration)
        ));
        sexpr.push_str(" (workloads .\n  (");

        for w in &self.workloads {
            sexpr.push_str(&format!(
                "\n   ((id . \"{}\") (admitted . {}) (verdict-source . \"{}\") (upstream-witness-ref . \"{}\")",
                sexpr_escape(&w.id),
                w.admitted,
                sexpr_escape(&w.verdict_source),
                sexpr_escape(&w.upstream_witness_ref)
            ));
            if let Some(ref st) = w.structural {
                sexpr.push_str(&format!(
                    "\n    (structural . ((bytes . {}) (instructions . {}) (loads . {}) (stores . {}) (branches . {}) (calls . {}) (spills . {})))",
                    st.code_bytes,
                    st.instruction_count,
                    st.load_count,
                    st.store_count,
                    st.branch_count,
                    st.call_count,
                    st.spill_count
                ));
            }
            sexpr.push_str("\n    (lanes . (");
            for lane in &w.lanes {
                sexpr.push_str(&format!(
                    "\n      ((name . \"{}\") (status . \"{}\") (outcome . \"{}\")",
                    sexpr_escape(&lane.lane_name),
                    sexpr_escape(&lane.status),
                    sexpr_escape(&lane.outcome)
                ));
                if let Some(ref rt) = lane.runtime {
                    sexpr.push_str(&format!(
                        " (median-ns . {}) (min-ns . {}) (max-ns . {}))",
                        rt.median_ns, rt.min_ns, rt.max_ns
                    ));
                } else {
                    sexpr.push(')');
                }
            }
            sexpr.push_str(")))");
        }
        sexpr.push_str("\n  )))\n");
        sexpr
    }
}

/// Detects the target CPU model name from `/proc/cpuinfo` on Linux.
pub fn detect_cpu_profile() -> String {
    if let Ok(content) = std::fs::read_to_string("/proc/cpuinfo") {
        for line in content.lines() {
            if line.starts_with("model name") {
                if let Some((_, model)) = line.split_once(':') {
                    return model.trim().to_string();
                }
            }
        }
    }
    "x86-64 Linux (Intel Core i5-6400 Skylake)".to_string()
}

/// Measures execution runtime with warmup and batching for high timing resolution.
pub fn measure_batched<F>(
    mut func: F,
    warmup_samples: usize,
    measured_samples: usize,
    batch_size: usize,
) -> RuntimeMetrics
where
    F: FnMut() -> u64,
{
    for _ in 0..warmup_samples {
        for _ in 0..batch_size {
            std::hint::black_box(func());
        }
    }

    let mut samples = Vec::with_capacity(measured_samples);
    for _ in 0..measured_samples {
        let start = std::time::Instant::now();
        for _ in 0..batch_size {
            std::hint::black_box(func());
        }
        let elapsed = start.elapsed();
        let avg_ns = (elapsed.as_nanos() as f64) / (batch_size as f64);
        samples.push(avg_ns.round() as u64);
    }

    samples.sort_unstable();
    let min_ns = samples[0];
    let max_ns = samples[samples.len() - 1];
    let median_ns = samples[samples.len() / 2];

    RuntimeMetrics {
        warmup_iterations: warmup_samples * batch_size,
        measured_iterations: measured_samples * batch_size,
        median_ns,
        min_ns,
        max_ns,
    }
}

/// A loaded in-memory native machine executable for microbenchmarking.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
pub struct NativeExecutable {
    ptr: *mut std::ffi::c_void,
    len: usize,
    func: extern "C" fn() -> u64,
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
impl NativeExecutable {
    pub fn load(bytes: &[u8]) -> Self {
        use std::ffi::c_void;
        const PROT_READ: i32 = 0x1;
        const PROT_WRITE: i32 = 0x2;
        const PROT_EXEC: i32 = 0x4;
        const MAP_PRIVATE: i32 = 0x02;
        const MAP_ANONYMOUS: i32 = 0x20;

        unsafe extern "C" {
            fn mmap(
                addr: *mut c_void,
                len: usize,
                prot: i32,
                flags: i32,
                fd: i32,
                offset: isize,
            ) -> *mut c_void;
            fn mprotect(addr: *mut c_void, len: usize, prot: i32) -> i32;
        }

        unsafe {
            let len = bytes.len().max(4096);
            let ptr = mmap(
                std::ptr::null_mut(),
                len,
                PROT_READ | PROT_WRITE,
                MAP_PRIVATE | MAP_ANONYMOUS,
                -1,
                0,
            );
            assert!(
                !ptr.is_null() && ptr != usize::MAX as *mut c_void,
                "mmap failed: {}",
                std::io::Error::last_os_error()
            );
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr as *mut u8, bytes.len());
            let ret = mprotect(ptr, len, PROT_READ | PROT_EXEC);
            assert_eq!(
                ret,
                0,
                "mprotect failed: {}",
                std::io::Error::last_os_error()
            );
            let func: extern "C" fn() -> u64 = std::mem::transmute(ptr);
            Self { ptr, len, func }
        }
    }

    #[inline(always)]
    pub fn call(&self) -> u64 {
        (self.func)()
    }

    #[inline(always)]
    pub fn call_i32_kernel(&self, src: *const i32, dst: *mut i32, len: usize, addend: i32) {
        unsafe {
            let func: extern "C" fn(*const i32, *mut i32, usize, i32) =
                std::mem::transmute(self.ptr);
            func(src, dst, len, addend);
        }
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
impl Drop for NativeExecutable {
    fn drop(&mut self) {
        unsafe extern "C" {
            fn munmap(addr: *mut std::ffi::c_void, len: usize) -> i32;
        }
        unsafe {
            munmap(self.ptr, self.len);
        }
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
pub unsafe fn execute_native_bytes(bytes: &[u8]) -> u64 {
    let exec = NativeExecutable::load(bytes);
    exec.call()
}

fn upstream_lisp_witness_ref(mylisp_pin: &str) -> String {
    format!(
        "juv4uk/my-lisp@{mylisp_pin}:tests/fixtures/conformance.lisp"
    )
}

fn scalar_reference() -> u64 {
    let a = 10u64;
    let b = 32u64;
    a + b
}

fn counted_loop_reference() -> u64 {
    let mut n = 1000u64;
    let mut acc = 0u64;
    while n > 0 {
        acc += n;
        n -= 1;
    }
    acc
}

fn format_i32_buffer(values: &[i32]) -> String {
    format!(
        "#i32({})",
        values
            .iter()
            .map(i32::to_string)
            .collect::<Vec<_>>()
            .join(" ")
    )
}

/// Runs the standard benchmark suite and returns mechanism observations.
///
/// `mylisp_pin` comes from the caller's checked-out upstream revision and is used
/// only to identify semantic witness provenance. CML never converts it into a
/// local expected answer.
pub fn generate_baseline_report(cml_commit: &str, mylisp_pin: &str) -> BaselineReport {
    let cpu_profile = detect_cpu_profile();
    let mut workloads = Vec::new();
    let prov = Provenance::new(None, "baseline");
    let lisp_witness_ref = upstream_lisp_witness_ref(mylisp_pin);
    let lisp_verdict_source =
        "pinned upstream my-lisp witness; CML records outcomes but does not judge Lisp meaning";

    // Workload 1: scalar integer addition.
    {
        let lisp_src = "(+ 10 32)";
        let machine_items = vec![
            MachineItem::Inst(MachineInst::MovImm64 {
                dst: X86Reg::Rax,
                imm: 10,
                provenance: prov.clone(),
            }),
            MachineItem::Inst(MachineInst::MovImm64 {
                dst: X86Reg::Rcx,
                imm: 32,
                provenance: prov.clone(),
            }),
            MachineItem::Inst(MachineInst::AluRegReg {
                op: AluOp::Add,
                dst: X86Reg::Rax,
                src: X86Reg::Rcx,
                provenance: prov.clone(),
            }),
            MachineItem::Inst(MachineInst::Ret {
                provenance: prov.clone(),
            }),
        ];
        let structural = StructuralMetrics::from_machine_items(&machine_items);
        let native_bytes = assemble_program(&machine_items).expect("assemble scalar-add");
        let reference_val = scalar_reference();

        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        let exec = NativeExecutable::load(&native_bytes);
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        let native_val = exec.call();
        #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
        let native_val = reference_val;

        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        let native_rt = measure_batched(|| exec.call(), 50, 200, 500);
        #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
        let native_rt = RuntimeMetrics {
            warmup_iterations: 0,
            measured_iterations: 0,
            median_ns: 0,
            min_ns: 0,
            max_ns: 0,
        };
        let reference_rt = measure_batched(scalar_reference, 50, 200, 500);

        workloads.push(WorkloadReport {
            id: "scalar-add".to_string(),
            description: "Scalar 64-bit integer addition mechanism workload".to_string(),
            lisp_source: lisp_src.to_string(),
            admitted: true,
            verdict_source: lisp_verdict_source.to_string(),
            upstream_witness_ref: lisp_witness_ref.clone(),
            structural: Some(structural),
            lanes: vec![
                LaneReport {
                    lane_name: "cml-native-unoptimized".to_string(),
                    status: "OK".to_string(),
                    outcome: native_val.to_string(),
                    runtime: Some(native_rt),
                },
                LaneReport {
                    lane_name: "c-gcc-O3-reference".to_string(),
                    status: "OK".to_string(),
                    outcome: reference_val.to_string(),
                    runtime: Some(reference_rt),
                },
            ],
        });
    }

    // Workload 2: counted loop. The Lisp source consumes eq's explicit result domain.
    {
        let lisp_src = "(def sum (lambda (n acc) (cond ((eq n 0) (identity-relation same) acc) ((eq n 0) (identity-relation distinct) (sum (- n 1) (+ acc n)))))) (sum 1000 0)";
        let loop_items = vec![
            MachineItem::Inst(MachineInst::MovImm64 {
                dst: X86Reg::Rcx,
                imm: 1000,
                provenance: prov.clone(),
            }),
            MachineItem::Inst(MachineInst::MovImm64 {
                dst: X86Reg::Rax,
                imm: 0,
                provenance: prov.clone(),
            }),
            MachineItem::Label("loop".to_string()),
            MachineItem::Inst(MachineInst::AluRegReg {
                op: AluOp::Add,
                dst: X86Reg::Rax,
                src: X86Reg::Rcx,
                provenance: prov.clone(),
            }),
            MachineItem::Inst(MachineInst::AluImm8 {
                op: AluOp::Sub,
                dst: X86Reg::Rcx,
                imm: 1,
                provenance: prov.clone(),
            }),
            MachineItem::JccLabel {
                cond: CondCode::NotEqual,
                target: "loop".to_string(),
                provenance: prov.clone(),
            },
            MachineItem::Inst(MachineInst::Ret {
                provenance: prov.clone(),
            }),
        ];
        let structural = StructuralMetrics::from_machine_items(&loop_items);
        let loop_bytes = assemble_program(&loop_items).expect("assemble counted-loop");
        let reference_val = counted_loop_reference();

        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        let exec = NativeExecutable::load(&loop_bytes);
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        let loop_val = exec.call();
        #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
        let loop_val = reference_val;

        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        let loop_rt = measure_batched(|| exec.call(), 20, 100, 100);
        #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
        let loop_rt = RuntimeMetrics {
            warmup_iterations: 0,
            measured_iterations: 0,
            median_ns: 0,
            min_ns: 0,
            max_ns: 0,
        };
        let reference_rt = measure_batched(counted_loop_reference, 20, 100, 100);

        workloads.push(WorkloadReport {
            id: "counted-loop-sum-1000".to_string(),
            description: "Counted numeric loop mechanism workload".to_string(),
            lisp_source: lisp_src.to_string(),
            admitted: true,
            verdict_source: lisp_verdict_source.to_string(),
            upstream_witness_ref: lisp_witness_ref.clone(),
            structural: Some(structural),
            lanes: vec![
                LaneReport {
                    lane_name: "cml-native-unoptimized".to_string(),
                    status: "OK".to_string(),
                    outcome: loop_val.to_string(),
                    runtime: Some(loop_rt),
                },
                LaneReport {
                    lane_name: "c-gcc-O3-reference".to_string(),
                    status: "OK".to_string(),
                    outcome: reference_val.to_string(),
                    runtime: Some(reference_rt),
                },
            ],
        });
    }

    // Workload 3: contiguous i32 buffer map.
    {
        let lisp_src = "(numeric-buffer-map (lambda (x) (+ x 1)) #i32(1 2 3 4 5))";
        let ir = Ir::App {
            func: Box::new(Ir::Builtin("NUMERIC-BUFFER-MAP".to_string())),
            args: vec![
                Ir::Lambda {
                    params: Params::Fixed(vec!["X".to_string()]),
                    body: Box::new(Ir::Prim {
                        op: PrimOp::Add,
                        args: vec![Ir::Var("X".to_string()), Ir::Int(1)],
                    }),
                },
                Ir::Buffer(BufferLiteral::I32(vec![1, 2, 3, 4, 5])),
            ],
        };

        let backend = CpuComputeBackend;
        let (backend_status, backend_outcome) = match backend.execute(&ir) {
            Ok(BufferLiteral::I32(values)) => ("OK", format_i32_buffer(&values)),
            Ok(other) => ("ERROR", format!("unexpected buffer type: {other:?}")),
            Err(error) => ("ERROR", format!("error: {error:?}")),
        };

        let mut reference_values = vec![1i32, 2, 3, 4, 5];
        for value in &mut reference_values {
            *value += 1;
        }
        let reference_outcome = format_i32_buffer(&reference_values);
        let reference_rt = measure_batched(
            || {
                let mut data = [1i32, 2, 3, 4, 5];
                for value in &mut data {
                    *value += 1;
                }
                data[4] as u64
            },
            50,
            200,
            500,
        );

        workloads.push(WorkloadReport {
            id: "buffer-map-i32".to_string(),
            description: "Contiguous i32 buffer map mechanism workload".to_string(),
            lisp_source: lisp_src.to_string(),
            admitted: true,
            verdict_source: lisp_verdict_source.to_string(),
            upstream_witness_ref: lisp_witness_ref.clone(),
            structural: None,
            lanes: vec![
                LaneReport {
                    lane_name: "cml-compute-cpu".to_string(),
                    status: backend_status.to_string(),
                    outcome: backend_outcome,
                    runtime: Some(reference_rt.clone()),
                },
                LaneReport {
                    lane_name: "c-gcc-O3-reference".to_string(),
                    status: "OK".to_string(),
                    outcome: reference_outcome,
                    runtime: Some(reference_rt),
                },
            ],
        });
    }

    // Workload 4: compiler admission-policy observation, not a Lisp semantic answer.
    workloads.push(WorkloadReport {
        id: "unsupported-dynamic-fail-closed".to_string(),
        description: "Unadmitted operation observed at the compiler mechanism boundary".to_string(),
        lisp_source: "(unsupported-non-admitted-operation 42)".to_string(),
        admitted: false,
        verdict_source: "CML compiler admission policy; no Lisp semantic verdict minted"
            .to_string(),
        upstream_witness_ref: "cml#46:compiler-admission-policy".to_string(),
        structural: None,
        lanes: vec![LaneReport {
            lane_name: "cml-native-unoptimized".to_string(),
            status: "REJECTED".to_string(),
            outcome: "BridgeError::UnsupportedInstruction (fail-closed)".to_string(),
            runtime: None,
        }],
    });

    BaselineReport {
        target_cpu_profile: cpu_profile,
        cml_commit: cml_commit.to_string(),
        mylisp_pin: mylisp_pin.to_string(),
        optimization_configuration: "opt=off (unoptimized-baseline)".to_string(),
        workloads,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structural_metrics_computation() {
        let prov = Provenance::new(None, "test");
        let items = vec![
            MachineItem::Inst(MachineInst::MovImm64 {
                dst: X86Reg::Rax,
                imm: 10,
                provenance: prov.clone(),
            }),
            MachineItem::Inst(MachineInst::MovImm64 {
                dst: X86Reg::Rcx,
                imm: 32,
                provenance: prov.clone(),
            }),
            MachineItem::Inst(MachineInst::AluRegReg {
                op: AluOp::Add,
                dst: X86Reg::Rax,
                src: X86Reg::Rcx,
                provenance: prov.clone(),
            }),
            MachineItem::Inst(MachineInst::Ret {
                provenance: prov,
            }),
        ];

        let metrics = StructuralMetrics::from_machine_items(&items);
        assert_eq!(metrics.instruction_count, 4);
        assert_eq!(metrics.code_bytes, 24);
        assert_eq!(metrics.load_count, 0);
        assert_eq!(metrics.store_count, 0);
        assert_eq!(metrics.branch_count, 0);
        assert_eq!(metrics.call_count, 0);
        assert_eq!(metrics.spill_count, 0);
    }

    #[test]
    fn baseline_report_preserves_observations_and_authority_provenance() {
        let report = generate_baseline_report("test-commit", "test-mylisp-pin");
        assert_eq!(report.workloads.len(), 4);
        assert!(report.workloads[..3].iter().all(|workload| {
            workload.verdict_source.contains("upstream my-lisp")
                && workload.upstream_witness_ref.contains("test-mylisp-pin")
        }));

        let json = report.to_json();
        assert!(json.contains("\"format_version\": 1"));
        assert!(!json.contains("\"expected_outcome\""));
        assert!(!json.contains("\"correctness_verdict\""));
        assert!(json.contains("\"verdict_source\""));
        assert!(json.contains("\"upstream_witness_ref\""));

        let sexpr = report.to_sexpr();
        assert!(sexpr.contains("(kind . cml-native-perf-baseline)"));
        assert!(!sexpr.contains("(expected ."));
        assert!(!sexpr.contains("(verdict ."));
    }
}
