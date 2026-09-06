; compiler-tasks.my — execution queue for the native my-lisp compiler.
; This file is authoritative for the standalone compiler track inside cml.
; `done . t` requires executable in-repo evidence. Unsupported is never pass.
;
; The compiler must be judged against language semantics, not against its own
; implementation. Native Rust eval is the reference today; Lisp my-eval joins
; the oracle wherever its admitted surface is proven.

((kind . compiler-tasks)
 (version . 1)
 (goal . "Compile the declared my-lisp compiler-v1 surface to standalone native executables and prove semantic parity for every admitted fixture.")
 (tasks .
  (("COMPILER-00-TRIPLE-ORACLE" .
    ((priority . 1.00)
     (done . nil)
     (capabilities . (compiler testing lisp rust proof))
     (depends-on . ())
     (description . "Build one compiler harness that runs identical source through native my-lisp, Lisp my-eval when supported, and the compiled executable; classify value/error/unsupported structurally.")
     (acceptance . "At least 10 existing constitutive fixtures are compared automatically; every mismatch identifies the diverging observer; unsupported is explicit; no stdout prose is the semantic discriminator.")))
   ("COMPILER-01-BUILD-COMMAND" .
    ((priority . 0.98)
     (done . nil)
     (capabilities . (compiler cli c toolchain testing))
     (depends-on . ("COMPILER-00-TRIPLE-ORACLE"))
     (description . "Provide one compiler command for source -> IR -> generated C -> target runtime -> system C compiler -> executable.")
     (acceptance . "A command equivalent to `cml build program.my -o program` emits a runnable Linux x86_64 executable; generated C can be retained for audit; toolchain failure is explicit.")))
   ("COMPILER-02-RUNTIME-ABI-V0" .
    ((priority . 0.96)
     (done . nil)
     (capabilities . (compiler c abi runtime design testing))
     (depends-on . ("COMPILER-01-BUILD-COMMAND"))
     (description . "Extract a small stable runtime ABI from c_backend.rs: Value ABI, constructors, predicates, call convention, entry point, allocation interface and structured failure channel.")
     (acceptance . "Generated program and runtime build as separate translation units; generated code depends only on the declared ABI, not emitter-private structs.")))
   ("COMPILER-03-CORE-VALUES" .
    ((priority . 0.94)
     (done . nil)
     (capabilities . (compiler runtime representation conformance))
     (depends-on . ("COMPILER-02-RUNTIME-ABI-V0"))
     (description . "Close compiled representation for (), symbols, proper/dotted pairs, strings, exact integers and exact rationals. Treat inexact representation as a separate explicit decision.")
     (acceptance . "Every admitted value fixture round-trips to the same canonical observation as native my-lisp; no truncation, lossy conversion or hidden fallback.")))
   ("COMPILER-04-CLOSURE-BINDING-PARITY" .
    ((priority . 0.94)
     (done . nil)
     (capabilities . (compiler closures lisp conformance))
     (depends-on . ("COMPILER-02-RUNTIME-ABI-V0"))
     (description . "Prove fixed, dotted and all-rest lambdas, lexical capture, first-class builtins, self-recursion and then mutual recursion in compiled programs.")
     (acceptance . "Each binding mode has differential fixtures against native semantics; arity mismatch is explicit and never truncated.")))
   ("COMPILER-05-STRUCTURED-ERRORS" .
    ((priority . 0.92)
     (done . nil)
     (capabilities . (compiler errors runtime conformance))
     (depends-on . ("COMPILER-02-RUNTIME-ABI-V0"))
     (description . "Give compiled programs stable machine-readable failure identity for arity, unknown-symbol, type, not-callable, division-by-zero and unsupported target behavior.")
     (acceptance . "The oracle harness compares failure identities as data; libc, OS and compiler diagnostic prose is optional detail only.")))
   ("COMPILER-06-MACRO-PIPELINE" .
    ((priority . 0.90)
     (done . nil)
     (capabilities . (compiler lisp macros bootstrap differential-testing))
     (depends-on . ("COMPILER-00-TRIPLE-ORACLE"))
     (description . "Make macro expansion an explicit compiler stage and drive the proven Lisp implementation toward authority, keeping the Rust path as oracle until cutover evidence is sufficient.")
     (acceptance . "Representative and constitutive macro fixtures expand equivalently through both implementations and their compiled programs match native execution.")))
   ("COMPILER-07-LANGUAGE-LIBRARIES" .
    ((priority . 0.89)
     (done . nil)
     (capabilities . (compiler lisp bootstrap libraries testing))
     (depends-on . ("COMPILER-04-CLOSURE-BINDING-PARITY" "COMPILER-06-MACRO-PIPELINE"))
     (description . "Compile real language-owned bootstrap/core libraries through the same source->IR pipeline instead of duplicating their semantics as C runtime helpers.")
     (acceptance . "A nontrivial real slice of core.my is linked into a standalone program and passes its oracle fixtures without backend-specific semantic reimplementation.")))
   ("COMPILER-08-BOUNDED-MEMORY" .
    ((priority . 0.84)
     (done . nil)
     (capabilities . (runtime memory compiler testing))
     (depends-on . ("COMPILER-03-CORE-VALUES"))
     (description . "Define deterministic allocation ownership for bounded standalone programs before introducing GC; arena/region allocation is acceptable when limits and failure are explicit.")
     (acceptance . "Stress fixtures allocate many pairs and closures without UB; exhaustion is a named failure; ownership/lifetime rules are documented and tested.")))
   ("COMPILER-09-GC-EVIDENCE-GATE" .
    ((priority . 0.68)
     (done . nil)
     (capabilities . (runtime gc proof testing))
     (depends-on . ("COMPILER-08-BOUNDED-MEMORY"))
     (description . "Add GC only if real compiler-v1 workloads prove bounded allocation insufficient. Define roots before selecting collector complexity.")
     (acceptance . "Either evidence records GC as unnecessary for compiler-v1, or a stress corpus proves bounded live-memory behavior with no lost reachable objects.")))
   ("COMPILER-10-HOST-CAPABILITY-ABI" .
    ((priority . 0.82)
     (done . nil)
     (capabilities . (compiler host abi portability runtime))
     (depends-on . ("COMPILER-02-RUNTIME-ABI-V0"))
     (description . "Map the minimal my-lisp host substrate into compiled executables and link only capabilities actually required by the program.")
     (acceptance . "At least one raw file/time/process/TCP capability works end-to-end while interpretation remains Lisp-owned; absent capabilities fail explicitly.")))
   ("COMPILER-11-CONFORMANCE-MATRIX" .
    ((priority . 1.00)
     (done . nil)
     (capabilities . (compiler conformance proof automation))
     (depends-on . ("COMPILER-00-TRIPLE-ORACLE"))
     (description . "Generate and maintain fixture | native | my-eval | compiled | status | reason for every constitutive fixture.")
     (acceptance . "CI fails if a constitutive fixture disappears, is silently skipped or changes from pass to unsupported without an explicit reviewed reason.")))
   ("COMPILER-12-MUTUAL-RECURSION" .
    ((priority . 0.78)
     (done . nil)
     (capabilities . (compiler closures recursion lisp testing))
     (depends-on . ("COMPILER-04-CLOSURE-BINDING-PARITY"))
     (description . "Give top-level mutual recursion an explicit IR/runtime meaning instead of relying on emitter order or accidental backpatch behavior.")
     (acceptance . "even?/odd?-style programs compile and match native semantics; forward-reference rules are documented and tested.")))
   ("COMPILER-13-SECOND-HOST" .
    ((priority . 0.72)
     (done . nil)
     (capabilities . (compiler portability windows linux c testing))
     (depends-on . ("COMPILER-10-HOST-CAPABILITY-ABI" "COMPILER-11-CONFORMANCE-MATRIX"))
     (description . "Compile the same admitted corpus on a second host/toolchain, preferably Windows/MinGW after Linux, without duplicating language semantics.")
     (acceptance . "The same source corpus produces the same structured observations on both hosts; differences are confined to target adapter/toolchain details.")))
   ("COMPILER-14-V1-RELEASE-GATE" .
    ((priority . 1.00)
     (done . nil)
     (capabilities . (compiler release conformance audit proof))
     (depends-on . ("COMPILER-03-CORE-VALUES" "COMPILER-04-CLOSURE-BINDING-PARITY" "COMPILER-05-STRUCTURED-ERRORS" "COMPILER-07-LANGUAGE-LIBRARIES" "COMPILER-11-CONFORMANCE-MATRIX"))
     (description . "Ratify the exact compiler-v1 standalone surface only from executable conformance evidence.")
     (acceptance . "All admitted fixtures pass; every remaining unsupported item is explicit and outside the declared surface; release wording is no stronger than the corpus proves.")))
   ("COMPILER-15-COMPILER-IN-LISP" .
    ((priority . 0.45)
     (done . nil)
     (capabilities . (compiler lisp self-hosting research differential-testing))
     (depends-on . ("COMPILER-07-LANGUAGE-LIBRARIES"))
     (description . "Move compiler stages themselves into my-lisp incrementally: begin with a pure transform such as normalization/lowering, prove differential equivalence, then widen only after evidence.")
     (acceptance . "At least one real compiler stage is implemented in Lisp and produces equivalent IR/output on an agreed corpus; Rust remains replaceable oracle rather than semantic owner."))))))
