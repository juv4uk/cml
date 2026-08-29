import re

def fix(path):
    with open(path, 'r') as f:
        text = f.read()
    
    # We want to replace `Quoted::DottedList(items, tail) => { ... }` with `..., _ => todo!()`
    # Let's just do text replacements for the specific missing arms.
    # In c_backend.rs:
    text = text.replace(
        "            Ir::Prim { op, args } => self.compile_prim(*op, args, env),\n        }",
        "            Ir::Prim { op, args } => self.compile_prim(*op, args, env),\n            _ => todo!(),\n        }"
    )
    text = text.replace(
        "                self.emit_quoted_list(items, Some(tail), target_reg, acc)\n            }\n        }",
        "                self.emit_quoted_list(items, Some(tail), target_reg, acc)\n            }\n            _ => todo!(),\n        }"
    )
    # compiler.rs:
    text = text.replace(
        "        Ir::Nil | Ir::True | Ir::Var(_) => Ok(()),\n    }",
        "        Ir::Nil | Ir::True | Ir::Var(_) => Ok(()),\n        _ => todo!(),\n    }"
    )
    text = text.replace(
        "        Quoted::Str(_) | Quoted::Sym(_) | Quoted::Nil => Ok(()),\n    }",
        "        Quoted::Str(_) | Quoted::Sym(_) | Quoted::Nil => Ok(()),\n        _ => todo!(),\n    }"
    )
    text = text.replace(
        "            Ir::Prim { op, args } => self.compile_prim(*op, args, target_reg),\n        }",
        "            Ir::Prim { op, args } => self.compile_prim(*op, args, target_reg),\n            _ => todo!(),\n        }"
    )
    text = text.replace(
        "                self.emit_quoted_list(items, Some(tail), target_reg, acc)\n            }\n        }",
        "                self.emit_quoted_list(items, Some(tail), target_reg, acc)\n            }\n            _ => todo!(),\n        }"
    )
    
    # compute.rs
    text = text.replace(
        "        Ir::Prim { .. } => Err(ExecutionError::UnsupportedPrim),\n    }",
        "        Ir::Prim { .. } => Err(ExecutionError::UnsupportedPrim),\n        _ => todo!(),\n    }"
    )
    
    # x86_freestanding.rs
    text = text.replace(
        "        Ir::Def { .. } => return Err(CompileError::Unsupported(\"def\")),\n    }",
        "        Ir::Def { .. } => return Err(CompileError::Unsupported(\"def\")),\n        _ => return Err(CompileError::Unsupported(\"extended node\")),\n    }"
    )
    text = text.replace(
        "        Quoted::DottedList(_, _) => return Err(CompileError::Unsupported(\"dotted list\")),\n    }",
        "        Quoted::DottedList(_, _) => return Err(CompileError::Unsupported(\"dotted list\")),\n        _ => return Err(CompileError::Unsupported(\"extended node\")),\n    }"
    )
    text = text.replace(
        "                self.emit_quoted_list(items, Some(tail), target_reg)\n            }\n        }",
        "                self.emit_quoted_list(items, Some(tail), target_reg)\n            }\n            _ => return Err(CompileError::Unsupported(\"extended node\")),\n        }"
    )
    
    with open(path, 'w') as f:
        f.write(text)

for f in ["src/c_backend.rs", "src/compiler.rs", "src/compute.rs", "src/x86_freestanding.rs"]:
    fix(f)
