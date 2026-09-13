; Vendored from my-lisp CML-SEMANTIC-EXPORT-V1 producer (cml-export binary).
; Schema: cml-export/1 — see my-lisp docs/cml-semantic-export-v1-design.md.
; Slice 1 forms only: quote/cond/lambda/define/eq/subtraction (fixture #69 shape).
; Digest is FNV-1a-64 over the forms block bytes as produced by my-lisp.
; When the producer re-emits, replace this file and update the pinned digest test.
;
; Real producer run (cargo run --bin cml-export in my-lisp), not hand-authored --
; issue cml#3 item 1: replaces the earlier pending-producer-byte-pin placeholder.

(cml-export/1
  (contract (major 6) (minor 0))
  (digest "dfc880e5e5ae80f9")
  (forms
    (0001 (surfaces (en quote) (sa svarūpa) (sym ') (uk як-є)) (role syntax) (callable nil))
    (0007 (surfaces (en cond) (sa anukrama) (sym ?:) (uk за-умовою)) (role syntax) (callable nil))
    (0010 (surfaces (en lambda) (uk функція)) (role syntax) (callable nil))
    (0011 (surfaces (en define) (uk визначити)) (role syntax) (callable nil))
    (0003 (surfaces (en eq) (sa abheda) (sym =?) (uk тотожне?)) (role primitive) (callable t))
    (1001 (surfaces (sa viyoga) (sym -) (uk відняти)) (role library) (callable t))))
