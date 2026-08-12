use crate::ir::{Ir, Params, PrimOp, Quoted};

pub struct Compiler {
    output: Vec<String>,
    label_counter: usize,
    used_lookup: bool,
    used_equal: bool,
}

impl Compiler {
    pub fn new() -> Self {
        Compiler {
            output: Vec::new(),
            label_counter: 0,
            used_lookup: false,
            used_equal: false,
        }
    }

    fn next_label(&mut self, prefix: &str) -> String {
        self.label_counter += 1;
        format!("{}_{}", prefix, self.label_counter)
    }

    // --- Register preservation helpers (see docs/abi.md) -------------
    //
    // No register allocator exists here; every compile_* function
    // hardcodes its own scratch registers. These three helpers are the
    // only mechanism this codebase has for protecting a value across a
    // nested compile_* call that might clobber it as scratch.

    fn push(&mut self, reg: &str) {
        self.emit(&format!("CONS R11 {} R11", reg));
    }

    fn pop(&mut self, reg: &str) {
        self.emit(&format!("CAR {} R11", reg));
        self.emit("CDR R11 R11");
    }

    /// Runs `body`, having pushed `reg`'s current value onto R11 first and
    /// popped it back after -- protects `reg` across any compile_* call
    /// inside `body` that might use it as scratch (docs/abi.md's "one rule
    /// that matters"), regardless of that call's own target_reg.
    fn preserve_across(&mut self, reg: &str, body: impl FnOnce(&mut Self)) {
        self.push(reg);
        body(self);
        self.pop(reg);
    }

    /// Emits a protected `CALL R14 <label>` for a subroutine (cml_lookup,
    /// cml_equal) that itself returns via `RET R14` -- preserves the
    /// *caller's* pending return address (already sitting in R14) across
    /// the nested call, since `CALL` unconditionally overwrites R14 with
    /// its own return address on real hardware (see docs/abi.md). Without
    /// this, a subroutine call from inside a function body silently
    /// destroys that function's own eventual `RET R14` target.
    fn call_subroutine(&mut self, label: &str) {
        self.preserve_across("R14", |c| {
            let ret_label = c.next_label("call_ret");
            c.emit(&format!("LOADI R14 {}", ret_label));
            c.emit(&format!("CALL R14 {}", label));
            c.emit(&format!("{}:", ret_label));
        });
    }

    pub fn compile(&mut self, program: &[Ir]) -> String {
        // Initialize R4 (ENV) and R11 (Stack) to NIL at program start
        self.emit("LOADI R13 0");
        self.emit("LOADI R12 1");
        self.emit("EQ R4 R12 R13"); // R4 = NIL
        self.emit("MOV R11 R4"); // R11 = NIL

        for ir in program {
            self.compile_expr(ir, "R15");
        }
        self.emit("HALT");

        if self.used_lookup {
            self.emit_cml_lookup();
        }
        if self.used_equal {
            self.emit_cml_equal();
        }

        self.output.join("\n")
    }

    fn compile_expr(&mut self, ir: &Ir, target_reg: &str) {
        match ir {
            Ir::Int(n) => {
                self.emit_integer_literal(*n, target_reg);
            }
            Ir::Nil => {
                self.emit("LOADI R13 0");
                self.emit("LOADI R12 1");
                self.emit(&format!("EQ {} R12 R13", target_reg));
            }
            Ir::True => {
                self.emit(&format!("LOADI {} 0", target_reg));
                self.emit(&format!("ATOM {} {}", target_reg, target_reg));
            }
            Ir::Var(s) => {
                // Variable lookup
                self.used_lookup = true;
                self.emit(&format!("; LOOKUP {}", s));
                self.emit(&format!("LOADSYM R12 {}", s));
                self.emit("MOV R13 R4"); // R4 is our standard ENV register
                self.call_subroutine("cml_lookup");
                self.emit(&format!("MOV {} R15", target_reg));
            }
            Ir::Quote(q) => self.compile_quoted(q, target_reg),
            Ir::Lambda { params, body } => self.compile_lambda(params, body, target_reg),
            Ir::App { func, args } => self.compile_generic_call(func, args, target_reg),
            Ir::Cond { branches } => self.compile_cond(branches, target_reg),
            Ir::Let { bindings, body } => self.compile_let(bindings, body, target_reg),
            Ir::Def { name, value } => self.compile_def(name, value, target_reg),
            Ir::Prim { op, args } => self.compile_prim(*op, args, target_reg),
        }
    }

