#![cfg(feature = "core1-t5-candidate")]
//! Test-only candidate SENS exact two-codec source -> CURRENT verified CML IR.
//! This never changes the production SENS gitlink or adopts a new CML opcode.
//! Run with an independently checked-out exact SENS candidate SHA and
//! SENS_COMPILER_EXPORT_FILE from that *same* candidate checkout.
use cml::sens_current_lowering::lower_current_sens_binary_projection;
use std::{fs, path::PathBuf};

const TESTED_SENS_SHA: &str = "390f313242b538cb954dc00afa32ef0003f7d94b";
const QUOTE_T5: [u8; 4] = [0x63, 0x89, 0x06, 0xa1];
const QUOTE_F3: [u8; 3] = [0xf3, 0x40, 0x60];

fn candidate_export() -> String {
    let path = std::env::var("SENS_COMPILER_EXPORT_FILE")
        .expect("BLOCK: exact candidate SENS compiler-export path missing");
    fs::read_to_string(path).expect("BLOCK: candidate compiler export unreadable")
}

fn candidate_fixture() -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("external/sens/tests/fixtures/migration-quote-cohort-main/quote-legacy.sens");
    let physical = fs::read(path).expect("BLOCK: missing physical SENS fixture");
    assert_eq!(physical.as_slice(), QUOTE_T5, "BLOCK: pinned SENS T5 fixture changed");
    physical
}

#[test]
fn verified_tiny_f3_transport_and_execution_without_compiler_authority() {
    let raw = candidate_fixture();
    let framed = sens::frame3_small::encode_explicit_small_f3(&raw)
        .expect("original F3 encode must validate the exact SENS D2/D3")
        .expect("tiny physical F3 saves one real byte");
    assert_eq!(framed, QUOTE_F3);
    let back = sens::frame3_small::decode_explicit_small_f3(&framed)
        .expect("strict original F3 requires exact carrier and SENS canonical reader");
    assert_eq!(raw, back);
    let baseline = sens::PhysicalT5Program::decode(&raw)
        .unwrap().execute(&mut sens::Session::default()).unwrap();
    let again = sens::PhysicalT5Program::decode(&back)
        .unwrap().execute(&mut sens::Session::default()).unwrap();
    assert_eq!(baseline.value, again.value);
    assert_eq!(baseline.output, again.output);
    println!("CML_CANDIDATE_F3_TRANSPORT_PASS sens_sha={TESTED_SENS_SHA} t5=4 f3=3 exact_eval=true");
}

#[test]
fn original_frame3_and_t5_lower_into_same_verified_cml_ir() {
    let baseline = candidate_fixture();
    let framed = sens::frame3_small::encode_explicit_small_f3(&baseline)
        .expect("F3 encoder must validate canonical source")
        .expect("4B QUOTE should admit strictly smaller F3");
    assert_eq!(framed.as_slice(), QUOTE_F3, "BLOCK: Frame3 physical identity drift");
    assert!(framed.len() < baseline.len());
    let restored = sens::frame3_small::decode_explicit_small_f3(&framed)
        .expect("F3 to the SAME canonical T5, no second CML parser");
    assert_eq!(baseline, restored, "BLOCK: exact word/transport roundtrip drift");
    let baseline_program = sens::PhysicalT5Program::decode(&baseline).unwrap();
    let framed_program = sens::PhysicalT5Program::decode(&restored).unwrap();
    let left = baseline_program.execute(&mut sens::Session::default())
        .expect("SENS original T5 execution");
    let right = framed_program.execute(&mut sens::Session::default())
        .expect("SENS F3 reconstructed execution");
    assert_eq!(left.value, right.value);
    assert_eq!(left.output, right.output);
    // CML does not own D2/D3 semantics. It sees one SENS-verified exact
    // canonical source, a verified compiler export and the same IR.
    let projection_a = sens::open_ternary_program(&baseline)
        .expect("canonical T5 reader");
    let projection_b = sens::open_ternary_program(&restored)
        .expect("reconstructed F3 via canonical T5 reader");
    assert_eq!(projection_a, projection_b);
    let authority = candidate_export();
    let ir_a = lower_current_sens_binary_projection(&projection_a, &authority)
        .expect("SENS verified CML D3 mechanism/quote lowering");
    let ir_b = lower_current_sens_binary_projection(&projection_b, &authority)
        .expect("same exact verified SENS mechanism after F3");
    assert_eq!(ir_a.authority, ir_b.authority);
    assert_eq!(ir_a.ir, ir_b.ir);
    assert_eq!(ir_a.ir.len(), 1);
    eprintln!(
        "CML_TWO_CODEC_BRIDGE_PASS sens_sha={TESTED_SENS_SHA} t5_bytes={} f3_bytes={} identical_ir=true identical_eval=true",
        baseline.len(), framed.len()
    );
}

#[test]
fn bad_or_ineligible_f3_never_reaches_cml_ir() {
    for carrier in [
        vec![], vec![0xf3], vec![0xf4, 0x40, 0x60],
        vec![0x63, 0x89, 0x06, 0xa1], vec![0xf3; 16],
    ] {
        assert!(
            sens::frame3_small::decode_explicit_small_f3(&carrier).is_err(),
            "BLOCK: unsafe tiny F3 admitted: {carrier:02x?}"
        );
    }
    assert!(sens::PhysicalT5Program::decode(&QUOTE_F3).is_err());
}
