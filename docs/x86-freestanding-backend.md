# x86_64 freestanding backend — first slice

`src/x86_freestanding.rs` is a deterministic GNU assembly consumer of the
shared admitted `Ir`. It imports its numeric value representation from the
`wsm-os-target` crate pinned in `Cargo.toml`; it does not copy Rust
`my_lisp::Value`, NaN-boxing, or host pointer layouts.

The initial supported surface is intentionally bounded:

- integer, `()` and `t` immediates;
- quoted integers, symbols, proper lists and dotted lists;
- `cons`, `car`, `cdr`, `eq` and `atom` through the versioned `wsm_*` ABI.

Every other IR node fails during a complete preflight pass before the output
buffer exists. There is no libc, syscall, filesystem, C-backend fallback, or
claim of full my-lisp 3.0 support.

The emitted entry point follows the target contract:

```text
Value wsm_entry(RuntimeContext *context)
```

It preserves the opaque context in callee-saved `r12`, keeps the stack aligned
before runtime calls, preserves the SysV AMD64 callee-saved register, disables
the executable stack note, and returns the final value word in `rax`.

`tests/x86_freestanding_test.rs` assembles generated `.s` with a real host
assembler and inspects the resulting object with `nm -u`. Undefined symbols
must be a subset of the target contract's `wsm_*` runtime imports. Runtime
behavior and QEMU boot parity remain separate later milestones.
