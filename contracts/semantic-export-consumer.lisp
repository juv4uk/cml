; CML consumer expectations for my-lisp CML-SEMANTIC-EXPORT-V1
; Consumer-side only — does NOT invent language semantics.
; my-lisp remains the sole authority for program meaning.
;
; Owner directive 2026-09-10: compiler stays synchronized with Lisp;
; CML must not hand-duplicate or invent language rules.

((kind . cml-semantic-export-consumer)
 (version . (0 2))
 (status . consuming-slice-1)
 (producer . ((repository . juv4uk/my-lisp)
              (task . MYLISP-CML-SEMANTIC-EXPORT-V1)
              (commit . f142e55)
              (binary . cml-export)
              (authority . language-core)))
 (consumer . ((repository . juv4uk/cml)
              (path . contracts/semantic-export-consumer.my)
              (module . src/semantic_export.rs)
              (vendored-artifact . contracts/mylisp-cml-export.wsm)
              (authority . compiler-middle-end)))
 (required-fields
  . (contract-identity
     semantic-form-identities
     surface-aliases
     admission-status
     export-version
     content-digest))
 (first-vertical-slice
  . ((program-shape . named-def-plus-recursion)
     (form-ids . (0001 0003 0007 0010 0011 1001))
     (forms . (quote eq cond lambda define subtraction))
     (evidence . (tests/semantic_export_slice1_test.rs))
     (note . "count-down through C path; digest soft-pinned until producer file is byte-vendored from a live cml-export run")))
 (non-goals
  . (backend-policy
     second-specification
     raising-global-compatibility-claim
     implementing-wsm-eval-string))
 (integration-plan
  . ((1 . "Vendored contracts/mylisp-cml-export.wsm (committed artifact)")
     (2 . "parse_export + validate_slice1 fail-closed")
     (3 . "Map form roles onto existing Ir / special-forms — no new eval rules")
     (4 . "Hard-pin digest once my-lisp publishes a stable checked-in .wsm")))
 (resolved-by-cml
  . ((delivery . committed-artifact)
     (next-slice-owner . cml-by-real-need)))
 (current-cml-claim
  . ((global-contract . (2 0))
     (partial . (3.0-c-backend-errors 5.0-decimal-reader 6.0-canon-static-reject))
     (via . claim-authority.my))))
