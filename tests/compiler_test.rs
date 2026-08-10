use cml::parser;
use cml::compiler::Compiler;

#[test]
fn test_compile_cond() {
    let code = "(cond (t 'a) (nil 'b))";
    let exprs = parser::parse(code).unwrap();
    let mut compiler = Compiler::new();
    let asm = compiler.compile(&exprs);
    
    assert!(asm.contains("LOADSYM R1 TRUE"));
    assert!(asm.contains("JF R1"));
    assert!(asm.contains("LOADSYM R15 A"));
    assert!(asm.contains("LOADSYM R1 NIL"));
    assert!(asm.contains("LOADSYM R15 B"));
    assert!(asm.contains("HALT"));
}

#[test]
fn test_compile_lambda() {
    let code = "(lambda (x) x)";
    let exprs = parser::parse(code).unwrap();
    let mut compiler = Compiler::new();
    let asm = compiler.compile(&exprs);
    
    assert!(asm.contains("LAMBDA START"));
    assert!(asm.contains("LAMBDA END"));
    assert!(asm.contains("CONS R15 R4 -> R15")); // Closure building
    assert!(asm.contains("RET") || asm.contains("JMP R14")); // Return
}

#[test]
fn test_compile_apply() {
    let code = "((lambda (x) x) 'test)";
    let exprs = parser::parse(code).unwrap();
    let mut compiler = Compiler::new();
    let asm = compiler.compile(&exprs);
    
    assert!(asm.contains("CALL START"));
    assert!(asm.contains("CALL END"));
    assert!(asm.contains("CAR R10 R15"));
    assert!(asm.contains("CDR R4 R15"));
    assert!(asm.contains("JMP R10"));
}
