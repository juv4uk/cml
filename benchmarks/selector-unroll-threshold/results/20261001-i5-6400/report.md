# cml#398 — first selector unroll-threshold result

Date: 2026-10-01
Host: Intel Core i5-6400 @ 2.70 GHz, WSL2
Toolchain: GCC 16.1.0, Valgrind 3.27.0
Method: Cachegrind I refs, cache/branch simulation disabled, 50,000 calls/row, median of 3, common base loop subtracted.

Research-only mechanism result. This is not yet CML production x86 lowering and does not define SENS semantics.

## Result

| depth | unrolled I-refs/call | loop I-refs/call | unrolled static bytes | loop static bytes |
|---:|---:|---:|---:|---:|
| 0 | 27 | 60 | 4 | 52 |
| 1 | 27 | 69 | 5 | 53 |
| 2 | 28 | 76 | 8 | 54 |
| 4 | 30 | 90 | 15 | 56 |
| 8 | 34 | 118 | 29 | 60 |
| 16 | 42 | 174 | 57 | 68 |
| 32 | 58 | 286 | 113 | 84 |
| 64 | 90 | 510 | 225 | 116 |

`static bytes` for the loop are its 52-byte loop body plus the current byte-per-bit path descriptor. The unrolled path has no runtime path descriptor.

## Bounded observations

1. In this mechanism harness, unrolling wins decisively in execution I-refs at every measured depth.
2. With the current byte-per-bit descriptor, unrolling also has the smaller static footprint through depth 16.
3. At depth 32 and 64 there is a real Pareto trade-off: unrolling remains much cheaper to execute but becomes larger in static code.
4. Both candidates still perform the same `depth` primitive CAR/CDR steps. The unrolled candidate removes only runtime path decoding; it does not make the semantic work disappear.
5. The loop representation is intentionally not declared optimal. A packed suffix could reduce descriptor bytes, but would add bit extraction work. That is a separate candidate to measure, not an assumption.

## Compiler implication

A useful evidence-driven policy to test in real CML lowering is:

```text
static short path  -> direct unroll
static long path   -> choose unroll vs compact loop by code-size/runtime Pareto
dynamic path       -> direct bit-walk first; do not default to HashMap/cache
```

The current harness suggests the first code-size crossover is between depths 16 and 32 for this implementation, but this is **not** a production threshold. cml#397 must measure actual CML-generated code and compile-time cost.

## Limitations

- standalone C mechanism model, not current CML x86 backend;
- cache and branch simulation disabled; only I refs are primary here;
- loop suffix is one byte per bit, not packed;
- one deterministic mixed selector path per depth;
- no compile-time I-ref measurement in this child;
- no claim about non-selector families.

Raw evidence: `raw.tsv`, `summary.tsv`, `provenance.json` in this directory.