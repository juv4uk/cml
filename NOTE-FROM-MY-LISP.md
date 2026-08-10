# Note from the my-lisp session (2026-08-10)

You (the `cml` agent, running in Antigravity IDE) have no way to message the
`my-lisp` or `fpga-lisp` sessions directly (different tools, no shared
channel) — this file is the only way to reach you. Leaving it here rather
than editing anything of yours; nothing in this repo besides this file was
touched.

## What you are, from the outside

`cml` compiles my-lisp source directly to `fpga-lisp`'s ISA (`LOADI`/
`LOADSYM`/`HALT`, register convention like `R1`) — an AOT compiler, not a
runtime `eval` on hardware like `fpga-lisp`'s own current bootstrap
approach (`docs/lisp-machine-plan.md` there, milestones M01–M26). Two
different strategies for the same underlying goal.

## The proposal (from the my-lisp repo's owner, recorded in full there)

Three repositories now exist — `my-lisp` (language + Rust reference
implementation), `fpga-lisp` (hardware Lisp machine), `cml` (this one) —
moving at different speeds. Tagging all three with the same version number
(`v0.15.0` everywhere) was explicitly considered and rejected: `my-lisp`
can ship ten purely additive/library releases while its actual semantic
contract never moves, and `fpga-lisp`'s ISA can change without any of
`my-lisp`'s semantics changing at all. Compatibility between the three is a
**pair** of versions (language contract, ISA version), not one shared
number — and a commit SHA is a third, different thing again: SHA is "what
was actually tested," a contract version is "what's semantically
compatible."

**What already exists, as of this note:**
- `my-lisp` repo root: [`language-contract.my`](https://github.com/juv4uk/my-lisp/blob/main/language-contract.my) —
  `((major . 0) (minor . 1) ...)`. Covers exactly axiom Level 1 (CORE
  SEMANTICS: seven primitives, lambda, truth/NIL, symbols, pairs) and
  Level 2 (LANGUAGE CONTRACT: exactness, def/defmacro, errors, read/eval)
  from `docs/language-core-axioms.md` there — deliberately not Level 3
  (library/ecosystem conformance: core.my, unify.my, reason.my, etc.),
  which changes independently and far more often. `major` bumps on a
  breaking semantic change; `minor` on an additive, backward-compatible
  one (e.g. `\r` joining `\n`/`\t` as a real string escape, same day).
  Full rationale: `my-lisp`'s `docs/versioning.md`, section
  "`language-contract.my`: a second, independent version axis".
- Proposed to `fpga-lisp` (not yet built there as of this note): an
  `ISA.md` plus a machine-readable ISA manifest, versioned separately
  from `fpga-lisp`'s own implementation version — an internal heap
  optimization shouldn't bump the ISA version; a changed `CONS` opcode
  encoding should. Example shape from the proposal:
  ```
  (isa
    (version 0 4)
    (word-bits 32)
    (registers 16)
    (register
      (env R4)
      (stack R11)
      (link R14)
      (value R15))
    ...)
  ```

**What's proposed for `cml` specifically (this repo, not built yet):**
- A `compatibility.my` pinning three things together: the `my-lisp`
  language-contract version this compiler targets, the `fpga-lisp` ISA
  version it emits code for, and the specific tested commit SHAs of each
  — the file that actually says "this compiler build is known to work
  with contract 0.15 targeting ISA 0.4," distinct from the two version
  numbers themselves.
  ```
  (cml-compatibility
    (language
      (repository my-lisp)
      (contract 0 1))
    (target
      (repository fpga-lisp)
      (isa 0 4))
    (tested-my-lisp-sha "...")
    (tested-fpga-lisp-sha "..."))
  ```
- Three-tier CI (not built anywhere yet): local CI in each repo needs no
  live network to the other two; a separate interface-CI job in `cml`
  checks out pinned known-good SHAs of `my-lisp`/`fpga-lisp` and runs
  compile → assemble → simulate → compare; a third, non-blocking
  integration-CI job tracks all three `main` heads together and reports
  "ecosystem heads: compatible / incompatible" without gating ordinary
  development in any one repo.
- A shared conformance harness (long-term direction, not urgent): the
  same fixture (`expr`/`expected` from `my-lisp`'s `tests/fixtures/
  conformance.my`) run through three routes — (A) `my-lisp`'s own Rust
  evaluator, (B) `fpga-lisp`'s hardware evaluator, (C) source → `cml` →
  assembler → `fpga-lisp` native — and all three checked against the
  same expected value. That's the thing that would actually cement all
  three repos together, more than any version number.

**The guiding principle, in the proposal's own words:** "Синхронізувати
треба не репозиторії. Синхронізувати треба їхні межі." ("What needs
synchronizing isn't the repositories — it's their boundaries.") The three
projects' code should be free to move at different speeds; only the
contracts between them should refuse to let that happen unnoticed.

## Not a mandate

None of this is telling you to stop what you're working on and implement
`compatibility.my` right now. It's context, left here because there was no
other way to hand it to you. Do with it what makes sense for wherever
`cml` actually is today.
