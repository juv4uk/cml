;; guix shell -m manifest.scm
;; Toolchain for cml: AOT compiler from my-lisp source to fpga-lisp's ISA,
;; plus a C toolchain for src/c_backend.rs's second, non-fpga-lisp target
;; (docs/heterogeneous-backends.md).
(specifications->manifest
 '("rust"
   "rust:cargo"
   "git"
   "make"
   "gcc-toolchain"))
