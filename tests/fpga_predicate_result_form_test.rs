use cml::compiler::Compiler;
use cml::lower;
use cml::parser;

fn compile(source: &str) -> String {
    let expressions = parser::parse(source).expect("SENS source must parse");
    let program = lower::lower_program(&expressions).expect("SENS source must lower");
    Compiler::new().compile(&program).expect("FPGA source must compile")
}

#[test]
fn eq_materializes_corpus_relation_instead_of_raw_fpga_truth() {
    let assembly = compile("(00000011 (quote radio) (quote radio))");

    assert!(assembly.contains("EQ R3 R1 R2"));
    assert!(assembly.contains("JF R3 relation_false_"));
    assert!(
        !assembly.contains("EQ R15 R1 R2"),
        "raw FPGA EQ result must not escape as the language result"
    );
}

#[test]
fn atom_preserves_the_corpus_nil_atom_pair_trichotomy() {
    let assembly = compile("(00000010 (quote radio))");

    assert!(assembly.contains("atom_non_nil_"));
    assert!(assembly.contains("ATOM R3 R1"));
    assert!(assembly.contains("relation_false_"));
    assert!(
        !assembly.contains("ATOM R15 R1"),
        "raw FPGA ATOM result must not escape as the language result"
    );
}

#[test]
fn structural_equal_materializes_the_same_corpus_relation_shape() {
    let assembly = compile("(00100010 (quote (p . 0)) (quote (p . 0)))");

    assert!(assembly.contains("CALL R14 cml_equal"));
    assert!(assembly.contains("JF R15 relation_false_"));
    assert!(assembly.contains("MOV R10 R11"));
    assert!(assembly.contains("EQ R6 R11 R10"));
}