    fn compile_prim(&mut self, op: PrimOp, args: &[Ir], target_reg: &str) {
        match op {
            PrimOp::Cons => {
                // args[1] may itself be a two-arg primitive call, which
                // also hardcodes R1 as scratch -- preserve R1 across
                // evaluating args[1] so it can't clobber the
                // already-computed first operand (docs/abi.md).
                self.compile_expr(&args[0], "R1");
                self.preserve_across("R1", |c| c.compile_expr(&args[1], "R2"));
                self.emit(&format!("CONS {} R1 R2", target_reg));
            }
            PrimOp::Car => {
                self.compile_expr(&args[0], "R1");
                self.emit(&format!("CAR {} R1", target_reg));
            }
            PrimOp::Cdr => {
                self.compile_expr(&args[0], "R1");
                self.emit(&format!("CDR {} R1", target_reg));
            }
            PrimOp::Eq => {
                self.compile_expr(&args[0], "R1");
                self.preserve_across("R1", |c| c.compile_expr(&args[1], "R2"));
                self.emit(&format!("EQ {} R1 R2", target_reg));
            }
            PrimOp::Atom => {
                self.compile_expr(&args[0], "R1");
                self.emit(&format!("ATOM {} R1", target_reg));
            }
            PrimOp::EqualP => {
                self.compile_expr(&args[0], "R1");
                self.preserve_across("R1", |c| c.compile_expr(&args[1], "R2"));
                self.used_equal = true;
                // cml_equal returns via RET R14 like cml_lookup, so it
                // needs the same call_subroutine protection -- this used
                // to be a bare `CALL R14 cml_equal` with no R14 save,
                // silently destroying whatever function this equal? call
                // was nested inside's own eventual `RET R14` target.
                self.call_subroutine("cml_equal");
                self.emit(&format!("MOV {} R15", target_reg));
            }
            PrimOp::Add => {
                self.compile_expr(&args[0], "R1");
                self.preserve_across("R1", |c| c.compile_expr(&args[1], "R2"));
                self.emit(&format!("ADD {} R1 R2", target_reg));
            }
        }
    }

