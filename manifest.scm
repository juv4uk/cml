;; guix shell -m manifest.scm
;; Toolchain for cml: AOT compiler from my-lisp source to fpga-lisp's ISA.
(specifications->manifest
 '("rust"
   "rust:cargo"
   "git"
   "make"))
