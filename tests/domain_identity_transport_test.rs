use cml::ast::Expr;
use cml::ir::Ir;
use cml::lower::lower_expr;
use sens::{Bija3, Bit3, Bit4, CoreD4, CoreDomainIdentity};

fn d3(bits: u8) -> CoreDomainIdentity {
    CoreDomainIdentity::D3(Bija3::from_word(
        Bit3::new(bits).expect("valid D3 word"),
    ))
}

fn d4(bits: u8) -> CoreDomainIdentity {
    CoreDomainIdentity::D4(CoreD4::from_word(
        Bit4::new(bits).expect("valid D4 word"),
    ))
}

#[test]
fn exact_domain_identity_survives_ast_to_ir_without_translation() {
    let identity = d3(0b001);
    let lowered =
        lower_expr(&Expr::DomainIdentity(identity)).expect("lower exact-domain identity");

    assert_eq!(lowered, Ir::DomainIdentity(identity));
}

#[test]
fn equal_payloads_in_d3_and_d4_remain_distinct_through_lowering() {
    let d3_identity = d3(0b001);
    let d4_identity = d4(0b0001);

    assert_ne!(d3_identity, d4_identity);

    let d3_ir = lower_expr(&Expr::DomainIdentity(d3_identity)).expect("lower D3");
    let d4_ir = lower_expr(&Expr::DomainIdentity(d4_identity)).expect("lower D4");

    assert_ne!(d3_ir, d4_ir);
    assert_eq!(d3_ir, Ir::DomainIdentity(d3_identity));
    assert_eq!(d4_ir, Ir::DomainIdentity(d4_identity));
}
