use crate::ast::Expr;

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

    pub fn compile(&mut self, exprs: &[Expr]) -> String {
        // Initialize R4 (ENV) and R11 (Stack) to NIL at program start
        self.emit("LOADI R13 0");
        self.emit("LOADI R12 1");
        self.emit("EQ R4 R12 R13"); // R4 = NIL
        self.emit("MOV R11 R4"); // R11 = NIL
        
        for expr in exprs {
            self.compile_expr(expr, "R15");
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

    fn compile_expr(&mut self, expr: &Expr, target_reg: &str) {
        match expr {
            Expr::Integer(n) => {
                self.emit(&format!("LOADI {} {}", target_reg, n));
            }
            Expr::String(s) => {
                self.emit(&format!("LOADSYM {} {}", target_reg, s.to_uppercase()));
            }
            Expr::Symbol(s) => {
                let s_upper = s.to_uppercase();
                if s_upper == "T" {
                    self.emit(&format!("LOADI {} 0", target_reg));
                    self.emit(&format!("ATOM {} {}", target_reg, target_reg));
                } else if s_upper == "NIL" {
                    self.emit("LOADI R13 0");
                    self.emit("LOADI R12 1");
                    self.emit(&format!("EQ {} R12 R13", target_reg));
                } else {
                    // Variable lookup
                    self.used_lookup = true;
                    self.emit(&format!("; LOOKUP {}", s));
                    self.emit(&format!("LOADSYM R12 {}", s.to_uppercase()));
                    self.emit("MOV R13 R4"); // R4 is our standard ENV register
                    self.emit("CONS R11 R14 R11"); // Push R14
                    let ret_label = self.next_label("cml_lookup_ret");
                    self.emit(&format!("LOADI R14 {}", ret_label)); 
                    self.emit("CALL R14 cml_lookup");
                    self.emit(&format!("{}:", ret_label));
                    self.emit("CAR R14 R11"); // Pop R14
                    self.emit("CDR R11 R11");
                    self.emit(&format!("MOV {} R15", target_reg));
                }
            }
            Expr::List(list) => {
                if list.is_empty() {
                    self.emit("LOADI R13 0");
                    self.emit("LOADI R12 1");
                    self.emit(&format!("EQ {} R12 R13", target_reg));
                } else if let Expr::Symbol(func) = &list[0] {
                    self.compile_call(func, &list[1..], target_reg);
                } else {
                    // if operator is a generic expression (e.g. ((lambda (x) x) 5))
                    self.compile_generic_call(&list[0], &list[1..], target_reg);
                }
            }
            Expr::DottedList(_, _) => {
                // If it reaches here as an expression, it might be a malformed unquoted list
                // or we are evaluating it directly (which Lisp usually doesn't allow for dotted lists)
                self.emit("; unquoted dotted list unsupported");
            }
        }
    }

    fn compile_call(&mut self, func: &str, args: &[Expr], target_reg: &str) {
        match func {
            "quote" => {
                if args.len() == 1 {
                    self.compile_quote(&args[0], target_reg);
                }
            }
            "cond" => {
                self.compile_cond(args, target_reg);
            }
            "lambda" => {
                self.compile_lambda(args, target_reg);
            }
            "let" => {
                self.compile_let(args, target_reg);
            }
            "cons" => {
                if args.len() == 2 {
                    self.compile_expr(&args[0], "R1");
                    self.compile_expr(&args[1], "R2");
                    self.emit(&format!("CONS {} R1 R2", target_reg));
                }
            }
            "car" => {
                if args.len() == 1 {
                    self.compile_expr(&args[0], "R1");
                    self.emit(&format!("CAR {} R1", target_reg));
                }
            }
            "cdr" => {
                if args.len() == 1 {
                    self.compile_expr(&args[0], "R1");
                    self.emit(&format!("CDR {} R1", target_reg));
                }
            }
            "eq" => {
                if args.len() == 2 {
                    self.compile_expr(&args[0], "R1");
                    self.compile_expr(&args[1], "R2");
                    self.emit(&format!("EQ {} R1 R2", target_reg));
                }
            }
            "atom" => {
                if args.len() == 1 {
                    self.compile_expr(&args[0], "R1");
                    self.emit(&format!("ATOM {} R1", target_reg));
                }
            }
            "equal?" => {
                if args.len() == 2 {
                    self.compile_expr(&args[0], "R1");
                    self.compile_expr(&args[1], "R2");
                    self.used_equal = true;
                    self.emit("CALL R14 cml_equal");
                    self.emit(&format!("MOV {} R15", target_reg));
                }
            }
            _ => {
                // Call a user-defined function named `func`
                self.compile_generic_call(&Expr::Symbol(func.to_string()), args, target_reg);
            }
        }
    }
    
    fn compile_generic_call(&mut self, func_expr: &Expr, args: &[Expr], target_reg: &str) {
        self.emit("; CALL START");
        // 1. Evaluate arguments (support up to 8 args mapped to R1..R3, R5..R9)
        let arg_regs = ["R1", "R2", "R3", "R5", "R6", "R7", "R8", "R9"];
        
        for (i, arg) in args.iter().enumerate() {
            if i < arg_regs.len() {
                self.compile_expr(arg, arg_regs[i]);
            }
        }
        
        // 2. Evaluate the closure expression (into R15)
        self.compile_expr(func_expr, "R15");

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
        
        // Save ENV (R4) and Link Register (R14) to stack (R11)
        self.emit("CONS R11 R4 R11"); // Push R4
        self.emit("CONS R11 R14 R11"); // Push R14
        
        // Closure is (LABEL_PTR . CAPTURED_ENV)
        let ret_label = self.next_label("call_ret");
        self.emit("CAR R10 R15"); // Extract LABEL_PTR to R10
        self.emit("CDR R4 R15");  // Extract CAPTURED_ENV to R4 (current ENV register)
        
        self.emit(&format!("LOADI R14 {}", ret_label)); // Return address
        self.emit("RET R10"); // Indirect jump to lambda body
        
        self.emit(&format!("{}:", ret_label));
        // Restore R14 and R4
        self.emit("CAR R14 R11"); // Pop R14
        self.emit("CDR R11 R11");
        self.emit("CAR R4 R11");  // Pop R4
        self.emit("CDR R11 R11");
        
        self.emit(&format!("MOV {} R15", target_reg)); // Lambda returns in R15 by convention
        self.emit("; CALL END");
    }

    // `let` is a derived Lisp form, not an FPGA primitive. Lower
    // (let ((name value) ...) body) to ((lambda (name ...) body) value ...).
    // `let` — похідна форма Lisp, а не примітив FPGA.
    // `let` ist eine abgeleitete Lisp-Form, keine FPGA-Primitive.
    fn compile_let(&mut self, args: &[Expr], target_reg: &str) {
        let [Expr::List(bindings), body] = args else {
            self.emit("; malformed let");
            return;
        };

        let mut parameters = Vec::with_capacity(bindings.len());
        let mut values = Vec::with_capacity(bindings.len());
        for binding in bindings {
            let Expr::List(pair) = binding else {
                self.emit("; malformed let binding");
                return;
            };
            let [Expr::Symbol(name), value] = pair.as_slice() else {
                self.emit("; malformed let binding");
                return;
            };
            parameters.push(Expr::Symbol(name.clone()));
            values.push(value.clone());
        }

        let lambda = Expr::List(vec![
            Expr::Symbol("lambda".to_string()),
            Expr::List(parameters),
            body.clone(),
        ]);
        self.compile_generic_call(&lambda, &values, target_reg);
    }

    fn compile_quote(&mut self, expr: &Expr, target_reg: &str) {
        match expr {
            Expr::Integer(n) => self.emit(&format!("LOADI {} {}", target_reg, n)),
            Expr::String(s) => self.emit(&format!("LOADSYM {} {}", target_reg, s.to_uppercase())),
            Expr::Symbol(s) => self.emit(&format!("LOADSYM {} {}", target_reg, s.to_uppercase())),
            Expr::List(list) => {
                if list.is_empty() {
                    self.emit("LOADI R13 0");
                    self.emit("LOADI R12 1");
                    self.emit(&format!("EQ {} R12 R13", target_reg));
                } else {
                    self.emit("LOADI R13 0");
                    self.emit("LOADI R12 1");
                    self.emit(&format!("EQ {} R12 R13", target_reg));
                    for item in list.iter().rev() {
                        self.emit(&format!("CONS R11 {} R11", target_reg)); // Push accumulated list tail
                        self.compile_quote(item, target_reg); // Evaluate item into target_reg
                        self.emit("CAR R12 R11"); // Pop list tail into R12
                        self.emit("CDR R11 R11");
                        self.emit(&format!("CONS {} {} R12", target_reg, target_reg)); // target_reg = cons(item, tail)
                    }
                }
            }
            Expr::DottedList(list, tail) => {
                self.compile_quote(tail, target_reg);
                for item in list.iter().rev() {
                    self.emit(&format!("CONS R11 {} R11", target_reg)); // Push accumulated list tail
                    self.compile_quote(item, target_reg); // Evaluate item into target_reg
                    self.emit("CAR R12 R11"); // Pop list tail into R12
                    self.emit("CDR R11 R11");
                    self.emit(&format!("CONS {} {} R12", target_reg, target_reg)); // target_reg = cons(item, tail)
                }
            }
        }
    }

    fn compile_cond(&mut self, branches: &[Expr], target_reg: &str) {
        let end_label = self.next_label("cond_end");
        
        for branch in branches {
            if let Expr::List(pair) = branch {
                if pair.len() == 2 {
                    let next_label = self.next_label("cond_next");
                    
                    self.compile_expr(&pair[0], "R1");
                    
                    // fpga-lisp's JF treats 0 as falsy, but my-lisp requires 0 to be truthy.
                    // We generate a strict NIL check by creating NIL and comparing against it twice.
                    self.emit("LOADI R2 0");
                    self.emit("LOADI R3 1");
                    self.emit("EQ R4 R2 R3"); // R4 = NIL
                    
                    self.emit("EQ R2 R1 R4"); // R2 = TRUE if R1 was NIL, else NIL
                    self.emit("EQ R3 R2 R4"); // R3 = TRUE if R1 was NOT NIL, else NIL
                    
                    self.emit(&format!("JF R3 {}", next_label));
                    
                    self.compile_expr(&pair[1], target_reg);
                    self.emit(&format!("JMP {}", end_label));
                    
                    self.emit(&format!("{}:", next_label));
                }
            }
        }
        self.emit(&format!("{}:", end_label));
    }

    fn compile_lambda(&mut self, args: &[Expr], target_reg: &str) {
        if args.len() >= 2 {
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
            
            // Check if it's a DottedList for variable arity or a standard List
            match &args[0] {
                Expr::List(params) => {
                    for (i, param) in params.iter().enumerate() {
                        if let Expr::Symbol(p) = param {
                            if i < arg_regs.len() {
                                self.emit(&format!("LOADSYM R12 {}", p.to_uppercase()));
                                self.emit(&format!("CONS R13 R12 {}", arg_regs[i])); // (param_sym . arg_val)
                                self.emit("CONS R4 R13 R4"); // env = (pair . env)
                            }
                        }
                    }
                }
                Expr::DottedList(list, tail) => {
                    // Normal params
                    for (i, param) in list.iter().enumerate() {
                        if let Expr::Symbol(p) = param {
                            if i < arg_regs.len() {
                                self.emit(&format!("LOADSYM R12 {}", p.to_uppercase()));
                                self.emit(&format!("CONS R13 R12 {}", arg_regs[i])); // (param_sym . arg_val)
                                self.emit("CONS R4 R13 R4"); // env = (pair . env)
                            }
                        }
                    }
                    // Variable arity tail: bind rest of args into a list
                    if let Expr::Symbol(tail_sym) = &**tail {
                        self.emit("MOV R10 R0");
                        for _ in 0..list.len() {
                            self.emit("CDR R10 R10");
                        }
                        
                        self.emit(&format!("LOADSYM R12 {}", tail_sym.to_uppercase()));
                        self.emit("CONS R13 R12 R10"); // (param_sym . rest_args_list)
                        self.emit("CONS R4 R13 R4"); // env = (pair . env)
                    }
                }
                Expr::Symbol(tail_sym) => {
                    self.emit(&format!("LOADSYM R12 {}", tail_sym.to_uppercase()));
                    self.emit("CONS R13 R12 R0");
                    self.emit("CONS R4 R13 R4");
                }
                _ => {}
            }
            
            self.compile_expr(&args[1], "R15"); // Return value in R15
            // Note: Caller saved return address in R14
            self.emit("RET R14");
            
            self.emit(&format!("{}:", skip_label));
            self.emit("; LAMBDA END");
        }
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

    fn emit(&mut self, instr: &str) {
        self.output.push(instr.to_string());
    }
}
