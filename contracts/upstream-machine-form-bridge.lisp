; CML #74 — upstream machine-form bridge provenance.
; Evidence metadata only. This file does not define Lisp semantics, ISA facts,
; machine-form meaning, or instruction encoding.
(machine-form-bridge-provenance
  (schema cml-machine-form-bridge-provenance/1)
  (upstream-repository "juv4uk/sens")
  (revision-channel supported-pin)
  (revision-sha "c66d4743bb70882c75376dbcec27d393e5a9649d")
  (contract-path "lib/machine/lowering/semantic-x86-64.lisp")
  (contract-git-blob "fa8836a528779ba89ac1d3c67ab548753af0bffd")
  (bridge-slice (mov-r64-imm64 add-r64-r64 ret))
  (authority
    (semantic my-lisp)
    (machine-form my-lisp)
    (encoding my-lisp)
    (selection-optimization cml))
  (mechanism-classification
    (cml-direct-byte differential-bootstrap)
    (gnu-as differential-oracle)
    (upstream-admission-encoder normative-machine-contract))
  (policy
    (drift fail-closed)
    (raw-byte-authority cml-forbidden)
    (unsupported-form reject)))
