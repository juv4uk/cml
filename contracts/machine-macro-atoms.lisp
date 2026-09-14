;; CONTRACT: MACHINE-MACRO-ATOMS-V1
;;
;; Lisp-authored target mechanism. These macros expand only to structured
;; `(x86 ...)` machine forms; they own no source-language semantic IDs and no
;; EN/UK/UKR/SA surface spellings. CML validates and encodes the target facts.
;;
;; Keep this layer inside the existing canonical macro meta-evaluator:
;; quote/cons/car/cdr/atom/eq/cond. Do not grow host-side macro semantics just
;; to make machine forms more convenient to author.

(defmacro tag-fixnum (reg)
  (cons (quote x86)
    (cons (quote shl-imm)
      (cons reg
        (cons 3 NIL)))))

(defmacro untag-fixnum (reg)
  (cons (quote x86)
    (cons (quote sar-imm)
      (cons reg
        (cons 3 NIL)))))

(defmacro mov-imm (reg imm)
  (cons (quote x86)
    (cons (quote mov-imm64)
      (cons reg
        (cons imm NIL)))))

(defmacro mov-reg (dst src)
  (cons (quote x86)
    (cons (quote mov-reg-reg)
      (cons dst
        (cons src NIL)))))

(defmacro add-imm (reg imm)
  (cons (quote x86)
    (cons (quote alu-imm32)
      (cons (quote add)
        (cons reg
          (cons imm NIL))))))

(defmacro sub-imm (reg imm)
  (cons (quote x86)
    (cons (quote alu-imm32)
      (cons (quote sub)
        (cons reg
          (cons imm NIL))))))

(defmacro fast-ret ()
  (cons (quote x86)
    (cons (quote ret) NIL)))
