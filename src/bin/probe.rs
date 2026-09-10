use cml::lower;
use cml::parser;
use cml::x86_freestanding::X86FreestandingBackend;

fn main() {
    for src in [
        "(def add-two (lambda (a b) (+ a b))) (add-two 3 4)",
        "(def four (lambda () 4)) (four)",
        "(def sub1 (lambda (n) (- n 1))) (def count (lambda (n) (cond ((eq n 0) (quote done)) (t (count (sub1 n)))))) (count 5)",
    ] {
        println!("=== SRC: {src}");
        match parser::parse(src) {
            Err(e) => {
                println!("parse error: {e:?}");
                continue;
            }
            Ok(exprs) => match lower::lower_program(&exprs) {
                Err(e) => {
                    println!("lower error: {e}");
                    continue;
                }
                Ok(ir) => {
                    println!("IR: {ir:?}");
                    let backend = X86FreestandingBackend::new();
                    match backend.compile_program(&ir) {
                        Ok(asm) => {
                            println!("=== COMPILES ===");
                            println!("{asm}");
                        }
                        Err(e) => println!("=== COMPILE ERROR: {e} ==="),
                    }
                }
            },
        }
    }
}
