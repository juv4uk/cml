; CML-SEMANTIC-IR-RECOVERY-M0 progress inventory — 2026-09-09
; Not a completion claim. Executable evidence only for what is listed.
; Updated same-day after claim-authority + Quoted explicit arms + rejection tests.

((kind . semantic-ir-recovery-inventory)
 (date . "2026-09-09")
 (status . partial)
 (task . CML-SEMANTIC-IR-RECOVERY-M0)

 (ir-variants-in-src-ir-rs
  . (Int Float Rational String Buffer Nil True Var Builtin Quote
     Lambda App Cond Let Def Prim TailSelfCall))

 (source-lowering
  . ((module . "src/lower.rs")
     (admits . (Int Rational String BufferI32 BufferF32 Nil True Var Builtin
                Quote Cond Lambda Let Def Prim App TailSelfCall-via-mark-pass))
     (rejects-named . (unquoted-dotted-list special-forms-as-values
                       reserved-canon-binders-via-semantic-gate
                       f32-buffer-via-semantic-gate
                       unquoted-string-via-semantic-gate))
     (note . "Float is in Ir enum but not produced by current parser/lower path.")))

 (fpga-validate-ir-matrix
  . ((Int . supported-if-in-LOADI-range)
     (Float . named-unsupported)
     (Rational . named-unsupported)
     (String . named-unsupported)
     (Buffer . named-unsupported-numeric-buffer)
     (Nil . supported)
     (True . supported)
     (Var . supported)
     (Builtin . named-unsupported)
     (Quote . supported-if-quoted-admitted)
     (Lambda . supported)
     (App . supported-max-8-args)
     (Cond . supported)
     (Let . supported-as-lambda-app)
     (Def . supported-top-level)
     (Prim . supported)
     (TailSelfCall . named-unsupported)))

 (quoted-validate-matrix
  . ((Int . supported-if-in-LOADI-range)
     (Float . named-unsupported)
     (Rational . named-unsupported)
     (Sym . supported)
     (Str . supported-as-LOADSYM)
     (Nil . supported)
     (List . supported)
     (DottedList . supported)))

 (backend-classification-evidence
  . ((fpga-lisp . "tests/exhaustive_classification_test.rs")
     (regression-2026-09-09 . (tests/semantic_ir_rejection_test.rs
                               builtin_is_typed_rejection_not_panic_on_fpga
                               rational_is_typed_rejection_not_panic_on_fpga
                               quoted_float_is_typed_rejection_not_panic_on_fpga
                               int_still_emits))))

 (historical-bug-fixed
  . ((Ir-Builtin-admitted-but-unreachable . "validate_ir returns UnsupportedVariant(Builtin); verified")))

 (wildcard-panic-audit
  . ((src/compiler.rs-compile_quoted . "explicit Float/Rational arms; no `_` wildcard")
     (src/compiler.rs-compile_expr . "all Ir variants explicit; unreachable only after validate")))

 (contract-scope
  . ((global-claim . (2 0))
     (authority . "compatibility.my claim-authority + tests/contract_claim_authority_test.rs")))

 (still-open-for-m0-completion
  . (full-my-lisp-contract-6.0-obligation-inventory-cross-repo
     machine-checked-matrix-table-exported-as-artifact
     x86-wsm-os-target-pin-to-current-not-deprecated-lab
     pushed-code-on-origin-master)))
