use crate::ast::Expr;

pub struct Compiler {
    output: Vec<String>,
}

impl Compiler {
    pub fn new() -> Self {
        Compiler {
            output: Vec::new(),
        }
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
                // Currently fpga-lisp supports FIXNUM via LOADI, though the ISA might differ slightly.
                // Assuming LOADI REG VALUE or similar
                self.emit(&format!("LOADI {} {}", target_reg, n));
            }
            Expr::Symbol(s) => {
                self.emit(&format!("LOADSYM {} {}", target_reg, s));
            }
            Expr::List(list) => {
                if list.is_empty() {
                    self.emit(&format!("LOADSYM {} NIL", target_reg));
                } else if let Expr::Symbol(func) = &list[0] {
                    self.compile_call(func, &list[1..], target_reg);
                } else {
                    // Evaluating a list where head is not a symbol - advanced eval
                    self.emit("; unhandled list evaluation");
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
                    self.compile_expr(&args[0], target_reg);
                }
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
                self.emit(&format!("; unknown function {}", func));
            }
        }
    }

    fn emit(&mut self, instr: &str) {
        self.output.push(instr.to_string());
    }
}
