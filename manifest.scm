;; guix shell -m manifest.scm
;; Toolchain for cml: AOT compiler from my-lisp source to fpga-lisp's ISA,
;; plus a C toolchain for src/c_backend.rs's second, non-fpga-lisp target
;; (docs/heterogeneous-backends.md).
;;
;; iverilog and python are here because compiler_test.rs/conformance_test.rs
;; actually shell out to them (assembler.py, then real iverilog simulation
;; against fpga-lisp's RTL) -- previously only working because the ambient
;; shared Guix profile happened to provide them, not because this repo's
;; own manifest declared them (CML-MANIFEST-COMPLETE). `guix shell --pure`
;; is what actually catches that kind of accidental-dependency drift; a
;; bare `guix shell` wouldn't have.
(specifications->manifest
 '("rust"
   "rust:cargo"
   "git"
   "make"
   "gcc-toolchain"
   "iverilog"
   "python"))
