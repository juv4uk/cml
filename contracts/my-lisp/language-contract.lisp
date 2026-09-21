; language-contract.my — the machine-readable semantic-contract version
; for my-lisp, covering Level 1 (CORE SEMANTICS: Canon 0, stable McCarthy-7 root,
; lambda, truth/NIL, symbols, pairs) and Level 2 (LANGUAGE CONTRACT:
; exactness, def/defmacro, errors, read/eval) from
; docs/language-core-axioms.md — deliberately NOT Level 3 (ECOSYSTEM
; CONFORMANCE: core.my/unify.my/reason.my/knowledge.my, literate markdown,
; CLIPS), which changes independently and far more often.
;
; Proposed 2026-08-10 to synchronize my-lisp, fpga-lisp, and cml (a new
; AOT compiler from my-lisp source to fpga-lisp's ISA) around a real
; question — "which semantic contract do you implement" — instead of a
; commit SHA or a shared release-version number. Tying all three to one
; version (e.g. "everyone is 0.15.0") was explicitly considered and
; rejected: it creates a false dependency where my-lisp could ship ten
; purely additive/library/dedup releases while the actual Level 1/2
; semantic contract never moved — and vice versa, fpga-lisp's own ISA can
; change without my-lisp's semantics changing at all. Compatibility is a
; PAIR of versions (language-contract, ISA-version), not one shared number.
;
; major: bumped on a breaking semantic change — an existing, previously
; well-defined program could now observe different behavior.
; minor: bumped on an additive, backward-compatible semantic change.
;
; language-contract.my — машинно-читана версія семантичного контракту
; my-lisp, що покриває рівно Рівень 1 (СЕМАНТИКА ЯДРА) і Рівень 2
; (КОНТРАКТ МОВИ) з docs/language-core-axioms.md — свідомо НЕ Рівень 3.
;
; Контракт 8.0 ратифіковано 2026-09-20. Це breaking semantic change:
; канонічний тричленний COND більше не маскує вичерпання під Canon 0/NIL,
; а повертає named failure UnsatisfiedConditional. Порожній COND так само
; є незадоволеним. Migration-only двочленний bridge тимчасово зберігає
; історичну truthiness. Правила Contract 7.0/ADR-005 та Contract 6.0 зберігаються.
((major . 8) (minor . 0)
 (note . "RATIFIED by owner 2026-09-20. Contract 8.0 makes canonical three-part COND fail closed: if no clause query equals its explicit expected-result, evaluation yields the named UnsatisfiedConditional failure instead of Canon 0/NIL. Empty COND is likewise unsatisfied. Migration-only two-part COND remains a bounded compatibility bridge and retains historical truthiness until retired. Contract 7.0 primitive-admission and kernel-archipelago rules, Contract 6.0 shadowing, Contract 5.0 decimal-separator semantics, Contract 4.0 apostrophe semantics, and prior semantic decisions remain otherwise unchanged.")
 (covers . (G1 G2 G3 G4 G5 G6 G7 G8 S1 S2 S3))
 (invariants
   . ((shadowing
       . "Contract 6.0. Lexical shadowing is ALLOWED for ordinary bindings and non-Canon builtins, but the finite Canon 0+7 surface-name set is RESERVED and unshadowable. Any language binder that attempts to bind a Canon spelling fails as InvalidForm. Canon resolution precedes Environment lookup, so even a pre-existing same-text Environment binding cannot replace canonical semantics. This supersedes the owner decision of 2026-08-23/2026-09-06 only for Canon 0+7 names; no general protected namespace or prefix is introduced.")
      (canon-immutability
       . "Contract 7.0. CANON_EMPTY_LIST plus PRIM_QUOTE PRIM_ATOM PRIM_EQ PRIM_CONS PRIM_CAR PRIM_CDR PRIM_COND remain stable canonical identities and their registered Canon spellings remain reserved/unshadowable. This is a stable historical/minimal root, not a permanent upper bound on primitive admission. Additional semantic identities may be admitted only through the evidence discipline of ADR-005 and must not arise accidentally from runtime helpers, kernel internals, ABI functions or hardware opcodes.")
      (primitive-admission
       . "Contract 7.0. Primitive count is not predetermined. A new primitive identity requires executable evidence that its distinction is externally observable, compositionally necessary, shared by independent witnesses, or cannot be represented honestly by existing identities plus ordinary data. One 8-bit semantic-ID space remains the active experimental budget; kernel-private ontologies do not automatically consume IDs.")
      (kernel-archipelago
       . "Contract 7.0. my-lisp owns semantic identity, Canon, surfaces and laws. Common Lisp, Prolog, Datalog, CLIPS and future kernels may own their native execution/search/fixpoint/rule mechanisms and native result multiplicity. A kernel may witness zero or more semantic identities; one semantic identity may have zero or more execution witnesses. Kernel execution never redefines SID meaning.")
      (canonical-cond
       . "Contract 8.0. Canonical COND clauses have exactly three fields: (query expected-result expression). Queries are evaluated left-to-right; expected-result is data, not code; only the expression of the first exact structural match is evaluated. If no canonical clause matches, including an empty COND, evaluation fails with UnsatisfiedConditional rather than returning Canon 0/NIL. Any two-part clause marks the form as migration-only compatibility and retains historical truthiness until that bridge is retired.")
      (special-forms-boundary
        . "quote cond lambda def defmacro are NOT callable values. They are syntactic evaluation rules and remain outside the callable namespace. Canon 6.0 additionally reserves all registered surface spellings of quote and cond against binding.")
      (reader-apostrophe
        . "Contract 4.0: apostrophe is context-sensitive reader syntax. At the start of an expression, 'form desugars exactly to (quote form). Inside an identifier, apostrophe is an ordinary symbol character; об'єкт, п'ять and зв'язок remain single identifiers. The reader must not split internal apostrophes or reinterpret them as nested quote syntax.")
      (reader-decimal-separator
        . "Contract 5.0: dot and comma are equivalent decimal-separator spellings only for an otherwise valid finite decimal/base-10 scientific numeric token. 12.455 and 12,455 denote the same exact rational value; -0,25 and 1,5e3 are valid numeric spellings. A comma in a non-numeric token remains an ordinary symbol character, so а,б and версія1,2 remain symbols; mixed or repeated separators that do not form a valid number also remain symbols under the existing malformed-literal rule.")
      (error-classification
        . "ErrorKind is observable semantics. Contract 8.0 adds UnsatisfiedConditional for a structurally valid canonical COND whose clauses are exhausted without an explicit expected-result match. InvalidForm remains reserved for malformed evaluation forms, disabled capability use, and Contract 6.0 attempts to bind reserved Canon names. Contract 3.0 still separates DivisionByZero, NumericOverflow, Parse, and the earlier named categories."))))
