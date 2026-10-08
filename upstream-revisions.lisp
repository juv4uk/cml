; upstream-revisions.lisp — canonical upstream revision channels for CML
; (juv4uk/sens, renamed from juv4uk/my-lisp per #310).
; Revision roles are evidence metadata, not language-semantic authority.
;
; build-source:
;   the external/sens gitlink the CML Rust build compiles against.
;   This is mechanism/API provenance, not a semantic compatibility claim.
; supported-pin:
;   the checked-in external/sens gitlink, used as the semantic
;   compatibility/evidence denominator (#84). It MUST equal the submodule
;   checkout, and it currently coincides with build-source: the two roles
;   are still distinct in principle, but a single gitlink serves both.
;   Collapsing them is a known tension — the denominator no longer moves
;   independently of the build substrate, so compatibility evidence must be
;   gathered per-pin rather than per-contract-version.
; observed-current:
;   exact reproducible snapshot used for drift detection and forward workload
;   intake. It stays an independent exact-github-commit and never follows the
;   gitlink. Moving it never upgrades CML's supported language contract.
; bootstrap-core1-source:
;   exact profile-scoped source snapshot for the Core1 S3/S4/S5 bootstrap lane.
;   Moving it requires fresh bootstrap evidence and never promotes global support.

((kind . cml-upstream-revision-channels)
 (version . (1 2))
 (repository . "juv4uk/sens")
 (build-source . external/sens-gitlink)
 (build-source-sha . "d0787a40fcea36501747732aa61b6de19fe29c70")
 (supported-pin-source . external/sens-gitlink)
 (supported-pin-sha . "d0787a40fcea36501747732aa61b6de19fe29c70")
 (observed-current-source . exact-github-commit)
 (observed-current-sha . "3fd7abcfa83b746de2d417ddfc073a042e6fa511")
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
