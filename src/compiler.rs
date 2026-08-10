use crate::ast::Expr;

pub struct Compiler {
    output: Vec<String>,
    label_counter: usize,
}

impl Compiler {
    pub fn new() -> Self {
        Compiler {
            output: Vec::new(),
            label_counter: 0,
        }
    }

    fn next_label(&mut self, prefix: &str) -> String {
        self.label_counter += 1;
        format!("{}_{}", prefix, self.label_counter)
    }

    pub fn compile(&mut self, exprs: &[Expr]) -> String {
        for expr in exprs {
            self.compile_expr(expr, "R1");
        }
        self.emit("HALT");
        self.output.join("\n")
    }

    fn compile_expr(&mut self, expr: &Expr, target_reg: &str) {
        match expr {
            Expr::Integer(n) => {
                self.emit(&format!("LOADI {} {}", target_reg, n));
            }
            Expr::Symbol(s) => {
                // In my-lisp, t and nil are self-evaluating symbols
                if s == "t" {
                    self.emit(&format!("LOADSYM {} TRUE", target_reg));
                } else if s == "nil" {
                    self.emit(&format!("LOADSYM {} NIL", target_reg));
                } else {
                    // Variable lookup
                    self.emit(&format!("; LOOKUP {} -> {}", s, target_reg));
                    // For now, we emit a mock lookup instruction
                    // Real implementation would walk the ENV list
                }
            }
            Expr::List(list) => {
                if list.is_empty() {
                    self.emit(&format!("LOADSYM {} NIL", target_reg));
                } else if let Expr::Symbol(func) = &list[0] {
                    self.compile_call(func, &list[1..], target_reg);
                } else {
                    self.emit("; unhandled complex list evaluation");
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
                self.emit(&format!("; CALL {}", func));
                // To call a generic function:
                // 1. Evaluate arguments and push them or build a list
                // 2. Evaluate the function symbol (closure lookup)
                // 3. JAL / CALL
            }
        }
    }

    fn compile_quote(&mut self, expr: &Expr, target_reg: &str) {
        match expr {
            Expr::Integer(n) => self.emit(&format!("LOADI {} {}", target_reg, n)),
            Expr::Symbol(s) => self.emit(&format!("LOADSYM {} {}", target_reg, s.to_uppercase())),
            Expr::List(list) => {
                if list.is_empty() {
                    self.emit(&format!("LOADSYM {} NIL", target_reg));
                } else {
                    // We need to construct the list using CONS
                    self.emit("; compiling quoted list...");
                    // This requires emitting CONS for each element from right to left
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
                    
                    // Compile predicate
                    self.compile_expr(&pair[0], "R1");
                    
                    // fpga-lisp JF jumps if R1 is NIL/False
                    self.emit(&format!("JF R1 {}", next_label));
                    
                    // Compile consequence
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
            
            self.emit(&format!("; LAMBDA START"));
            // At runtime, we need to create a closure: (CONS lambda_label ENV)
            // But for now, we just emit the label reference and skip the body
            
            self.emit(&format!("LOADI R1 {}", lambda_label)); 
            self.emit(&format!("CONS R1 ENV -> {}", target_reg)); // ENV is a special register in fpga-lisp
            self.emit(&format!("JMP {}", skip_label));
            
            self.emit(&format!("{}:", lambda_label));
            // Compile lambda body
            self.compile_expr(&args[1], "R1");
            self.emit("RET");
            
            self.emit(&format!("{}:", skip_label));
            self.emit(&format!("; LAMBDA END"));
        }
    }

    fn emit(&mut self, instr: &str) {
        self.output.push(instr.to_string());
    }
}
