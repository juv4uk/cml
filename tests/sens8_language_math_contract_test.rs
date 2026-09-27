//! SID8 language contract (owner directive 2026-09-24, mirroring my-lisp
//! `sid.rs`): the whole `00000000..11111111` space is language function
//! identity -- exactly eight bare binary digits in, a function identity out.
//! Math over a SID cannot compile; the SID value is never lost as an 8-bit
//! binary. These two tests exercise the global semantic gate that runs for
//! every backend before IR lowering.

use cml::ast::Expr;
use cml::ir::Ir;
use cml::{lower, parser};

/// A SID may never appear as the numeric operand of an arithmetic or
/// ordering operation, whatever surface (or direct SID8 head) names the
/// operation. This is the compiler-wide gate: it lives in the semantic
/// admission layer that every backend lowers through.
#[test]
fn math_operations_over_sid_are_forbidden_for_the_whole_compiler() {
    // Every registered math/ordering operation, plus the "=" numeric
    // equality -- id = 00011100 -- over a SID literal operand.
    let sources = [
        // direct math heads with SID literal operand
        "(+ 00000101 1)",
        "(+ 1 00000101)",
        "(- 00000101 1)",
        "(* 00000101 2)",
        "(mod 00000101 2)",
        "(quotient 00000101 2)",
        "(< 00000101 00000110)",
        "(> 00000101 00000110)",
        "(<= 00000101 00000110)",
        "(>= 00000101 00000110)",
        "(= 00000101 00000110)",
        // SID8 heads naming the same operations with SID literal operand
        "(00001100 00000101 1)",
        "(00001101 00000101 1)",
        "(00001110 00000101 2)",
        "(00010011 00000101 2)",
        "(00010100 00000101 2)",
        "(00011010 00000101 00000110)",
        "(00011011 00000101 00000110)",
        "(00011100 00000101 00000110)",
        "(00011101 00000101 00000110)",
        "(00011110 00000101 00000110)",
    ];

    for source in sources {
        let expressions = parser::parse(source).expect("source must parse");
        let result = lower::lower_program(&expressions);
        assert!(
            matches!(
                result,
                Err(cml::lower::LowerError {
                    kind: cml::lower::LowerErrorKind::Semantic,
                    ..
                })
            ),
            "{source}: math over SID must fail the semantic gate, got {:?}",
            result.map(|_| ())
        );
    }
}

/// The SID value keeps its exact eight-bit binary form end to end, across
/// the entire closed `00000000..11111111` domain: source token -> parser
/// -> Expr::Sid -> lower -> Ir::Sid, with the rendered spelling identical to
/// the token at every step. Proven exhaustively for all 256 values, like
/// my-lisp's `every_possible_byte_round_trips_through_sid8_without_loss`.
#[test]
fn sid8_value_keeps_exact_eight_bit_binary_form_across_the_full_domain() {
    for byte in 0u16..=255 {
        let byte = byte as u8;
        let spelling = format!("{byte:08b}");

        let expressions = parser::parse(&spelling).expect("any SID spelling must parse");
        assert_eq!(
            expressions.len(),
            1,
            "{spelling}: bare SID token must parse as exactly one expression"
        );

        let Expr::Sid(sid) = &expressions[0] else {
            panic!(
                "{spelling}: token must be a typed SID8, got {:?}",
                expressions[0]
            );
        };
        assert_eq!(
            sid.to_string(),
            spelling,
            "{spelling}: SID spelling must survive parsing exactly"
        );

        let mut lowered = lower::lower_program(&expressions).expect("bare SID must lower");
        assert_eq!(lowered.len(), 1, "{spelling}: lower must keep one IR node");
        let ir_sid = lowered.remove(0);
        match ir_sid {
            Ir::Sid(ir_sid) => assert_eq!(
                ir_sid.to_string(),
                spelling,
                "{spelling}: SID spelling must survive lowering exactly"
            ),
            other => panic!("{spelling}: must lower to Ir::Sid, got {other:?}"),
        }
    }
}
