; CML real-upstream workload manifest v1.
; Реєстр реальних upstream-навантажень CML v1.
;
; This file transports experiment/provenance only. It MUST NOT contain an
; `expected`, `expected-outcome`, or equivalent CML-authored semantic answer.
; The semantic verdict remains at the cited upstream witness/verdict source.
;
; `upstream-channel` follows cml#84:
;   observed-current = research/drift/performance intake from today's upstream;
;   supported-pin    = the frozen compatibility revision CML claims to support.
;
; Один top-level alist = один workload. Нові записи додаються без зміни старих.

((kind . cml-upstream-workload)
 (format-version . 1)
 (id . my-lisp-meta-registry-generator-317)
 (status . active)
 (upstream-channel . observed-current)
 (upstream-repo . "juv4uk/my-lisp")
 (upstream-pr . 317)
 (upstream-ref . "ed1ef28928df1db0cc83f17ae993527635b6256d")
 (upstream-issue . 76)
 (performance-ledger . "juv4uk/my-lisp#74")
 (semantic-authority . "lib/surface/semantic-registry.lisp")
 (verdict-source . "PR #317 exact projection parity gate")
 (command . "./target/debug/my-lisp scripts/generate-meta-semantic-registry.lisp --check")
 (evidence .
   ((runner-class . github-hosted-ubuntu-24.04)
    (rustc . "1.98.1")
    (workflow-run . 35253773341)
    (job . 105312238020)
    (lisp-total-ns-observed . 169711000000)
    (registry-read-ns . 106330159297)
    (collect-ns . 10320115743)
    (render-ns . 517360359)
    (script-total-to-render-ns . 117167710490)))
 (notes . "Hosted-runner evidence only; not pinned i5-6400 evidence. Python comparison is recorded upstream in my-lisp#74."))

((kind . cml-upstream-workload)
 (format-version . 1)
 (id . my-lisp-cli-cold-bootstrap-332)
 (status . profiling)
 (upstream-channel . observed-current)
 (upstream-repo . "juv4uk/my-lisp")
 (upstream-issue . 332)
 (upstream-ref . "d299bd99b08f34d5297081a52f5baffb1efa44c7")
 (performance-ledger . "juv4uk/my-lisp#74")
 (semantic-authority . "language-owned macro/core/time/utf8/process/tcp/fs sources")
 (verdict-source . "my-lisp bootstrap/semantic witnesses; exact workload descriptor pending my-lisp#334")
 (compiler-child . "juv4uk/cml#78")
 (evidence .
   ((runner-class . github-hosted-ubuntu-24.04)
    (observed-pre-script-prefix . approximate-52-seconds)
    (duplicate-load-observation . "load_process_library and load_fs_library both evaluate UTF8_LIBRARY_SOURCE")))
 (notes . "Direct Rust helper/API growth is forbidden by my-lisp#299 one-way subtraction valve; mechanism investigation therefore belongs in cml#78 and returns upstream through #332."))

((kind . cml-upstream-workload)
 (format-version . 1)
 (id . my-lisp-text-pipeline-333)
 (status . profiling)
 (upstream-channel . observed-current)
 (upstream-repo . "juv4uk/my-lisp")
 (upstream-issue . 333)
 (upstream-ref . "d299bd99b08f34d5297081a52f5baffb1efa44c7")
 (performance-ledger . "juv4uk/my-lisp#74")
 (semantic-authority . "lib/utf8.lisp + lib/fs.lisp")
 (verdict-source . "my-lisp UTF-8 valid/invalid witnesses; exact workload descriptor pending my-lisp#334")
 (compiler-child . "juv4uk/cml#79")
 (rejected-candidate .
   ((source-pr . "juv4uk/my-lisp#326")
    (workflow-run . 35255907399)
    (base-read-file-ns . 87330448104)
    (candidate-read-file-ns . 93870210027)
    (verdict . slower)))
 (notes . "Balanced pairwise Unicode string materialization was ~7.5% slower on the real same-runner read-file workload; do not treat final string balancing as the primary bottleneck."))
