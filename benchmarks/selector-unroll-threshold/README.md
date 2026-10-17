# cml#398 — selector unroll threshold benchmark

Research-only mechanism benchmark. It does **not** add SENS semantics or a production compiler lowering.

It consumes the already-proven selector rule (`0 -> CAR`, `1 -> CDR`) and compares two target mechanisms for a static path:

1. **unrolled** — one straight-line pointer dereference per suffix bit;
2. **loop** — one compact loop that reads suffix bits at runtime.

Correctness is checked before measurement. Cachegrind `I refs` are measured with cache and branch simulation disabled, matching the reproducible metric used by the SENS benchmark protocol.

The key trade-off is deliberately two-dimensional:

- unrolling may reduce runtime path/decode work to zero;
- the emitted code size grows with selector depth;
- the loop keeps code size nearly constant but retains runtime path work.

Run:

```sh
python3 benchmarks/selector-unroll-threshold/run.py \
  --out-dir benchmarks/selector-unroll-threshold/results/current
```

Outputs:

- `raw.tsv` — rows compatible with the SENS #1987 schema where applicable;
- `summary.tsv` — net I-refs/call after subtracting the common base loop;
- `provenance.json` — host/compiler/Valgrind provenance.

This is a mechanism bound, not yet the result of CML's real x86 backend. cml#397 remains responsible for actual compiler-path lowering evidence.