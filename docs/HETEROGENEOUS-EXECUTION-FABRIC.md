# CPU + GPU + FPGA execution fabric

**Status:** architecture direction, 2026-08-24. Machine-readable contracts
remain authoritative.

CML is the integration point of one semantic system, not a second Lisp:

```text
my-lisp semantics -> CML semantic IR -> Execution Graph
                                          |    |    |
                                         CPU  GPU  FPGA
```

The CPU owns full semantics and fallback. GPU executors own pure bulk work on
immutable typed numeric buffers. The portable direction is Rust + `wgpu`;
CUDA is an optional NVIDIA optimization, while future Intel Level Zero/oneAPI
and AMD ROCm implementations enter through the same capability contract. The
FPGA executes either general Lisp through `fpga-lisp` ISA or, later, proven
stream/dataflow regions through a separate specialization path.

## Execution Graph boundary

CML should represent explicit nodes, dependencies, logical buffers, and
targets:

```rust
enum ExecutionTarget {
    Cpu,
    Gpu { backend: GpuBackend },
    Fpga { device: String },
}

struct PlanNode {
    id: NodeId,
    operation: Operation,
    inputs: Vec<BufferId>,
    outputs: Vec<BufferId>,
    dependencies: Vec<NodeId>,
    target: ExecutionTarget,
}
```

Raw host/device pointers never cross this boundary. M0 moves values through
host-visible immutable buffers; zero-copy and direct transfers are later,
semantics-preserving optimizations.

Every backend reports capabilities and implements prepare/execute. Only
`Live` capabilities are selectable. `Planned`, unknown representation facts,
unsupported effects, or unsatisfied numeric obligations reject acceleration.
Outputs become visible atomically; failures expose no partial language value.

Planner order is:

```text
semantic safety
-> live capability
-> supported representation and operation
-> transfer/launch cost and workload size
-> selected target
```

CPU is always the canonical fallback. Initial placement is explicit and
deterministic; automatic placement waits for differential correctness and
measurements.

## Milestones

1. **M0:** graph plus CPU executor; multi-node dependency, buffer, and failure
   tests; GPU/FPGA targets fail closed without registered live executors.
2. **M1:** lower `numeric-buffer-map`; compare CPU with one live GPU executor;
   retain CUDA and portable `wgpu` behind the same interface.
3. **M2:** versioned FPGA job/result frames and Rust transport; execute a graph
   node on the physical board while retaining the monitor as an independent
   diagnostic tool.
4. **M3:** golden CPU -> GPU -> CPU -> FPGA -> CPU pipeline, checked against
   reference execution. This proves orchestration, not speedup.
5. **M4:** measured, explainable, cost-aware placement and safe fallback.

Conformance records `CONFIRMED`, `PARTIAL`, `UNSUPPORTED`, `UNAVAILABLE`,
`BROKEN`, or `UNRESOLVED` per backend. `i32` values and overflow behavior must
match; exact values are never silently converted to `f32`; floating comparison
rules belong in a named contract.

The first code slice is M0 in CML. It gives the existing CPU and CUDA paths,
the portable GPU direction, and the connected FPGA one stable seam without
putting hardware names into my-lisp semantics.
