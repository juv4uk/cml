; Machine-readable authority boundary between language meaning and target lowering.
; This file deliberately contains no target mnemonic, register, opcode, or ISA name.
; my-lisp owns observable language meaning and semantic IDs. CML owns compiler
; machine identity and target realization. A backend fact can never allocate a
; language semantic identity in the reverse direction.

(machine-lowering-boundary
  (schema machine-lowering-boundary/1)
  (semantic-authority my-lisp)
  (compiler-authority cml)
  (portable-monotonic-observation 1075)
  (machine-instruction-identity compiler-owned)
  (semantic-id-allocation explicit-language-contract-only)
  (lowering-direction semantic-to-machine)
  (reverse-authority forbidden)
  (raw-machine-instructions nonportable-compiler-mechanism)
  (machine-text projection-only)
  (machine-bytes projection-only)
  ; 1153 briefly reached main from a compiler-originated experiment. It is burned:
  ; never assign it to another language meaning, while keeping it out of the active
  ; semantic registry because no language contract admitted that operation.
  (retired-semantic-id 1153))
