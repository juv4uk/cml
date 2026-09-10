; Cross-repo task recommendations for the Cyberpunk 2077 / my-lisp embedding track.
; Author: cml session 2026-09-10. Status: recommendations only — not claims of ownership.
; Owner may accept, reject, or re-prioritize. Agents: claim only tasks in YOUR repo.
;
; Principle: my-lisp = sole language meaning; wsm-my-lisp = in-process host;
; cml = offline compiler; my-lisp-cyberpunk = product surface (currently empty);
; wsm-target-contract = ABI authority for tags/words.

((kind . cross-repo-task-recommendations)
 (date . 2026-09-10)
 (theme . cyberpunk-2077-my-lisp-embedding)

 (owner-decisions-needed .
  ((Q1 . "First mod-script semantics: data-primitives+fixed-dispatch vs closures+callbacks?")
   (Q2 . "Where does mylisp-cml-export.wsm live in my-lisp tree?")
   (Q3 . "Is my-lisp-cyberpunk the product repo, or a redirect to wsm-my-lisp/plugin?")))

 (by-repo .
  ((my-lisp-cyberpunk .
    ((priority-order .
      (("CP-OWNER-SCOPE-V0" .
        ((priority . 10.0)
         (who . owner)
         (description . "Answer README open question: v0 needs only fixed host-dispatch (cond+def) or also lambda/apply callbacks. Until answered, agents must not invent product architecture.")))
       ("CP-REPO-ROLE" .
        ((priority . 9.5)
         (who . owner)
         (description . "Decide: this repo hosts RED4ext+UI product code, or stays thin and points at wsm-my-lisp/plugin. Document the choice in README.")))
       ("CP-ONE-SHOT-SCRIPT" .
        ((priority . 9.0)
         (depends-on . (CP-OWNER-SCOPE-V0))
         (description . "After scope: one checked-in .my script (e.g. дай-зброю / телепортуй) that matches my-lisp cyberpunk fixtures and is the only v0 demo.")))
       ("CP-NO-SILENT-CLOSURES" .
        ((priority . 8.5)
         (description . "Policy task: refuse fake closures for engine callbacks until a real embedding decision; prefer explicit 'not in v0'."))))))

   (wsm-my-lisp .
    ((priority-order .
      (("CP-IN-GAME-LOAD-WITNESS" .
        ((priority . 9.8)
         (description . "Load plugin+DLL inside a real Cyberpunk 2077 process; log wsm_session_init success via RED4ext Logger. Today only MSVC build is proven, not in-game load.")
         (acceptance . "Screenshot or log from live game process; failure modes named.")))
       ("CP-HOST-PRIM-ONE" .
        ((priority . 9.5)
         (depends-on . (CP-IN-GAME-LOAD-WITNESS))
         (description . "Wire exactly one real game-facing host primitive (e.g. stub teleport or log-only) end-to-end: Lisp call -> FFI -> RED4ext API or explicit Unsupported.")
         (acceptance . "Fixture row: source | oracle | in-process | in-game | status | reason.")))
       ("CP-FIXTURE-MATRIX" .
        ((priority . 9.2)
         (description . "Automate my-lisp docs/cyberpunk-host-dispatch-fixtures.md against dll eval; fail closed on text/arity drift (UnknownSymbol exact string already matched).")))
       ("CP-TAG-BOXED-CONSUME" .
        ((priority . 8.8)
         (depends-on . (WSM-CONTRACT-TAG-BOXED))
         (description . "After wsm-target-contract ratifies TAG_BOXED, remove TENTATIVE labels and pin tag value in one place.")))
       ("CP-ARENA-THREAD-POLICY" .
        ((priority . 8.0)
         (description . "Document and enforce single-threaded arena for embed: either documented limitation or per-session arena. Do not claim thread-safe.")))
       ("CP-NO-EVAL-IN-CML" .
        ((priority . 7.5)
         (description . "Keep wsm_eval_string ownership here; never push runtime reader into cml C backend."))))))

   (my-lisp .
    ((priority-order .
      (("CP-EXPORT-ARTIFACT-PIN" .
        ((priority . 9.5)
         (description . "Check in deterministic mylisp-cml-export.wsm (or agreed path) from cml-export binary so cml can hard-pin FNV digest.")
         (acceptance . "Byte-identical file in tree; cml consumer switches off pending-producer-byte-pin.")))
       ("CP-FIXTURES-OWNED" .
        ((priority . 9.0)
         (description . "Keep cyberpunk-host-dispatch-fixtures.md as oracle; any host change must update fixtures first, not after.")))
       ("CP-MULTI-HOST-CAPABILITY-MODEL" .
        ((priority . 8.5)
         (description . "Design note + optional code: how capability installation differs for CLI (blocking), WASM, and game (frame/callback). Do not implement second host set until written.")))
       ("CP-ORACLE-IN-PROCESS-GAP" .
        ((priority . 8.0)
         (description . "Name the gap: CI oracle ≠ shipped in-game evaluator. Propose offline hash of evaluator artifact or shared fixture runner for wsm-my-lisp dll.")))
       ("CP-SLICE2-ON-REQUEST" .
        ((priority . 6.0)
         (description . "Do not expand semantic export slice-2 until cml asks with a concrete program shape."))))))

   (cml .
    ((priority-order .
      (("CP-DISPATCH-CORPUS" .
        ((priority . 9.0)
         (description . "Add cyberpunk dispatch fixtures (uk identifiers) to conformance matrix / triple-oracle as compiled-C rows.")
         (acceptance . "teleport/give-weapon style scripts compile and match expected values.")))
       ("CP-STRINGS-C-PATH" .
        ((priority . 8.8)
         (description . "Land TAG_STRING/TAG_BOXED on C backend if contract allows; complete COMPILER-03 strings acceptance.")))
       ("CP-EXPORT-HARD-PIN" .
        ((priority . 8.5)
         (depends-on . (CP-EXPORT-ARTIFACT-PIN))
         (description . "Replace pending-producer-byte-pin with real FNV digest from my-lisp artifact.")))
       ("CP-NO-WSM-EVAL-STRING" .
        ((priority . 9.5)
         (description . "Standing non-goal: do not implement runtime eval_string in cml. Document in COMPILER-10 if host ABI work starts.")))
       ("COMPILER-09-SKIP-UNTIL-EVIDENCE" .
        ((priority . 5.0)
         (description . "Do not start GC until COMPILER-08 stress proves need; for CP path prefer bounded arena."))))))

   (wsm-target-contract .
    ((priority-order .
      (("WSM-CONTRACT-TAG-BOXED" .
        ((priority . 9.7)
         (description . "Ratify or reject TAG_BOXED=7 (generic boxed/ref; String as first payload kind). Record in target-contract.wsm with version bump.")
         (acceptance . "Consumers (wsm-my-lisp, cml x86 path) have a single authoritative tag number.")))
       ("WSM-CONTRACT-CYBERPUNK-NOTE" .
        ((priority . 7.0)
         (description . "One paragraph in README: CP embed may use same word ABI; no second tag space for game."))))))

   (wsm-os / wsm-os-lisp .
    ((note . "No CP-specific work required for v0. Stay on nucleus/self-host track unless owner redirects.")))

   ))

 (recommended-sequence .
  (1 . "Owner answers CP-OWNER-SCOPE-V0 + CP-REPO-ROLE")
  (2 . "wsm-target-contract: WSM-CONTRACT-TAG-BOXED")
  (3 . "wsm-my-lisp: CP-IN-GAME-LOAD-WITNESS")
  (4 . "wsm-my-lisp: CP-HOST-PRIM-ONE + CP-FIXTURE-MATRIX")
  (5 . "my-lisp: CP-EXPORT-ARTIFACT-PIN")
  (6 . "cml: CP-EXPORT-HARD-PIN + CP-DISPATCH-CORPUS + strings")
  (7 . "Only then: callbacks/closures product discussion"))
)
