; CML #74 — upstream machine-form bridge provenance.
; Evidence metadata only. This file does not define Lisp semantics, ISA facts,
; machine-form meaning, or instruction encoding.
(machine-form-bridge-provenance
  (schema cml-machine-form-bridge-provenance/1)
  (upstream-repository "juv4uk/my-lisp")
  (revision-channel supported-pin)
  (revision-sha "8088e9f88d845ba0edb2197d44da3dbbe57eca0e")
  (contract-path "lib/machine/lowering/semantic-x86-64.lisp")
  (contract-git-blob "dc3c8d446680a576d44b79e9939116fe4648194d")
  (bridge-slice (mov-r64-imm64 add-r64-r64 ret))
  (authority
    (semantic my-lisp)
    (machine-form my-lisp)
    (encoding my-lisp)
    (selection-optimization cml))
  (policy
    (drift fail-closed)
    (raw-byte-authority cml-forbidden)
    (unsupported-form reject)))
