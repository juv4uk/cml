// Ad-hoc, not part of the crate's normal test suite: compiles the curried,
// genuinely escaping closure from
// tests/x86_freestanding_test.rs::escaping_captured_closure_source_reaches_x86_admission
// and writes the resulting assembly to wsm-os/artifacts, to check whether it
// actually RUNS correctly (that existing test only checks the emitted
// assembly text contains expected call instructions, never executes it).
use cml::lower;
use cml::parser;
use cml::x86_freestanding::X86FreestandingBackend;

fn main() {
    // Oracle expects (A B): x=A captured by the escaping inner closure, then
    // applied to y=B.
    let source = "(((lambda (x) (lambda (y) (cons x (cons y (quote ()))))) (quote A)) (quote B))";
    let expressions = parser::parse(source).expect("parse");
    let ir = lower::lower_program(&expressions).expect("lower");
    let backend = X86FreestandingBackend::new();
    let assembly = backend.compile_program(&ir).expect("compile");
    std::fs::write(
        "../wsm-os/artifacts/nucleus-witness-escaping-closure-fixture.s",
        assembly,
    )
    .expect("write");
    println!("wrote nucleus-witness-escaping-closure-fixture.s for: {source}");
}
