; upstream-revisions.lisp — canonical my-lisp revision channels for CML.
; Revision roles are evidence/provenance metadata, not language-semantic authority.
;
; build-source:
;   exact revision mounted at external/my-lisp and used as the Rust/build/API substrate.
;
; supported-pin:
;   frozen semantic compatibility/evidence denominator.
;
; observed-current:
;   exact reproducible upstream snapshot used for drift/forward-workload observation.
;
; bootstrap-core1-source:
;   exact profile-scoped source snapshot used by Core1 S3/S4/S5 bootstrap evidence.

((kind . cml-upstream-revision-channels)
 (version . (1 2))
 (repository . "juv4uk/my-lisp")

 (build-source-source . external/my-lisp-gitlink)
 (build-source-sha . "5a99136bf7a2e9ab5792bdc945ad53c3774802cd")

 (supported-pin-source . frozen-compatibility-evidence)
 (supported-pin-sha . "8088e9f88d845ba0edb2197d44da3dbbe57eca0e")

 (observed-current-source . exact-github-commit)
 (observed-current-sha . "0a89a03b2565c5e2f7723e6ceaf4878eaeb6844c")

 (bootstrap-core1-source . exact-github-commit)
 (bootstrap-core1-source-sha . "d359c4885e0609a6c8350daf45de157b40cf48f3")
 (bootstrap-core1-profile . core1)

 (historical
  . (((id . reader-contract-4-ratification)
      (sha . "67b6ab17222413af9bf671fee0d5a79217eb6d9a")
      (role . historical-contract-evidence))
     ((id . issue-79-utf8-profile)
      (sha . "ed1ef28928df1db0cc83f17ae993527635b6256d")
      (role . historical-workload-snapshot))))

 (policy
  . ((build-source . compiler-build-api-substrate)
     (supported-pin . compatibility-claim-denominator)
     (observed-current . forward-drift-and-workload-intake)
     (bootstrap-core1-source . profile-scoped-bootstrap-input)
     (historical . immutable-evidence-not-active-channel)
     (build-source-does-not-promote-supported-contract . t)
     (observed-current-does-not-promote-supported-contract . t)
     (bootstrap-core1-source-does-not-promote-supported-contract . t))))
