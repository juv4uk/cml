; CML #74 — upstream machine-form bridge provenance.
; Evidence metadata only. This file does not define Lisp semantics, ISA facts,
; machine-form meaning, or instruction encoding.
(machine-form-bridge-provenance
  (schema cml-machine-form-bridge-provenance/1)
  (upstream-repository "juv4uk/my-lisp")
  (revision-channel supported-pin)
  (revision-sha "5a99136bf7a2e9ab5792bdc945ad53c3774802cd")
  (contract-path "lib/machine/lowering/semantic-x86-64.lisp")
  (contract-git-blob "dd5d393a0a443202dd4b9954bba92812334d8612")
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
