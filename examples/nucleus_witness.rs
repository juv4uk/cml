// Ad-hoc, not part of the crate's normal test suite: compiles one
// independent C0-scope conformance fixture through the x86_64-freestanding
// backend and writes the resulting assembly to wsm-os/artifacts, so
// wsm-os-hosted can actually execute it (not just assemble-check it).
use cml::lower;
use cml::parser;
use cml::x86_freestanding::X86FreestandingBackend;

fn main() {
    // Same expression as my-lisp/tests/fixtures/conformance.my's
    // "(atom (quote ()))" tier-1/G2 fixture (expected: t).
    let source = "(atom (quote ()))";
    let expressions = parser::parse(source).expect("parse");
    let ir = lower::lower_program(&expressions).expect("lower");
    let backend = X86FreestandingBackend::new();
    let assembly = backend.compile_program(&ir).expect("compile");
    std::fs::write("../wsm-os/artifacts/nucleus-witness-fixture.s", assembly).expect("write");
    println!("wrote nucleus-witness-fixture.s for: {source}");
}
