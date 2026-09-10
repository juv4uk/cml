; CML-local recommendations derived from CROSS-REPO-TASK-RECOMMENDATIONS-2026-09-10.
; Parallel to compiler-tasks.my.

((kind . cml-cyberpunk-recommendations)
 (version . 2)
 (tasks .
  (("CP-DISPATCH-CORPUS" .
    ((priority . 9.0)
     (done . partial)
     (evidence . (tests/cyberpunk_dispatch_test.rs tests/conformance_matrix_test.rs))
     (commit . 10acff86)
     (capabilities . (compiler testing cyberpunk))
     (description . "Ukrainian/English host-dispatch fixtures through compiled-C path.")
     (acceptance . "teleport/give-weapon style programs compile; unknown event named; no silent skip.")
     (note . "Executable tests landed; full my-lisp fixture file automation still optional.")))
   ("CP-STRINGS-C-PATH" .
    ((priority . 8.8)
     (done . partial)
     (evidence . (tests/string_values_test.rs src/c_backend.rs))
     (description . "TAG_STRING on C path (CI applicator). Align with TAG_BOXED when contract ratifies.")))
   ("CP-EXPORT-HARD-PIN" .
    ((priority . 8.5)
     (done . nil)
     (description . "Hard-pin mylisp-cml-export.wsm FNV digest once my-lisp checks in producer output.")))
   ("CP-NO-WSM-EVAL-STRING" .
    ((priority . 9.5)
     (done . nil)
     (description . "Standing non-goal: never implement runtime reader/eval_string in cml backends."))))))
