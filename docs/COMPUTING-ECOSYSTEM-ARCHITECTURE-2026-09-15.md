# Computing Ecosystem — Architecture, Dependencies and Flow (2026-09-15)

![My Computing Ecosystem — Architecture, Dependencies and Flow](architecture/computing-ecosystem-architecture.png)

Owner-authored architecture diagram of the full stack, from Lisp semantics
down to hardware. This repository (`cml`) is the **CML (Compiler for My
Lisp)** layer: it consumes Lisp semantics and verified CPU profiles from
`my-lisp` (Lisp forms, semantic identities/rules, canonical machine forms
— `#175`/`#176`) and produces target-specific code via a frontend, an
optimizer, a code generator, and a target abstraction. It never redefines
language meaning (`cml#46` is the key authority guard for this boundary).

Full stack, top to bottom: User/Developer → **my-lisp** (Language &
Semantics, owns Canon/meaning) → **CML** (this repo) → Target Runtime/OS
(e.g. `wsm-os-lisp`, implements the ABI) → Hardware. External sources (CPU
vendor docs, standards, external oracles) are read-only evidence, never
semantic authority.

This diagram is a snapshot of intent, not itself an authority document —
`juv4uk/ecosystem#7` (the living cross-repo map) remains the authoritative,
updated-in-place source of truth about current ownership and boundaries.
