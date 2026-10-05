; language-contract.lisp — current machine-readable Level 1/2 contract.
;
; Contract 11.6 — owner-ratified D1–D7 foundation; D8 research.
; Owner paradigm: #2490. Implementation cutover: #2817 / #2822.
;
; Contract 11 preserves the observable PredicateBit / ATOM / EQ / COND law
; ratified in Contract 10 while replacing the superseded flat Function8
; ontology with the current exact-domain ontology:
;
;   semantic object
;   = exact binary number
;   + exact domain
;   + proved / ratified law
;
; The exact Contract 10.0 source is preserved as NON-NORMATIVE provenance at:
;   docs/archive/historical/language-contract-10.0.lisp
;
; Historical Sens8/Sid8/Function8 machinery may remain only as explicitly
; bounded compatibility / transport / backend projection during migration.
; It cannot mint or redefine canonical semantic identity.

((major . #d11) (minor . 6)
 (status . current-domain-qualified-authority)
 (supersedes . "Contract 10.0 flat Function8 identity authority")
 (historical-snapshot . "docs/archive/historical/language-contract-10.0.lisp")
 (note . "Contract 11.6 owner-ratifies D1–D7 as the current foundation chain. D1–D6 retain their prior authorities; D7 is owner-ratified 126/128 in #3572 from the consolidated law-first D7-v2 cut, with 0100001 and 0101010 remaining owner-reserved/pinned. D8 remains UNRATIFIED / RESEARCH. Exact carriers W1-W8 remain mechanical independently of semantic admission.")
 (covers . (G1 G2 G3 G4 G5 G6 G7 G8 S1 S2 S3))
 (invariants
   . ((binary-domain-identity
       . "Every canonical semantic object is identified by its exact binary number together with its exact domain and the proved or ratified law that interprets that object in that domain. Equal packed numeric payloads in different widths or domains are not thereby the same identity.")
      (domain-is-interpretation-boundary
       . "A domain is the stated carrier/admissibility context and laws that interpret its resident binary objects. A domain is not inferred from numeric payload, width, human spelling, host type, table position, or backend opcode.")
      (domain-non-inference
       . "Bits or width alone never mint semantic membership, occupancy, callability, or meaning. A syntactically valid binary coordinate remains unallocated or unknown until admitted by the owning domain law.")
      (no-width-coercion
       . "Zero-padding, truncation, low-nibble extraction, integer equality, prefix resemblance, or any other width-changing transform cannot create or recover canonical domain identity. A compatibility projection is legal only when a separately stated role-aware law proves that exact projection.")
      (legacy-sens8-compatibility
       . "Historical Sens8/Sid8/Function8 values may remain only as explicitly named compatibility, transport, provenance, or backend mechanism projections while migration proceeds. They are not universal semantic identity and never compare equal to a domain-qualified object solely from packed bits.")
      (surface-non-authority
       . "Human-language and symbolic surfaces are optional source/UI routing metadata. A surface may resolve mechanically to an already-admitted domain-qualified identity or an explicitly legacy compatibility projection; it never creates semantic identity, owns meaning, or becomes semantic authority.")
      (predicate-one-bit
       . "Core.D1 PredicateBit answers are exactly one contextual bit: 1 means YES and 0 means NO. PredicateBit is not Number, host Bool, T/NIL, Symbol, structural (), or any wider-domain value. No third predicate answer and no graded-width truth value is active.")
      (structure-two-bit
       . "Core.D2 racana2 is exact two-bit structural syntax under its ratified law: 00 separator, 01 close, 10 open, 11 dot. These are structure-domain objects, not numeric or callable identities merely because they are binary.")
      (d3-foundation
       . "Core.D3 bīja3 is the owner-ratified exact three-bit foundation (#3202): 000 structural empty (), 001 QUOTE, 010 ATOM, 011 CDR, 100 CAR, 101 EQ, 110 COND, 111 CONS. Human role names are documentation projections. Historical exact-eight-bit forms are role-aware compatibility projections only.")
      (d3-l1-l5-constitution
       . "The D3 map is fixed by the owner-ratified L1-L5 stack: L1 000 is structural empty; L2 D3 preserves exact D2 prefix fibres 00→()/QUOTE, 01→ATOM/CDR, 10→CAR/EQ, 11→COND/CONS; L3 one uniform semantic duality covers ()↔CONS, QUOTE↔COND, ATOM↔EQ, CDR↔CAR; L4 that D3 dual is XOR 111, recursively matching D1 XOR 1 and D2 XOR 11; L5 orients suffix-0 as the evaluator/metalinguistic spine ()→ATOM→CAR→COND. This law supersedes every previous current D3 coordinate ordering; old orderings survive only as historical/provenance evidence.")
      (d4-bootstrap
       . "Core.D4 is the owner-ratified full compact four-bit bootstrap domain (#3272): 0000 APPLY, 0001 EVAL, 0010 LAMBDA, 0011 DEFINE, 0100 NOT, 0101 NULL, 0110 CDAR, 0111 CDDR, 1000 CAAR, 1001 CADR, 1010 LOOKUP, 1011 BIND, 1100 EVCON, 1101 EVLIS, 1110 LIST, 1111 APPEND. Old D4/SID8/Sens8/Function8 coordinates have zero placement authority.")
      (d4-fibre-law
       . "D4 is dense 16/16 and grouped by the ratified D3 semantic parent: EMPTY→APPLY/EVAL, QUOTE→LAMBDA/DEFINE, ATOM→NOT/NULL, CDR→CDAR/CDDR, CAR→CAAR/CADR, EQ→LOOKUP/BIND, COND→EVCON/EVLIS, CONS→LIST/APPEND. Residency is compact identity, not a claim that every resident is an irreducible primitive.")
      (d4-null-not-distinction
       . "D4 0100 NOT and 0101 NULL are distinct because D1 PredicateBit 0 is not D3 structural empty (). NOT complements exact PredicateBit; NULL recognizes structural empty as a derived compact resident.")
      (d4-list-append-distinction
       . "D4 1110 LIST and 1111 APPEND are CONS-family derived residents: LIST collects supplied values into one proper list; APPEND combines admitted lists into one list. Their four-bit residency is for compact identity, not extra primitive power.")
      (d5-full-compact
       . "Core.D5 is the owner-ratified full compact five-bit domain (#3305), dense 32/32 with 32 distinct residents and zero lower-domain semantic duplicates. Its exact resident map is normative in contracts/d5-ratification.lisp and knowledge/d5-ratified.json.")
      (d5-local-law-federation
       . "D5 has no universal fifth-bit meaning. Its resident pairs are governed locally: five SEMANTIC-GENERATOR families, five LOCAL-ALGEBRA families, two MULTI-DELTA-FAMILY pairs, and four COORDINATE-HISTORICAL pairs. Historical adjacency is not promoted into a semantic law without evidence.")
      (d5-runtime-separation
       . "D5 semantic residency and callable mechanism are separate facts. Every exact W5 coordinate has ratified D5 identity; invocation succeeds only where a resident mechanism is admitted and otherwise fails closed.")
      (d6-ratified-status
       . "Core.D6 is OWNER-RATIFIED 64/64 under #3393. Its exact resident map is normative in contracts/d6-ratification.lisp and knowledge/d6-ratified.json. The S4 gauge choice is now coordinate authority by owner decision, while remaining explicitly distinguished from pre-ratification derivation proofs. Ratified residency does not imply callable mechanism; missing mechanisms fail closed.")
      (d7-ratified-status
       . "Core.D7 is OWNER-RATIFIED 126/128 under #3572. Its exact resident map is normative in contracts/d7-ratification.lisp and knowledge/d7-ratified.json. The 19 same-coordinate Shiva overlays are admitted by owner decision; 0100001 and 0101010 remain owner-reserved/pinned, not free. Text digits remain Text, LocalOrdinal remains a separate W7 role, and D7 residency does not imply generic callable-Core mechanism.")
      (d8-research-status
       . "Core.D8 is UNRATIFIED / RESEARCH under #3572. Exact W8 carrier identity is width-preserving but does not grant Core.D8 semantic occupancy or callability and never collapses into historical Sens8/Sid8/Function8.")
      (cross-domain-non-collapse
       . "The same packed numeric payload may coexist in D1, D2, D3, D4, D5, D6, D7, D8 or Core-Math domains without semantic equality. Cross-domain reuse requires an explicit independently proved bridge law.")
      (atom-one-bit-core1-4
       . "Core.D3 010 ATOM has one law across Core1/Core2/Core3/Core4: structural empty () and every admitted non-pair value answer PredicateBit 1; pair answers PredicateBit 0. Structural () is an ATOM-yes subject, not a truth value. Historical Function8 00000010 is compatibility projection only.")
      (eq-one-bit-core1-4
       . "Core.D3 101 EQ has one law across Core1/Core2/Core3/Core4: the same admitted atom answers PredicateBit 1; distinct admitted atoms answer PredicateBit 0; pair/out-of-domain input raises the named domain/type failure. EQ is not deep structural equality. Historical Function8 00000011 is compatibility projection only.")
      (cond-two-part-core1-4
       . "Core.D3 110 COND has one law across Core1/Core2/Core3/Core4. Every clause has exactly two fields: (test expression). Tests are evaluated left-to-right and must return exact PredicateBit. PredicateBit 1 selects and evaluates that clause expression; PredicateBit 0 skips it. If no clause selects, COND returns structural (). Structural () is not a predicate answer. Historical Function8 00000111 is compatibility projection only.")
      (core-profile-law
       . "Core1/Core2/Core3/Core4 are execution/research profiles over shared admitted domain identities and laws. A profile may select mechanisms but may not mint, renumber, or override the shared D1-D8 semantic domains or the D1/D3 predicate-control foundation.")
      (kernel-archipelago
       . "Execution kernels may own native mechanisms and observations. They consume an already-selected domain-qualified semantic object or an explicitly compatibility-tagged legacy projection plus arguments/context. Kernel names, opcodes, packed bytes and native types never acquire SENS semantic identity by themselves.")
      (reader-apostrophe
       . "At expression start, apostrophe is reader sugar for the already-admitted Core.D3 001 QUOTE identity. It must not create an intermediate human or legacy eight-bit semantic identity. Inside an identifier, apostrophe remains an ordinary Unicode character.")
      (reader-exact-width
       . "Canonical binary reading must preserve exact word width and payload. Source width may be evidence for an exact carrier only where the source-domain bridge explicitly admits it; width alone does not select semantic meaning. Historical exact-eight-bit source remains a bounded compatibility path during migration.")
      (reader-decimal-separator
       . "Dot and comma are equivalent decimal separators only for otherwise valid finite decimal/base-10 scientific numeric input. Numeric projection never creates Core domain identity.")
      (error-classification
       . "Named error categories remain observable where separately admitted, but UnsatisfiedConditional is not the exhaustion law of Core.D3 110 COND. Any remaining three-part COND or alternate exhaustion behavior is migration/history debt, not alternate current law.")
      (structural-empty-non-alias
       . "Core.D3 000 structural empty is not historical exact-eight-bit 00000000 and is not PredicateBit 0 or Number zero. Equal packed numeric zero across domains never collapses those identities.")
      (migration-direction
       . "New canonical code must move from legacy flat Sens8/Sid8 authority toward exact domain-qualified identity. New dependencies on legacy identity are permitted only inside explicitly named compatibility, transport, backend, archive or provenance boundaries."))))