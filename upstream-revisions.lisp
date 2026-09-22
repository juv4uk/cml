; upstream-revisions.lisp — canonical my-lisp revision channels for CML.
; Revision roles are evidence metadata, not language-semantic authority.
;
; supported-pin:
;   exact revision CML may use as its compatibility/evidence denominator.
; observed-current:
;   exact reproducible snapshot used for drift detection and forward workload intake.
;   Moving it never upgrades CML's supported language contract.

((kind . cml-upstream-revision-channels)
 (version . (1 0))
 (repository . "juv4uk/my-lisp")
 (supported-pin-source . external/my-lisp-gitlink)
 (supported-pin-sha . "5a99136bf7a2e9ab5792bdc945ad53c3774802cd")
 (observed-current-source . exact-github-commit)
 (observed-current-sha . "5a99136bf7a2e9ab5792bdc945ad53c3774802cd")
 (historical
  . (((id . reader-contract-4-ratification)
      (sha . "67b6ab17222413af9bf671fee0d5a79217eb6d9a")
      (role . historical-contract-evidence))
     ((id . issue-79-utf8-profile)
      (sha . "ed1ef28928df1db0cc83f17ae993527635b6256d")
      (role . historical-workload-snapshot))))
 (policy
  . ((supported-pin . compatibility-claim-denominator)
     (observed-current . forward-drift-and-workload-intake)
     (historical . immutable-evidence-not-active-channel)
     (observed-current-does-not-promote-supported-contract . t))))
