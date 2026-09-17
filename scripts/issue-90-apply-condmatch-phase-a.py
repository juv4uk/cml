from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    p = Path(path)
    text = p.read_text()
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected exact replacement site once, found {count}")
    p.write_text(text.replace(old, new, 1))


# C backend: canonical control uses private structural matching, never truthiness.
replace_once(
    "src/c_backend.rs",
    '            Ir::Cond { branches } => self.compile_cond(branches, env),\n',
    '            Ir::Cond { branches } => self.compile_cond(branches, env),\n'
    '            Ir::CondMatch { branches } => self.compile_cond_match(branches, env),\n',
)

c_cond = '''    fn compile_cond(&mut self, branches: &[(Ir, Ir)], env: &str) -> Result<String, CompileError> {
        let mut out = String::from("({ Value *_c;");
        let mut first = true;
        for (test, body) in branches {
            let test_expr = self.compile_expr(test, env)?;
            let body_expr = self.compile_expr(body, env)?;
            if first {
                out.push_str(&format!(
                    " if (truthy({test_expr})) {{ _c = {body_expr}; }}"
                ));
                first = false;
            } else {
                out.push_str(&format!(
                    " else if (truthy({test_expr})) {{ _c = {body_expr}; }}"
                ));
            }
        }
        out.push_str(" else { _c = &NIL_V; } _c; })");
        Ok(out)
    }
'''
c_cond_match = c_cond + '''
    fn compile_cond_match(
        &mut self,
        branches: &[(Ir, Quoted, Ir)],
        env: &str,
    ) -> Result<String, CompileError> {
        let mut out = String::from("({ Value *_c;");
        let mut first = true;
        for (query, expected, body) in branches {
            let query_expr = self.compile_expr(query, env)?;
            let expected_expr = self.compile_quoted(expected)?;
            let body_expr = self.compile_expr(body, env)?;
            if first {
                out.push_str(&format!(
                    " if (v_equal_p({query_expr}, {expected_expr})) {{ _c = {body_expr}; }}"
                ));
                first = false;
            } else {
                out.push_str(&format!(
                    " else if (v_equal_p({query_expr}, {expected_expr})) {{ _c = {body_expr}; }}"
                ));
            }
        }
        out.push_str(" else { _c = &NIL_V; } _c; })");
        Ok(out)
    }
'''
replace_once("src/c_backend.rs", c_cond, c_cond_match)

# FPGA backend already has private cml_equal: reuse it as mechanism-only result matching.
compiler_cond_validate = '''        Ir::Cond { branches } => branches.iter().try_for_each(|(test, body)| {
            validate_ir(test)?;
            validate_ir(body)
        }),
'''
replace_once(
    "src/compiler.rs",
    compiler_cond_validate,
    compiler_cond_validate
    + '''        Ir::CondMatch { branches } => branches.iter().try_for_each(|(query, expected, body)| {
            validate_ir(query)?;
            validate_quoted(expected)?;
            validate_ir(body)
        }),
''',
)
replace_once(
    "src/compiler.rs",
    '            Ir::Cond { branches } => self.compile_cond(branches, target_reg),\n',
    '            Ir::Cond { branches } => self.compile_cond(branches, target_reg),\n'
    '            Ir::CondMatch { branches } => self.compile_cond_match(branches, target_reg),\n',
)

fpga_cond = '''    fn compile_cond(&mut self, branches: &[(Ir, Ir)], target_reg: &str) {
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
'''
fpga_cond_match = fpga_cond + '''
    fn compile_cond_match(&mut self, branches: &[(Ir, Quoted, Ir)], target_reg: &str) {
        let end_label = self.next_label("cond_match_end");

        for (query, expected, body) in branches {
            let next_label = self.next_label("cond_match_next");
            self.compile_expr(query, "R1");
            self.preserve_across("R1", |c| c.compile_quoted(expected, "R2"));
            self.used_equal = true;
            self.call_subroutine("cml_equal");
            self.emit(&format!("JF R15 {}", next_label));
            self.compile_expr(body, target_reg);
            self.emit(&format!("JMP {}", end_label));
            self.emit(&format!("{}:", next_label));
        }
        self.emit(&format!("{}:", end_label));
    }
'''
replace_once("src/compiler.rs", fpga_cond, fpga_cond_match)

# x86 freestanding has no private structural matcher yet: fail named/closed.
replace_once(
    "src/x86_freestanding.rs",
    '            Ir::Cond { branches } => self.emit_cond(branches),\n',
    '            Ir::Cond { branches } => self.emit_cond(branches),\n'
    '            Ir::CondMatch { .. } => Err(CompileError::UnsupportedVariant(\n'
    '                "CondMatch (explicit result matcher not yet implemented)",\n'
    '            )),\n',
)

# Metadata traversal must preserve inert expected-result symbol provenance.
metadata_cond = '''        Ir::Cond { branches } => {
            for (test, body) in branches {
                collect_backend_symbol_names(test, out);
                collect_backend_symbol_names(body, out);
            }
        }
'''
replace_once(
    "src/x86_freestanding_metadata.rs",
    metadata_cond,
    metadata_cond
    + '''        Ir::CondMatch { branches } => {
            for (query, expected, body) in branches {
                collect_backend_symbol_names(query, out);
                collect_quoted_symbol_names(expected, out);
                collect_backend_symbol_names(body, out);
            }
        }
''',
)

print("issue #90 CondMatch phase-A exact patches applied")