    fn compile_generic_call(&mut self, func: &Ir, args: &[Ir], target_reg: &str) {
        self.emit("; CALL START");
        // 1. Evaluate arguments (support up to 8 args mapped to R1..R3, R5..R9)
        let arg_regs = ["R1", "R2", "R3", "R5", "R6", "R7", "R8", "R9"];
        let bound_arg_count = args.len().min(arg_regs.len());

        // Push each argument onto the R11 stack immediately after computing
        // it, before compiling the next one. A primitive call used as an
        // argument expression (+, cdr, eq, cons, equal?, ...) always
        // hardcodes R1/R2/R3 as scratch, regardless of its own target_reg --
        // so without this, evaluating argument i+1 could silently clobber
        // argument i's already-computed value still sitting in arg_regs[i]
        // (e.g. `(f (cdr values) (+ acc 1))`: `(+ acc 1)` clobbers R1, which
        // `(cdr values)` had just written its result into).
        // Пушимо кожен аргумент на стек R11 одразу після обчислення, до
        // компіляції наступного -- інакше примітив-аргумент (напр. `+`)
        // тихо затирає R1..R3 попереднього вже обчисленого аргументу.
        for (i, arg) in args.iter().enumerate() {
            if i < arg_regs.len() {
                self.compile_expr(arg, arg_regs[i]);
                self.push(arg_regs[i]);
            }
        }

        // 2. Evaluate the closure expression (into R15) while every computed
        // argument sits safely on the stack -- this also protects them from
        // cml_lookup's own R0/R1/R2 scratch use when func_expr is a symbol.
        self.compile_expr(func, "R15");

        // 3. Pop the arguments back off, in reverse push order.
        for reg in arg_regs.iter().take(bound_arg_count).rev() {
            self.pop(reg);
        }

        // R0 carries the complete evaluated argument list. Fixed-arity
        // lambdas keep using the fast argument registers; dotted and bare
        // parameter lists take their rest value from this structural form.
        // R0 несе повний список аргументів для variadic lambda.
        // R0 traegt die vollstaendige Argumentliste fuer variadische Lambdas.
        self.emit("LOADI R13 0");
        self.emit("LOADI R12 1");
        self.emit("EQ R0 R12 R13"); // R0 = NIL
        for reg in arg_regs.iter().take(args.len()).rev() {
            self.emit(&format!("CONS R0 {} R0", reg));
        }

        // Save ENV (R4) and Link Register (R14), jump into the closure body
        // (RET, not CALL: the target is a runtime label pointer in a
        // register, and RET's hardware form doesn't auto-link R14 the way
        // CALL does -- see docs/abi.md), then restore both on return.
        self.push("R4");
        self.push("R14");

        // Closure is (LABEL_PTR . CAPTURED_ENV)
        let ret_label = self.next_label("call_ret");
        self.emit("CAR R10 R15"); // Extract LABEL_PTR to R10
        self.emit("CDR R4 R15");  // Extract CAPTURED_ENV to R4 (current ENV register)

        self.emit(&format!("LOADI R14 {}", ret_label)); // Return address
        self.emit("RET R10"); // Indirect jump to lambda body

        self.emit(&format!("{}:", ret_label));
        self.pop("R14");
        self.pop("R4");

        self.emit(&format!("MOV {} R15", target_reg)); // Lambda returns in R15 by convention
        self.emit("; CALL END");
    }

    // (def name value) binds `name` in the current environment (R4) via the
    // fpga-lisp letrec pattern proven in M28/M29: extend R4 with a
    // placeholder pair (name . NIL) *before* compiling `value`, so a lambda
    // captures the extended frame and can look itself up by name; then
    // SETCDR-backpatch the placeholder's cdr to the compiled value. This is
    // the only mutation permitted after CONS allocates a cell (see fpga-lisp
    // M26). Only self-recursion is supported this way -- two mutually
    // recursive defs where the first calls the second before the second is
    // defined would need two-pass forward declaration, not implemented.
    // (def name value) прив'язує `name` у поточному середовищі (R4) через
    // letrec-патерн fpga-lisp (M28/M29): розширюємо R4 placeholder-парою
    // ДО компіляції value, потім SETCDR-бекпатчимо її cdr на скомпільоване
    // значення.
    fn compile_def(&mut self, name: &str, value: &Ir, target_reg: &str) {
        self.emit("; DEF START");
        self.emit("LOADI R13 0");
        self.emit("LOADI R12 1");
        self.emit("EQ R9 R12 R13"); // R9 = NIL
        self.emit(&format!("LOADSYM R12 {}", name));
        self.emit("CONS R13 R12 R9"); // R13 = ph_pair = (NAME . NIL)
        self.emit("CONS R4 R13 R4"); // env = (ph_pair . env) -- captured by `value` if it's a lambda
        self.emit("CONS R11 R13 R11"); // save ph_pair pointer across compiling `value`

        self.compile_expr(value, target_reg);

        self.emit("CAR R13 R11"); // restore ph_pair pointer
        self.emit("CDR R11 R11");
        self.emit(&format!("SETCDR R12 R13 {}", target_reg)); // backpatch: ph_pair.cdr = value
        self.emit("; DEF END");
    }

