;; CONTRACT: MACHINE-MACRO-ATOMS-V1
;;
;; Architectural Role:
;; Lisp-level macro-atom library for authoring x86 machine sequences.
;; High-level machine idioms expand to structured target (x86 ...) forms.
;;
;; Epistemology & Authority:
;; - The low-level CML encoder only validates and encodes machine mechanism.
;; - Reusable machine idioms (fixnum tagging, frame setup, ABI sequences)
;;   reside at the Lisp macro level, not hardcoded into the Rust byte encoder.
;; - Contains target facts only: registers, opcodes, displacements, labels.
;; - Carries NO source-language semantic IDs (semantic_id = None).
;; - Carries NO language surface keywords (+, add, додати).

;; Tag a raw integer into a tagged fixnum representation
(defmacro tag-fixnum (reg)
  (list (quote x86) (quote shl-imm) reg 3))

;; Untag a tagged fixnum back to raw integer representation
(defmacro untag-fixnum (reg)
  (list (quote x86) (quote sar-imm) reg 3))

;; Load 64-bit immediate constant into register
(defmacro mov-imm (reg imm)
  (list (quote x86) (quote mov-imm64) reg imm))

;; Register-to-register move
(defmacro mov-reg (dst src)
  (list (quote x86) (quote mov-reg-reg) dst src))

;; Add 32-bit immediate to register
(defmacro add-imm (reg imm)
  (list (quote x86) (quote alu-imm32) (quote add) reg imm))

;; Subtract 32-bit immediate from register
(defmacro sub-imm (reg imm)
  (list (quote x86) (quote alu-imm32) (quote sub) reg imm))

;; Fast return from procedure
(defmacro fast-ret ()
  (list (quote x86) (quote ret)))
