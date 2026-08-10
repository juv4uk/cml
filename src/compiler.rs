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
        // Initialize R4 (ENV) to NIL at program start
        self.emit("LOADSYM R4 NIL");
        
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
            Expr::Symbol(s) => {
                if s == "t" {
                    self.emit(&format!("LOADSYM {} TRUE", target_reg));
                } else if s == "nil" {
                    self.emit(&format!("LOADSYM {} NIL", target_reg));
                } else {
                    // Variable lookup
                    self.used_lookup = true;
                    self.emit(&format!("; LOOKUP {}", s));
                    self.emit(&format!("LOADSYM R12 {}", s.to_uppercase()));
                    self.emit("MOV R13 R4"); // R4 is our standard ENV register
                    self.emit("LOADI R14 cml_lookup_ret"); // fpga-lisp doesn't have indirect CALL easily, let's use direct JMP if needed. Wait.
                    // Wait, fpga-lisp CALL is: "CALL R14, cml_lookup"
                    // But in assembler.py it might be written as `CALL R14 cml_lookup`. Let's assume assembler supports `CALL rd label`.
                    self.emit("CALL R14 cml_lookup");
                    self.emit(&format!("MOV {} R15", target_reg));
                }
            }
            Expr::List(list) => {
                if list.is_empty() {
                    self.emit(&format!("LOADSYM {} NIL", target_reg));
                } else if let Expr::Symbol(func) = &list[0] {
                    self.compile_call(func, &list[1..], target_reg);
                } else {
                    // if operator is a generic expression (e.g. ((lambda (x) x) 5))
                    self.compile_generic_call(&list[0], &list[1..], target_reg);
                }
            }
            Expr::DottedList(_, _) => {
                self.emit("; dotted list unsupported yet");
            }
            Expr::String(_) => {
                self.emit("; string unsupported yet");
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
                    self.emit(&format!("CONS R1 R2 -> {}", target_reg));
                }
            }
            "car" => {
                if args.len() == 1 {
                    self.compile_expr(&args[0], "R1");
                    self.emit(&format!("CAR R1 -> {}", target_reg));
                }
            }
            "cdr" => {
                if args.len() == 1 {
                    self.compile_expr(&args[0], "R1");
                    self.emit(&format!("CDR R1 -> {}", target_reg));
                }
            }
            "eq" => {
                if args.len() == 2 {
                    self.compile_expr(&args[0], "R1");
                    self.compile_expr(&args[1], "R2");
                    self.emit(&format!("EQ R1 R2 -> {}", target_reg));
                }
            }
            "atom" => {
                if args.len() == 1 {
                    self.compile_expr(&args[0], "R1");
                    self.emit(&format!("ATOM R1 -> {}", target_reg));
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
        // 1. Evaluate arguments (support up to 3 args mapped to R1, R2, R3)
        // A full compiler would push these to a stack, but we assume max 3 registers for now.
        for (i, arg) in args.iter().enumerate() {
            if i < 3 {
                self.compile_expr(arg, &format!("R{}", i + 1));
            }
        }
        
        // 2. Evaluate the closure expression (into R15)
        self.compile_expr(func_expr, "R15");
        
        // Closure is (LABEL_PTR . CAPTURED_ENV)
        let ret_label = self.next_label("call_ret");
        self.emit("CAR R10 R15"); // Extract LABEL_PTR to R10
        self.emit("CDR R4 R15");  // Extract CAPTURED_ENV to R4 (current ENV register)
        
        self.emit(&format!("LOADI R14 {}", ret_label)); // Return address
        self.emit("JMP R10"); // Indirect JMP to the lambda body
        
        self.emit(&format!("{}:", ret_label));
        self.emit(&format!("MOV {} R15", target_reg)); // Lambda returns in R15 by convention
        self.emit("; CALL END");
    }

    fn compile_quote(&mut self, expr: &Expr, target_reg: &str) {
        match expr {
            Expr::Integer(n) => self.emit(&format!("LOADI {} {}", target_reg, n)),
            Expr::Symbol(s) => self.emit(&format!("LOADSYM {} {}", target_reg, s.to_uppercase())),
            Expr::List(list) => {
                if list.is_empty() {
                    self.emit(&format!("LOADSYM {} NIL", target_reg));
                } else {
                    self.emit("; compiling quoted list...");
                }
            }
            _ => self.emit("; unhandled quote type"),
        }
    }

    fn compile_cond(&mut self, branches: &[Expr], target_reg: &str) {
        let end_label = self.next_label("cond_end");
        
        for branch in branches {
            if let Expr::List(pair) = branch {
                if pair.len() == 2 {
                    let next_label = self.next_label("cond_next");
                    
                    self.compile_expr(&pair[0], "R1");
                    self.emit(&format!("JF R1 {}", next_label));
                    
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
            self.emit(&format!("CONS R15 R4 -> {}", target_reg)); // Closure = (LABEL_PTR . ENV)
            self.emit(&format!("JMP {}", skip_label));
            
            self.emit(&format!("{}:", lambda_label));
            // Lambda Body
            // We must bind parameters (up to 3 mapped from R1, R2, R3)
            if let Expr::List(params) = &args[0] {
                for (i, param) in params.iter().enumerate() {
                    if let Expr::Symbol(p) = param {
                        if i < 3 {
                            self.emit(&format!("LOADSYM R12 {}", p.to_uppercase()));
                            self.emit(&format!("CONS R13 R12 R{}", i + 1)); // (param_sym . arg_val)
                            self.emit("CONS R4 R13 R4"); // env = (pair . env)
                        }
                    }
                }
            }
            
            self.compile_expr(&args[1], "R15"); // Return value in R15
            // Note: Caller saved return address in R14
            self.emit("JMP R14"); // RET
            
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
        self.emit("JMP R14"); // RET
        self.emit("cml_lookup_next:");
        self.emit("CDR R13 R13");
        self.emit("JMP cml_lookup");
    }

    fn emit(&mut self, instr: &str) {
        self.output.push(instr.to_string());
    }
}
