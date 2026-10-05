; macros.lisp — a from-scratch, .lisp-hosted reimplementation of cml's
; macros.rs defmacro-expansion algorithm (a compile-time-only source
; transform, never reaching fpga-lisp -- see compatibility.my's `defmacro`
; entry). Same status as fpga-lisp's assembler.my relative to
; assembler.py (docs/tooling-language-priority.md): a parallel
; implementation proven correct by differential testing, NOT wired into
; cml's actual compile pipeline (that would need a subprocess/embedded-
; interpreter architecture decision out of scope for this step).
;
; The macro table and the bindings environment are both plain alists:
; ((key . value) (key . value) ...), walked with the same recursive
; lookup shape throughout, matching this codebase's own idioms
; elsewhere (cml_lookup in fpga-lisp assembly is the same pattern).
;
; Status (2026-08-12): differentially verified against macros.rs for two
; representative fixtures via the real my-lisp CLI (not the Rust
; reference's own AST shapes -- real my-lisp forms, since this runs
; inside actual my-lisp, not cml):
;   (defmacro my-list items (cons 'quote (cons items '())))
;     (my-list 1 2 3) -> (quote (1 2 3))  -- matches compatibility.my's
;     own documented example and cml's compiled output for the same
;     source.
;   (defmacro my-if (test then else) (cons 'cond (cons (cons test (cons then ()))
;     (cons (cons t (cons else ())) ()))))
;     (my-if x 1 2) -> (cond (x 1) (t 2))
; Two real bugs found+fixed while verifying against actual my-lisp
; rather than by inspection: `eq` requires both operands to be atoms
; (this codebase's own eval-macro-body originally compared a possibly-
; list `expr` against `nil`/`t` before checking `atom` first), and
; my-lisp's own truth symbol is lowercase `t`, not the uppercase `T`
; macros.rs uses (cml's Rust parser uppercases every identifier as its
; own convention; real my-lisp is case-sensitive and never does that).
; Not exhaustively tested -- same maturity level as fpga-lisp's
; assembler.my, a proven-real artifact, not a guarantee of full parity.

; --- explicit bridge from current SENS predicate domains to the CML
; macro meta-language. CML's historical `atom` means "not a pair", including
; structural (), while current SENS keeps structural empty distinct from the
; atom/pair predicate. Keep that distinction explicit instead of coercing ().
(def macro-empty?
  (lambda (value)
    (equal? value (quote ()))))

; Current SENS COND consumes exact D1/D3 control, while the historical CML
; macro meta-language still observes T/() as data. Keep that bridge local:
; these helpers manufacture only internal control values, never macro output.
(def cml-macro-control-yes
  (lambda ()
    (eq? (quote cml-macro-control) (quote cml-macro-control))))

(def cml-macro-control-no
  (lambda ()
    (eq? (quote cml-macro-left) (quote cml-macro-right))))

(def cml-macro-value-truthy?
  (lambda (value)
    (cond
      ((macro-empty? value) (cml-macro-control-no))
      ((cml-macro-control-yes) (cml-macro-control-yes)))))

(def macro-atom?
  (lambda (value)
    (cond
      ((macro-empty? value) (cml-macro-control-yes))
      ((atom? value) (cml-macro-control-yes))
      ((cml-macro-control-yes) (cml-macro-control-no)))))

; --- alist lookup, shared shape for both the macro table and bindings ---

(def alist-get
  (lambda (alist key)
    (cond
      ((macro-atom? alist) ())
      ((eq? (car (car alist)) key) (cdr (car alist)))
      ((cml-macro-control-yes) (alist-get (cdr alist) key)))))

