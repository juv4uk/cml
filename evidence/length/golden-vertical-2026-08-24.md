# Fresh four-leg golden vertical — 2026-08-24, wsl-ganaka-1

Canonical program: length '(a b c) => 3 (length-onto tail-recursion,
lib/core.my). All four legs re-run on CURRENT heads:

| Leg | Implementation | Head | Result |
|---|---|---|---|
| 1 | Rust interpreter (TCP oracle :9999) | 5a662c0 | 3 |
| 2 | cml compile (aad1d61) -> interned .asm (218 instructions) | aad1d61 | assembled ok |
| 3 | fpga-lisp RTL @ iverilog (+max_time=200000000) | 0351a6d | RESULT_TAG:0 RESULT_VAL:3 |
| 4 | Racket port (#lang my-lisp, pkg --link) | racket/boot + lib/core.my shared | 3 |

Bridge note: cml emits LOADSYM Rn <symbol-name>; fpga-lisp assembler.py
expects numeric TAG_SYMBOL immediates. Interim bridge:
/tmp/opencode/asm_intern.py (per-program interning from id 900; map in
golden.sym). Cross-repo decision pending (cml intern-at-emission vs
assembler-side table) - tracked in this report.

## Provenance and scope

- `cml@aad1d61` is an ancestor of the current CML head; this evidence pins
  the exact compiler revision used for the 218-instruction artifact.
- FPGA output artifact `golden.bin` SHA-256:
  `f5696777c93a58c7f52a0642fde227691e9a2888621882db8efa39d9a451e5d1`.
- Interned assembly `golden.i.asm` SHA-256:
  `0cdb44a329e488860103345d5d872c6de902b0ba06e9096564d2e5afd59bcce4`.
- Simulator transcript `sim.out` SHA-256:
  `424e3625b3bacdaf28379152f3ad9a6a9219ceb9a9a09354422805375acdff7d`.
- The Racket leg was observed from the external `euclid-wt` working tree
  (`#lang my-lisp`, linked package, shared `lib/core.my`); no repository SHA
  is claimed for that leg. It is an additional observed result, not a
  replacement for canonical my-lisp or CML/FPGA conformance evidence.
- This is one four-leg `length` observation, not blanket language,
  compiler, or hardware conformance.
