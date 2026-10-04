; language-contract.lisp — current machine-readable Level 1/2 contract.
;
; Contract 11.0 — domain-qualified identity authority.
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

((major . #d11) (minor . 0)
 (status . current-domain-qualified-authority)
 (supersedes . "Contract 10.0 flat Function8 identity authority")
 (historical-snapshot . "docs/archive/historical/language-contract-10.0.lisp")
 (note . "Contract 11.0 implements #2490/#2817/#2822. Canonical semantic identity is an exact binary object in an exact domain under a proved or ratified law. Contract 10 observable PredicateBit/ATOM/EQ/COND behavior is preserved; the flat 256-slot Function8/Sens8 ontology is now compatibility/history only.")
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
       . "Core.D3 bija3 is the exact three-bit foundation: 000 structural empty (), 001 QUOTE, 010 ATOM, 011 COND, 100 CONS, 101 CAR, 110 CDR, 111 EQ. Human role names are documentation projections. Historical exact-eight-bit forms are role-aware compatibility projections only.")
      (d4-bootstrap
       . "Core.D4 is the exact four-bit bootstrap domain ratified by #2169. Its resident coordinates execute only under D4 law; 0101 and 1001 remain unallocated. No historical Function8 identity may be reconstructed from a D4 word by bit-shape coincidence.")
      (d5-d6-residency
       . "Core.D5 and Core.D6 use exact five-bit and six-bit typed domains governed by their owner-ratified maps/laws. Residency, derivability, callability and runtime implementation are distinct facts: carrier existence alone grants neither occupancy nor callability.")
      (d7-sound-local-ordinal
       . "Core.D7 is the exact seven-bit Sound7 / Sanskrit sound-related domain with local sloka/sutra ordinal coordinates where admitted. D7 is not general arithmetic Number, and seven-bit width never grants selector geometry or callable Core-operation identity. Occupancy remains law/witness-specific.")
      (d8-exact-core-domain
       . "Core.D8 is the ratified exact eight-bit Core domain. Core.D8 identity is domain-qualified and is never historical Sens8/Sid8/Function8 merely because both use eight physical bits. D8 occupancy and callability remain separately law/witness-governed; admitted selector descendants may execute from the selector root+suffix law without legacy-byte authority.")
      (cross-domain-non-collapse
       . "The same packed numeric payload may coexist in D1, D2, D3, D4, D5, D6, D7, D8 or Core-Math domains without semantic equality. Cross-domain reuse requires an explicit independently proved bridge law.")
      (atom-one-bit-core1-4
       . "Core.D3 010 ATOM has one law across Core1/Core2/Core3/Core4: structural empty () and every admitted non-pair value answer PredicateBit 1; pair answers PredicateBit 0. Structural () is an ATOM-yes subject, not a truth value. Historical Function8 00000010 is compatibility projection only.")
      (eq-one-bit-core1-4
       . "Core.D3 111 EQ has one law across Core1/Core2/Core3/Core4: the same admitted atom answers PredicateBit 1; distinct admitted atoms answer PredicateBit 0; pair/out-of-domain input raises the named domain/type failure. EQ is not deep structural equality. Historical Function8 00000011 is compatibility projection only.")
      (cond-two-part-core1-4
       . "Core.D3 011 COND has one law across Core1/Core2/Core3/Core4. Every clause has exactly two fields: (test expression). Tests are evaluated left-to-right and must return exact PredicateBit. PredicateBit 1 selects and evaluates that clause expression; PredicateBit 0 skips it. If no clause selects, COND returns structural (). Structural () is not a predicate answer. Historical Function8 00000111 is compatibility projection only.")
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
       . "Named error categories remain observable where separately admitted, but UnsatisfiedConditional is not the exhaustion law of Core.D3 011 COND. Any remaining three-part COND or alternate exhaustion behavior is migration/history debt, not alternate current law.")
      (structural-empty-non-alias
       . "Core.D3 000 structural empty is not historical exact-eight-bit 00000000 and is not PredicateBit 0 or Number zero. Equal packed numeric zero across domains never collapses those identities.")
      (migration-direction
       . "New canonical code must move from legacy flat Sens8/Sid8 authority toward exact domain-qualified identity. New dependencies on legacy identity are permitted only inside explicitly named compatibility, transport, backend, archive or provenance boundaries."))))