; --- bind-params: params is a bare symbol, a proper list, or a dotted
; list, mirroring macros.rs's three Expr shapes for a defmacro's param
; spec. One recursive walk covers all three: a bare symbol (or a dotted
; list's tail symbol) is caught by the "params is a non-nil atom" case,
; binding it to whatever of `args` is left at that point in the walk. ---

(def bind-params
  (lambda (params args)
    (cond
      ((macro-atom? params)
       (cond
         ((macro-empty? params) ())
         ((cml-macro-control-yes) (cons (cons params args) ())))
      ((macro-atom? args) ())
      ((cml-macro-control-yes)
       (cons (cons (car params) (car args))
             (bind-params (cdr params) (cdr args)))))))

; --- eval-macro-body: the restricted meta-evaluator (quote/cons/car/cdr/
; atom/eq/cond only -- compatibility.my's `meta-evaluator-primitives`),
; over unevaluated call-site ASTs bound in `cml-macro-bindings`. ---

(def eval-macro-body
  (lambda (expr cml-macro-bindings)
    (cond
      ((macro-atom? expr)
       (cond
         ((macro-empty? expr) ())
         ((eq? expr (quote nil)) ())
         ((eq? expr (quote t)) (quote t))
         ((cml-macro-control-yes) (alist-get cml-macro-bindings expr))))
      ((cml-macro-control-yes) (eval-macro-form expr cml-macro-bindings)))))

(def eval-macro-form
  (lambda (expr cml-macro-bindings)
    (cond
      ((eq? (car expr) (quote quote)) (car (cdr expr)))
      ((eq? (car expr) (quote cons))
       (cons (eval-macro-body (car (cdr expr)) cml-macro-bindings)
             (eval-macro-body (car (cdr (cdr expr))) cml-macro-bindings)))
      ((eq? (car expr) (quote car)) (car (eval-macro-body (car (cdr expr)) cml-macro-bindings)))
      ((eq? (car expr) (quote cdr)) (cdr (eval-macro-body (car (cdr expr)) cml-macro-bindings)))
      ((eq? (car expr) (quote atom)) (truthy (macro-atom? (eval-macro-body (car (cdr expr)) cml-macro-bindings))))
      ((eq? (car expr) (quote eq))
       (truthy (equal? (eval-macro-body (car (cdr expr)) cml-macro-bindings)
                        (eval-macro-body (car (cdr (cdr expr))) cml-macro-bindings))))
      ((eq? (car expr) (quote cond)) (eval-macro-cond (cdr expr) cml-macro-bindings))
      ((cml-macro-control-yes) ()))))

(def truthy
  (lambda (v)
    (cond
      (v (quote t))
      ((cml-macro-control-yes) ()))))

(def eval-macro-cond
  (lambda (branches cml-macro-bindings)
    (cond
      ((macro-atom? branches) ())
      ((cml-macro-control-yes)
       (cond
         ((cml-macro-value-truthy?
            (eval-macro-body (car (car branches)) cml-macro-bindings))
          (eval-macro-body (car (cdr (car branches))) cml-macro-bindings))
         ((cml-macro-control-yes)
          (eval-macro-cond (cdr branches) cml-macro-bindings)))))))

; --- defmacro recognition and top-level expansion pass ---

(def defmacro-form?
  (lambda (expr)
    (cond
      ((macro-atom? expr) (cml-macro-control-no))
      ((eq? (car expr) (quote defmacro)) (cml-macro-control-yes))
      ((cml-macro-control-yes) (cml-macro-control-no)))))

(def defmacro-name (lambda (expr) (car (cdr expr))))
(def defmacro-params (lambda (expr) (car (cdr (cdr expr)))))
(def defmacro-body (lambda (expr) (car (cdr (cdr (cdr expr))))))

(def expand
  (lambda (expr table)
    (cond
      ((macro-atom? expr) expr)
      ((macro-atom? (car expr))
       (cond
         ((macro-empty? (car expr)) (expand-list expr table))
         ((eq? (car expr) (quote quote)) expr)
         ((cml-macro-control-yes) (expand-call expr table))))
      ((cml-macro-control-yes) (expand-list expr table)))))

(def expand-call
  (lambda (expr table)
    (cond
      ((macro-atom? (car expr)) (expand-with-macro-check expr table))
      ((cml-macro-control-yes) (expand-list expr table)))))

(def expand-with-macro-check
  (lambda (expr table)
    (cond
      ((cml-macro-value-truthy? (alist-get table (car expr)))
       (expand (eval-macro-body (macro-entry-body (alist-get table (car expr)))
                                (bind-params (macro-entry-params (alist-get table (car expr)))
                                             (cdr expr)))
               table))
      ((cml-macro-control-yes) (expand-list expr table)))))

; macro table entries store (params . body); helpers to read them back
; out of that pair shape -- named distinctly from defmacro-params/-body
; (which read straight off a (defmacro name params body) form) since
; `def` has no notion of overloading and a name collision would silently
; shadow the earlier definition instead of erroring.
(def macro-entry-params (lambda (entry) (car entry)))
(def macro-entry-body (lambda (entry) (cdr entry)))

(def expand-list
  (lambda (expr table)
    (cond
      ((macro-atom? expr) expr)
      ((cml-macro-control-yes)
       (cons (expand (car expr) table) (expand-list (cdr expr) table))))))

; --- process: one sequential staging walk, matching the live Rust
; MacroExpander::process exactly. A defmacro becomes visible only after its
; top-level definition has been encountered; later definitions never rewrite
; earlier source retroactively. Nested expansion of an already-visible macro
; still recurses through `expand` above. ---

(def add-macro-definition
  (lambda (expr table)
    (cons (cons (defmacro-name expr)
                (cons (defmacro-params expr) (defmacro-body expr)))
          table)))

(def expand-program-with
  (lambda (exprs table)
    (cond
      ((macro-atom? exprs) ())
      ((defmacro-form? (car exprs))
       (expand-program-with
         (cdr exprs)
         (add-macro-definition (car exprs) table)))
      ((cml-macro-control-yes)
       (cons (expand (car exprs) table)
             (expand-program-with (cdr exprs) table))))))

(def expand-program
  (lambda (exprs)
    (expand-program-with exprs ())))