    // `let` is a derived Lisp form, not an FPGA primitive. Lower
    // (let ((name value) ...) body) to ((lambda (name ...) body) value ...).
    // `let` — похідна форма Lisp, а не примітив FPGA.
    // `let` ist eine abgeleitete Lisp-Form, keine FPGA-Primitive.
    fn compile_let(&mut self, bindings: &[(String, Ir)], body: &Ir, target_reg: &str) {
        let params = Params::Fixed(bindings.iter().map(|(name, _)| name.clone()).collect());
        let values: Vec<Ir> = bindings.iter().map(|(_, value)| value.clone()).collect();
        let lambda = Ir::Lambda { params, body: Box::new(body.clone()) };
        self.compile_generic_call(&lambda, &values, target_reg);
    }

    fn compile_quoted(&mut self, q: &Quoted, target_reg: &str) {
        match q {
            Quoted::Int(n) => self.emit_integer_literal(*n, target_reg),
            Quoted::Str(s) => self.emit(&format!("LOADSYM {} {}", target_reg, s)),
            Quoted::Sym(s) => self.emit(&format!("LOADSYM {} {}", target_reg, s)),
            Quoted::Nil => {
                self.emit("LOADI R13 0");
                self.emit("LOADI R12 1");
                self.emit(&format!("EQ {} R12 R13", target_reg));
            }
            Quoted::List(list) => {
                self.emit("LOADI R13 0");
                self.emit("LOADI R12 1");
                self.emit(&format!("EQ {} R12 R13", target_reg));
                for item in list.iter().rev() {
                    self.emit(&format!("CONS R11 {} R11", target_reg)); // Push accumulated list tail
                    self.compile_quoted(item, target_reg); // Evaluate item into target_reg
                    self.emit("CAR R12 R11"); // Pop list tail into R12
                    self.emit("CDR R11 R11");
                    self.emit(&format!("CONS {} {} R12", target_reg, target_reg)); // target_reg = cons(item, tail)
                }
            }
            Quoted::DottedList(list, tail) => {
                self.compile_quoted(tail, target_reg);
                for item in list.iter().rev() {
                    self.emit(&format!("CONS R11 {} R11", target_reg)); // Push accumulated list tail
                    self.compile_quoted(item, target_reg); // Evaluate item into target_reg
                    self.emit("CAR R12 R11"); // Pop list tail into R12
                    self.emit("CDR R11 R11");
                    self.emit(&format!("CONS {} {} R12", target_reg, target_reg)); // target_reg = cons(item, tail)
                }
            }
        }
    }

    fn compile_cond(&mut self, branches: &[(Ir, Ir)], target_reg: &str) {
        let end_label = self.next_label("cond_end");

        for (test, body) in branches {
            let next_label = self.next_label("cond_next");

            self.compile_expr(test, "R1");

            // fpga-lisp's JF treats 0 as falsy, but my-lisp requires 0 to be truthy.
            // We generate a strict NIL check by creating NIL and comparing against it twice.
            // R9 is scratch here, not R4: R4 is the ENV register, and this NIL-check
            // runs unconditionally for every branch (even ones not taken), so clobbering
            // R4 here destroyed the environment before a taken branch's body could look
            // up any variable or recursive call in it -- the actual cause of self-recursive
            // `def` failing with RESULT_ERROR:Type (env lookups saw an empty/NIL env).
            self.emit("LOADI R2 0");
            self.emit("LOADI R3 1");
            self.emit("EQ R9 R2 R3"); // R9 = NIL

            self.emit("EQ R2 R1 R9"); // R2 = TRUE if R1 was NIL, else NIL
            self.emit("EQ R3 R2 R9"); // R3 = TRUE if R1 was NOT NIL, else NIL

            self.emit(&format!("JF R3 {}", next_label));

            self.compile_expr(body, target_reg);
            self.emit(&format!("JMP {}", end_label));

            self.emit(&format!("{}:", next_label));
        }
        self.emit(&format!("{}:", end_label));
    }

