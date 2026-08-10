use crate::ast::Expr;

pub struct Compiler {
    output: Vec<String>,
    label_counter: usize,
    used_lookup: bool,
}

impl Compiler {
    pub fn new() -> Self {
        Compiler {
            output: Vec::new(),
            label_counter: 0,
            used_lookup: false,
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
                        let start_idx = list.len();
                        
                        // We need to build a list of the remaining args
                        // For simplicity, we just bind NIL for now since varargs are complex
                        // in register-based calling convention. But let's build it dynamically!
                        
                        // Start by creating a NIL
                        self.emit("LOADI R13 0");
                        self.emit("LOADI R12 1");
                        self.emit("EQ R10 R12 R13"); // R10 = NIL
                        
                        // We loop downwards to CONS the remaining args
                        let remaining_max = std::cmp::min(args.len() - 1, arg_regs.len());
                        for i in (start_idx..remaining_max).rev() {
                            self.emit(&format!("CONS R10 {} R10", arg_regs[i]));
                        }
                        
                        self.emit(&format!("LOADSYM R12 {}", tail_sym.to_uppercase()));
                        self.emit("CONS R13 R12 R10"); // (param_sym . rest_args_list)
                        self.emit("CONS R4 R13 R4"); // env = (pair . env)
                    }
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

    fn emit(&mut self, instr: &str) {
        self.output.push(instr.to_string());
    }
}
