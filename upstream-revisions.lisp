; upstream-revisions.lisp — canonical my-lisp revision channels for CML.
; Revision roles are evidence metadata, not language-semantic authority.
;
; build-source:
;   exact my-lisp crate/registry-format revision used by the CML Rust build.
;   This is mechanism/API provenance, not a semantic compatibility claim.
; supported-pin:
;   exact revision used as the frozen semantic compatibility/evidence denominator.
;   It need not be the Rust build substrate.
; observed-current:
;   exact reproducible snapshot used for drift detection and forward workload intake.
;   Moving it never upgrades CML's supported language contract.
; bootstrap-core1-source:
;   exact profile-scoped source snapshot for the Core1 S3/S4/S5 bootstrap lane.
;   Moving it requires fresh bootstrap evidence and never promotes global support.

((kind . cml-upstream-revision-channels)
 (version . (1 2))
 (repository . "juv4uk/sens")
 (build-source . external/sens-gitlink)
 (build-source-sha . "e908b6934f0f9cfdc1bea132af399f636231be39")
 (supported-pin-source . exact-github-commit)
 (supported-pin-sha . "8088e9f88d845ba0edb2197d44da3dbbe57eca0e")
 (observed-current-source . exact-github-commit)
 (observed-current-sha . "d359c4885e0609a6c8350daf45de157b40cf48f3")
 (bootstrap-core1-source . exact-github-commit)
 (bootstrap-core1-source-sha . "d359c4885e0609a6c8350daf45de157b40cf48f3")
 (historical
  . (((id . reader-contract-4-ratification)
      (sha . "67b6ab17222413af9bf671fee0d5a79217eb6d9a")
      (role . historical-contract-evidence))
     ((id . issue-79-utf8-profile)
      (sha . "ed1ef28928df1db0cc83f17ae993527635b6256d")
      (role . historical-workload-snapshot))))
 (policy
  . ((build-source . compiler-mechanism-api-substrate)
     (build-source-does-not-promote-supported-contract . t)
     (supported-pin . compatibility-claim-denominator)
     (observed-current . forward-drift-and-workload-intake)
     (bootstrap-core1-source . profile-scoped-bootstrap-input)
     (historical . immutable-evidence-not-active-channel)
     (observed-current-does-not-promote-supported-contract . t)
     (bootstrap-core1-source-does-not-promote-supported-contract . t))))
