; language-contract.lisp — current machine-readable Level 1/2 contract.
;
; Contract 10.0 ratified by owner 2026-09-29 under #1703/#1831.
; sens (СЕНС) keeps exactly one function-identity space:
; 00000000..11111111, all 256 slots reserved for functions.
;
; Constitutional change from Contract 9:
; Core1/Core2/Core3/Core4 share one predicate/control foundation for
; 00000010 ATOM, 00000011 EQ and 00000111 COND.
; Predicate answers are exact one-bit values: 1 YES / 0 NO.
; COND clauses are exactly (test expression); exhaustion returns structural ().
;
; Any remaining T/NIL truthiness, graded answers, three-part COND or
; UnsatisfiedConditional exhaustion path is implementation migration debt,
; not an alternate language law.

((major . #d10) (minor . 0)
 (note . "RATIFIED by owner 2026-09-29 under #1703/#1831. Contract 10.0 preserves the single exact eight-bit function space 00000000..11111111 and ratifies one shared Core1-Core4 predicate/control foundation: ATOM and EQ return exact one-bit PredicateBit 1/0; COND uses only (test expression), accepts only PredicateBit tests, and returns structural () on exhaustion. Historical T/NIL truthiness, graded answers, three-part COND and UnsatisfiedConditional exhaustion are migration/history debt, not alternate laws.")
 (covers . (G1 G2 G3 G4 G5 G6 G7 G8 S1 S2 S3))
 (invariants
   . ((sid8-function-space
       . "The complete function-identity space is exactly 00000000..11111111. All 256 slots are reserved exclusively for functions. A function identity is the eight bits themselves; it is not text, a string, a symbol, a literal, a decimal number, a human name, an enum label, an opcode, or a backend identifier.")
      (single-function-ontology
       . "There is no second named function ontology. Runtime, compiler IR, contracts, backends and tooling must not introduce named identities beside Sid8. Internal mechanism metadata may describe how an already-selected SID executes, but may never rename or redefine the function.")
      (surface-non-authority
       . "Human-language and symbolic surfaces are optional source/UI routing metadata only. A surface may resolve mechanically to Sid8; it never creates a function, owns meaning, or becomes an identity. No name->meaning->SID or SID->named-meaning layer is authoritative.")
      (predicate-one-bit
       . "Predicate answers are exactly one contextual bit: 1 means YES and 0 means NO. PredicateBit is not Number, host Bool, T/NIL, Symbol, or structural (). No third predicate answer and no graded-width truth value is active.")
      (atom-one-bit-core1-4
       . "Function 00000010 ATOM has one law across Core1/Core2/Core3/Core4: structural empty () and every admitted non-pair value answer PredicateBit 1; pair answers PredicateBit 0. Structural () is an ATOM-yes subject, not a truth value.")
      (eq-one-bit-core1-4
       . "Function 00000011 EQ has one law across Core1/Core2/Core3/Core4: the same admitted atom answers PredicateBit 1; distinct admitted atoms answer PredicateBit 0; pair/out-of-domain input raises the named domain/type failure. EQ is not deep structural equality.")
      (core-profile-law
       . "Core1/Core2/Core3/Core4 are law profiles over the same Sid8 identities. Profiles may differ outside the shared foundation, but they may not assign different laws/results to Function 00000010 ATOM, 00000011 EQ, or 00000111 COND.")
      (kernel-archipelago
       . "Execution kernels may own native mechanisms and observations. They consume an already-selected Sid8 plus arguments/context and may return native observations. Kernel operator names, opcodes and native types never acquire sens function identity by themselves.")
      (sid-00000111-control
       . "Function 00000111 COND has one law across Core1/Core2/Core3/Core4. Every clause has exactly two fields: (test expression). Tests are evaluated left-to-right and must return exact PredicateBit. PredicateBit 1 selects and evaluates that clause expression; PredicateBit 0 skips it. If no clause selects, COND returns structural (). Structural () is not a predicate answer.")
      (reader-apostrophe
       . "At expression start, apostrophe is reader sugar whose produced list head is SID 00000001 directly. It must not create an intermediate named function identity. Inside an identifier, apostrophe remains an ordinary Unicode character.")
      (reader-eight-bits
       . "Exactly eight bare 0/1 source characters are read directly into Sid8. The reader does not reinterpret them as decimal or binary numeric data and does not wrap them in String/Symbol.")
      (reader-decimal-separator
       . "Dot and comma are equivalent decimal separators only for otherwise valid finite decimal/base-10 scientific numeric input. Non-numeric tokens retain their ordinary data behavior; this rule never affects Sid8.")
      (error-classification
       . "Named error categories remain observable where separately admitted, but UnsatisfiedConditional is not the exhaustion law of Function 00000111. Any remaining producer or witness for three-part COND exhaustion is transition debt and must not override the Contract 10 foundation.")
      (migration-debt-00000000
       . "Historical #1332 migration debt is complete in the current runtime: () is represented as a structural empty value outside the function space, and function 00000000 is not the empty-list value and receives no replacement ground-value identity."))))
