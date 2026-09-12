//! Emits the standalone `(cond (() (quote wrong)) (t (quote right)))`
//! witness assembly requested by wsm-my-lisp, for their Stage2 my-eval-cond
//! parity tracking. See the SCOPE NOTE on
//! standalone_cond_true_false_branch_selection_witness in
//! tests/x86_freestanding_test.rs: this is a hand-written standalone
//! fixture, not compiled from the real meta-eval.my evaluator call graph.
use cml::lower;
use cml::parser;
use cml::x86_freestanding::X86FreestandingBackend;

fn main() {
    let source = "(cond (() (quote wrong)) (t (quote right)))";
    let expressions = parser::parse(source).expect("parse");
    let program = lower::lower_program(&expressions).expect("lower");
    let assembly = X86FreestandingBackend::new()
        .compile_program(&program)
        .expect("compile");
    print!("{assembly}");
}
