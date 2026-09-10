"""Apply COMPILER-04 two-pass top-level def compilation to src/c_backend.rs."""
from pathlib import Path

p = Path("src/c_backend.rs")
t = p.read_text()
if "compile_def_placeholder" in t and "two-pass top-level defs" in t:
    print("already")
    raise SystemExit(0)

start = t.find("    pub fn compile_program(&mut self, program: &[Ir])")
if start < 0:
    raise SystemExit("compile_program missing")
end = t.find("    fn compile_def(&mut self, name: &str, value: &Ir)", start)
if end < 0:
    raise SystemExit("compile_def missing")
end_def = t.find("    fn compile_expr(&mut self, ir: &Ir, env: &str)", end)
if end_def < 0:
    raise SystemExit("compile_expr missing")

old = t[start:end_def]
fmt_line = [l for l in old.splitlines(True) if "print_value(result)" in l][0]
empty_line = (
    '            main_body.push_str("    { print_value(&NIL_V); printf(\\"\\\\n\\"); }\\n");\n'
)

ok_block = """        Ok(format!(
            "{RUNTIME}\\n{}\\n\\nint main(void) {{\\n    bootstrap_builtins();\\n{}    return 0;\\n}}\\n",
            self.functions.join("\\n"),
            main_body,
        ))
    }
"""

new = f"""    pub fn compile_program(&mut self, program: &[Ir]) -> Result<String, CompileError> {{
        let mut main_body = String::new();
        // COMPILER-04 / COMPILER-12: two-pass top-level defs so mutual
        // recursion sees sibling placeholders before any closure captures
        // global_env (env list pointer at creation time).
        for ir in program {{
            if let Ir::Def {{ name, .. }} = ir {{
                main_body.push_str(&self.compile_def_placeholder(name));
            }}
        }}
        for ir in program {{
            if let Ir::Def {{ name, value }} = ir {{
                main_body.push_str(&self.compile_def_backpatch(name, value)?);
            }}
        }}
        // my-lisp empty-program semantics (TASK-001)
        let mut printed_result = false;
        for ir in program {{
            match ir {{
                Ir::Def {{ .. }} => {{}}
                other => {{
                    let expr = self.compile_expr(other, "global_env")?;
                    main_body.push_str(&format!(
{fmt_line}                    ));
                    printed_result = true;
                }}
            }}
        }}
        if !printed_result {{
{empty_line}        }}

{ok_block}
    /// Install `(name . nil)` on `global_env` before any value is compiled.
    fn compile_def_placeholder(&self, name: &str) -> String {{
        format!(
            "    Value *ph_{{name}} = mk_cons(mk_sym(\\"{{name}}\\"), &NIL_V);\\n    global_env = mk_cons(ph_{{name}}, global_env);\\n"
        )
    }}

    /// Compile `value` and SETCDR the placeholder.
    fn compile_def_backpatch(
        &mut self,
        name: &str,
        value: &Ir,
    ) -> Result<String, CompileError> {{
        let value_expr = self.compile_expr(value, "global_env")?;
        Ok(format!("    ph_{{name}}->u.cons.cdr = {{value_expr}};\\n"))
    }}

    fn compile_def(&mut self, name: &str, value: &Ir) -> Result<String, CompileError> {{
        let mut out = self.compile_def_placeholder(name);
        out.push_str(&self.compile_def_backpatch(name, value)?);
        Ok(out)
    }}

"""

p.write_text(t[:start] + new + t[end_def:])
assert "compile_def_placeholder" in p.read_text()
assert "two-pass top-level defs" in p.read_text()
print("patched")