    fn compile_lambda(&mut self, params: &Params, body: &Ir, target_reg: &str) {
        let lambda_label = self.next_label("lambda_body");
        let skip_label = self.next_label("lambda_skip");

        self.emit("; LAMBDA START");
        self.emit(&format!("LOADI R15 {}", lambda_label));
        self.emit(&format!("CONS {} R15 R4", target_reg)); // Closure = (LABEL_PTR . ENV)
        self.emit(&format!("JMP {}", skip_label));

        self.emit(&format!("{}:", lambda_label));
        // Lambda Body
        // We must bind parameters (up to 3 mapped from R1, R2, R3)
        // We bind parameters mapped from R1..R3, R5..R9
        let arg_regs = ["R1", "R2", "R3", "R5", "R6", "R7", "R8", "R9"];

        match params {
            Params::Fixed(names) => {
                for (i, name) in names.iter().enumerate() {
                    if i < arg_regs.len() {
                        self.emit(&format!("LOADSYM R12 {}", name));
                        self.emit(&format!("CONS R13 R12 {}", arg_regs[i])); // (param_sym . arg_val)
                        self.emit("CONS R4 R13 R4"); // env = (pair . env)
                    }
                }
            }
            Params::Variadic { fixed, rest } => {
                // Normal params
                for (i, name) in fixed.iter().enumerate() {
                    if i < arg_regs.len() {
                        self.emit(&format!("LOADSYM R12 {}", name));
                        self.emit(&format!("CONS R13 R12 {}", arg_regs[i])); // (param_sym . arg_val)
                        self.emit("CONS R4 R13 R4"); // env = (pair . env)
                    }
                }
                // Variable arity tail: bind rest of args into a list
                self.emit("MOV R10 R0");
                for _ in 0..fixed.len() {
                    self.emit("CDR R10 R10");
                }

                self.emit(&format!("LOADSYM R12 {}", rest));
                self.emit("CONS R13 R12 R10"); // (param_sym . rest_args_list)
                self.emit("CONS R4 R13 R4"); // env = (pair . env)
            }
            Params::AllRest(rest) => {
                self.emit(&format!("LOADSYM R12 {}", rest));
                self.emit("CONS R13 R12 R0");
                self.emit("CONS R4 R13 R4");
            }
        }

        self.compile_expr(body, "R15"); // Return value in R15
        // Note: Caller saved return address in R14
        self.emit("RET R14");

        self.emit(&format!("{}:", skip_label));
        self.emit("; LAMBDA END");
    }

    fn emit_cml_lookup(&mut self) {
        self.emit("");
        self.emit("cml_lookup:");
        self.emit("; input: R12 = target symbol ID");
        self.emit("; input: R13 = environment list");
        self.emit("; output: R15 = value");
        self.emit("CAR R0 R13");
        self.emit("CAR R1 R0");
        self.emit("EQ  R2 R1 R12");
        self.emit("JF  R2 cml_lookup_next");
        self.emit("CDR R15 R0");
        self.emit("RET R14");
        self.emit("cml_lookup_next:");
        self.emit("CDR R13 R13");
        self.emit("JMP cml_lookup");
    }

