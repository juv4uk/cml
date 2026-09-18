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
 (supported-pin-sha . "8088e9f88d845ba0edb2197d44da3dbbe57eca0e")
 (observed-current-source . exact-github-commit)
 (observed-current-sha . "b5a128f8b1c7c85ded7670be8f94f1e153e7f8e3")
 (policy
  . ((supported-pin . compatibility-claim-denominator)
     (observed-current . forward-drift-and-workload-intake)
     (observed-current-does-not-promote-supported-contract . t))))
