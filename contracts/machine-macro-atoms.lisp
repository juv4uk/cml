;; CONTRACT: MACHINE-MACRO-ATOMS-V1
;;
;; Lisp-authored target mechanism. These macros expand only to structured
;; `(x86 ...)` machine forms; they own no source-language semantic IDs and no
;; EN/UK/UKR/SA surface spellings. CML validates and encodes the target facts.

(defmacro tag-fixnum (reg)
  (list (quote x86) (quote shl-imm) reg 3))

(defmacro untag-fixnum (reg)
  (list (quote x86) (quote sar-imm) reg 3))

(defmacro mov-imm (reg imm)
  (list (quote x86) (quote mov-imm64) reg imm))

(defmacro mov-reg (dst src)
  (list (quote x86) (quote mov-reg-reg) dst src))

(defmacro add-imm (reg imm)
  (list (quote x86) (quote alu-imm32) (quote add) reg imm))

(defmacro sub-imm (reg imm)
  (list (quote x86) (quote alu-imm32) (quote sub) reg imm))

(defmacro fast-ret ()
  (list (quote x86) (quote ret)))
