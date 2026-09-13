; language-contract.my — the machine-readable semantic-contract version
; for my-lisp, covering exactly Level 1 (CORE SEMANTICS: seven primitives,
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
; Контракт 6.0 ратифіковано 2026-09-08. Це breaking-зміна: раніше
; lexical shadowing дозволяло перевизначити surface spelling Canon-примітива,
; тепер скінченна множина Canon 0+7 names є зарезервованою й резолвиться
; раніше за звичайне lexical Environment.
((major . 6) (minor . 0)
 (note . "RATIFIED by owner 2026-09-08. Contract 6.0 makes Canon 0+7 program resolution genuinely immutable. Every surface spelling present in the immutable Canon registry (historical/English-facing, Ukrainian, Sanskrit, symbolic shorthand) is reserved: Canon resolution happens before ordinary lexical Environment lookup; define/def and lambda binders must reject those names as InvalidForm; language-owned let/let*/macro binding inherits the same prohibition through its lowering path. Callable Canon spellings resolve to one stable first-class handle per canonical identity; PRIM_QUOTE and PRIM_COND remain syntax-only. This intentionally breaks Contract 5.0 programs that shadowed names such as car/перше/ādi. Lexical shadowing remains ALLOWED for non-Canon builtins and ordinary values. Contract 5.0 decimal-separator semantics, Contract 4.0 apostrophe semantics, and Contract 3.0 error classifications remain unchanged. · Контракт 6.0 робить резолюцію Канону 0+7 справді незмінною: усі канонічні EN/UK/SA/символьні написання зарезервовані, мають пріоритет над lexical Environment і не можуть бути binder-іменами. Решта неканонічних builtin-ів і далі можуть затінюватися.")
 (covers . (G1 G2 G3 G4 G5 G6 G7 G8 S1 S2 S3))
 (invariants
   . ((shadowing
       . "Contract 6.0. Lexical shadowing is ALLOWED for ordinary bindings and non-Canon builtins, but the finite Canon 0+7 surface-name set is RESERVED and unshadowable. Any language binder that attempts to bind a Canon spelling fails as InvalidForm. Canon resolution precedes Environment lookup, so even a pre-existing same-text Environment binding cannot replace canonical semantics. This supersedes the owner decision of 2026-08-23/2026-09-06 only for Canon 0+7 names; no general protected namespace or prefix is introduced.")
      (canon-immutability
       . "Contract 6.0. CANON_EMPTY_LIST plus exactly PRIM_QUOTE PRIM_ATOM PRIM_EQ PRIM_CONS PRIM_CAR PRIM_CDR PRIM_COND form the immutable Canon. CANON_EMPTY_LIST is a ground value, not an eighth primitive operation. Historical/English-facing, Ukrainian, Sanskrit and symbolic spellings in the Canon registry resolve directly to these identities before lexical lookup. Callable identities share one stable first-class handle per identity across surfaces; PRIM_QUOTE and PRIM_COND are syntax-only and never become callable values.")
      (special-forms-boundary
        . "quote cond lambda def defmacro are NOT callable values. They are syntactic evaluation rules and remain outside the callable namespace. Canon 6.0 additionally reserves all registered surface spellings of quote and cond against binding.")
      (reader-apostrophe
        . "Contract 4.0: apostrophe is context-sensitive reader syntax. At the start of an expression, 'form desugars exactly to (quote form). Inside an identifier, apostrophe is an ordinary symbol character; об'єкт, п'ять and зв'язок remain single identifiers. The reader must not split internal apostrophes or reinterpret them as nested quote syntax.")
      (reader-decimal-separator
        . "Contract 5.0: dot and comma are equivalent decimal-separator spellings only for an otherwise valid finite decimal/base-10 scientific numeric token. 12.455 and 12,455 denote the same exact rational value; -0,25 and 1,5e3 are valid numeric spellings. A comma in a non-numeric token remains an ordinary symbol character, so а,б and версія1,2 remain symbols; mixed or repeated separators that do not form a valid number also remain symbols under the existing malformed-literal rule.")
      (error-classification
        . "Contract 3.0: ErrorKind is observable semantics. DivisionByZero names a zero divisor; NumericOverflow names an exact-arithmetic magnitude/resource failure; Parse names reader and decoder failures, including malformed json-parse input. InvalidForm is reserved for structurally invalid evaluation forms, disabled capability use, and Contract 6.0 attempts to bind reserved Canon names; it is not arithmetic or decoding failure."))))
