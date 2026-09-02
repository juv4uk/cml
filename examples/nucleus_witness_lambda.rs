// Ad-hoc, not part of the crate's normal test suite: compiles the bounded
// single-argument identity lambda application through the x86_64-freestanding
// backend and writes the resulting assembly to wsm-os/artifacts, so
// wsm-os-hosted can actually execute it (not just assemble/text-shape-check
// it, which is all tests/x86_freestanding_test.rs's identity_lambda_* tests
// currently do).
use cml::lower;
use cml::parser;
use cml::x86_freestanding::X86FreestandingBackend;

fn main() {
    // Oracle expects 7 (the argument, returned unchanged by the identity lambda).
    let source = "((lambda (x) x) 7)";
    let expressions = parser::parse(source).expect("parse");
    let ir = lower::lower_program(&expressions).expect("lower");
    let backend = X86FreestandingBackend::new();
    let assembly = backend.compile_program(&ir).expect("compile");
    std::fs::write("../wsm-os/artifacts/nucleus-witness-lambda-fixture.s", assembly)
        .expect("write");
    println!("wrote nucleus-witness-lambda-fixture.s for: {source}");
}