    // Structural equality without letrec/recursion: an explicit worklist of
    // (a . b) pairs pushed onto the shared stack register R11, drained
    // iteratively. Type mismatches stop pushing new work but keep draining
    // so R11 always returns balanced to its caller.
    // Структурна рівність без letrec/рекурсії: явний worklist пар (a . b)
    // на спільному регістрі-стеку R11.
    // Strukturelle Gleichheit ohne letrec/Rekursion: explizite Arbeitsliste
    // von (a . b)-Paaren auf dem gemeinsamen Stapelregister R11.
    fn emit_cml_equal(&mut self) {
        self.emit("");
        self.emit("cml_equal:");
        self.emit("; input: R1 = a, R2 = b");
        self.emit("; output: R15 = TRUE/NIL");
        self.emit("CONS R12 R1 R2");
        self.emit("CONS R11 R12 R11"); // push initial (a . b)
        self.emit("LOADI R15 0");
        self.emit("ATOM R15 R15"); // R15 = TRUE (running result)

        self.emit("cml_equal_loop:");
        self.emit("LOADI R9 0");
        self.emit("LOADI R8 1");
        self.emit("EQ R7 R8 R9"); // R7 = NIL
        self.emit("EQ R6 R11 R7"); // R6 = TRUE if worklist empty
        self.emit("JF R6 cml_equal_pop");
        self.emit("JMP cml_equal_done");

        self.emit("cml_equal_pop:");
        self.emit("CAR R12 R11"); // top pair
        self.emit("CDR R11 R11"); // pop
        self.emit("CAR R1 R12");
        self.emit("CDR R2 R12");

        self.emit("ATOM R5 R1");
        self.emit("ATOM R6 R2");
        self.emit("JF R5 cml_equal_a_not_atom");
        self.emit("JF R6 cml_equal_mismatch"); // a atom, b cons
        self.emit("EQ R7 R1 R2");
        self.emit("JF R7 cml_equal_setfail");
        self.emit("JMP cml_equal_loop");

        self.emit("cml_equal_a_not_atom:");
        self.emit("JF R6 cml_equal_both_cons"); // a cons, b atom -> mismatch below
        self.emit("JMP cml_equal_mismatch");

        self.emit("cml_equal_both_cons:");
        self.emit("CAR R7 R1");
        self.emit("CAR R8 R2");
        self.emit("CONS R9 R7 R8");
        self.emit("CONS R11 R9 R11"); // push (car a . car b)
        self.emit("CDR R7 R1");
        self.emit("CDR R8 R2");
        self.emit("CONS R9 R7 R8");
        self.emit("CONS R11 R9 R11"); // push (cdr a . cdr b)
        self.emit("JMP cml_equal_loop");

        self.emit("cml_equal_mismatch:");
        self.emit("cml_equal_setfail:");
        self.emit("LOADI R13 0");
        self.emit("LOADI R12 1");
        self.emit("EQ R15 R12 R13"); // R15 = NIL, keep draining
        self.emit("JMP cml_equal_loop");

        self.emit("cml_equal_done:");
        self.emit("RET R14");
    }

    // fpga-lisp's assembler encodes LOADI's immediate as a bare 16-bit
    // field (`imm & 0xFFFF`), with no sign extension into the fixnum
    // value's wider field -- `LOADI R2 -1` loads 0xFFFF (65535), not the
    // register-width two's-complement -1. Negative literals must instead
    // be built with a real ALU op (SUB is register-register, no immediate
    // truncation) so the tagged word downstream ops see is the hardware's
    // actual negative representation. R13 is this codebase's established
    // disposable scratch register (see the NIL/TRUE-construction idiom
    // used throughout).
    // Асемблер fpga-lisp кодує LOADI-immediate як голе 16-бітне поле, без
    // sign extension -- `LOADI R2 -1` завантажує 0xFFFF, не справжнє
    // two's-complement -1. Від'ємні літерали будуємо через SUB (rr-опція,
    // без обрізання immediate).
    fn emit_integer_literal(&mut self, n: i64, target_reg: &str) {
        if n >= 0 {
            self.emit(&format!("LOADI {} {}", target_reg, n));
        } else {
            self.emit(&format!("LOADI {} 0", target_reg));
            self.emit(&format!("LOADI R13 {}", -n));
            self.emit(&format!("SUB {} {} R13", target_reg, target_reg));
        }
    }

    fn emit(&mut self, instr: &str) {
        self.output.push(instr.to_string());
    }
}
