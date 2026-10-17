# #402 — first packed selector suffix result

Host: Intel Core i5-6400 @ 2.70 GHz, WSL2. GCC 16.1.0, Valgrind 3.27.0.
Research-only mechanism benchmark; not production CML lowering.

| depth | unroll I/call | byte-loop I/call | packed-loop I/call | unroll bytes | byte bytes | packed bytes |
|---:|---:|---:|---:|---:|---:|---:|
| 8 | 34 | 118 | 190 | 29 | 60 | 90 |
| 16 | 42 | 174 | 286 | 57 | 68 | 91 |
| 32 | 58 | 286 | 478 | 113 | 84 | 93 |
| 64 | 90 | 510 | 862 | 225 | 116 | 97 |
| 128 | 154 | 958 | 1630 | 449 | 180 | 105 |

Static bytes include loop code plus suffix descriptor. All candidates perform the same number of primitive CAR/CDR steps.

## Bounded observations

- unrolling is fastest at every measured depth;
- byte-per-bit loop is faster than packed loop at every measured depth;
- packed loop becomes the smallest static representation only on sufficiently long paths (visible by depth 64);
- packing does not remove semantic work; it trades descriptor bytes for shift/mask decode work;
- there is no universal winner: runtime speed and static footprint are separate Pareto axes.

## Compiler implication

Use packing only where code/data footprint matters enough to pay the decode cost. For static hot paths, direct lowering/unrolling remains the primary candidate. For long cold paths, compact loops remain worth measuring.

Raw evidence: `summary.tsv` and `provenance.json` in this directory.