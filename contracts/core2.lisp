; contracts/core2.lisp — CML research artifact: frozen legacy compatibility profile
; for my-lisp Core2 / Language Contract 6.0.
;
; This is a provisional placement in CML per owner direction 2026-09-22.
; Authoritative version belongs in juv4uk/my-lisp (issue #1133).
;
; Baseline: my-lisp commit 35c88142548dad137689cd69ca91c430da148bea
;            Last commit before f5947ee5 activates structural atom/eq results.
; Contract: Language Contract 6.0, ratified 2026-09-08.

(core2-profile/1
  (authority
    (repository . "juv4uk/my-lisp")
    (baseline-sha . "35c88142548dad137689cd69ca91c430da148bea")
    (contract-version . (6 0))
    (ratified . "2026-09-08")
    (note . "Pre-revolution my-lisp: Canon 0+7, historical truth/NIL model, closed primitive set."))

  (scope
    (execution-profile . "Core2")
    (compatibility-target . "Contract 6.0 programs")
    (not-authority-over . "Core1 bootstrap, Core3 island, Core4 current"))

  (differences-from-core4
    ((form . atom)
     (core2-result . boolean-truth)
     (core4-result . structural-kind-record)
     (witness-programs .
       (((expr . "(atom (quote radio))") (expected . t))
        ((expr . "(atom (quote ()))") (expected . t))
        ((expr . "(atom (quote (radio antenna)))") (expected . ())))))

    ((form . eq)
     (core2-result . boolean-truth)
     (core4-result . identity-relation-record)
     (witness-programs .
       (((expr . "(eq (quote radio) (quote radio))") (expected . t))
        ((expr . "(eq (quote radio) (quote antenna))") (expected . ()))
        ((expr . "(eq 3 3)") (expected . t))
        ((expr . "(eq 3 4)") (expected . ())))))

    ((form . equal?)
     (core2-result . boolean-truth)
     (core4-result . structural-relation-record)
     (witness-programs .
       (((expr . "(equal? (quote (p . 0)) (cons (quote p) 0))") (expected . t)))))

    ((form . cond)
     (core2-semantics . two-part-clauses-truthiness)
     (core4-semantics . three-part-explicit-match)
     (core4-failure . UnsatisfiedConditional)
     (migration-note . "Two-part cond remains a bounded compatibility bridge in Core4; Core2 is the native semantics.")))

  (compatibility-projection-to-core4
    ; Conceptual wrappers that project Core2 source onto Core4 semantics.
    ; A real implementation would provide these as macros or compiler passes.
    ((name . core2/atom)
     (params . (x))
     (projection .
       "(cond (((eq (atom x) (structural-kind atom)) t)
               (((eq (atom x) (structural-kind empty-list)) t)
               (t ())))"))
    ((name . core2/eq)
     (params . (x y))
     (projection .
       "(cond (((eq (eq x y) (identity-relation same)) t)
               (t ())))"))
    ((name . core2/equal?)
     (params . (x y))
     (projection .
       "(cond (((eq (equal? x y) (structural-relation same)) t)
               (t ())))"))
    ((name . core2/cond)
     (params . clauses)
     (projection .
       "Transform each Core2 (test expr) clause into Core4 (test t expr).")))

  (implementation-notes
    (rust-diff . "crates/my-lisp/src/eval/special_forms/core.rs between 35c8814 and f5947ee5")
    (contract-diff . "contracts/structural-observation-contract.lisp between 35c8814 and f5947ee5")
    (key-transition-commit . "f5947ee5")
    (key-transition-message . "feat(#218): activate structural atom/eq results after explicit control")
    (current-core4-contract . "8.0 ratified 2026-09-20")))
