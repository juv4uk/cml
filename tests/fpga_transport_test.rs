use cml::fpga_transport::{
    CommandFpgaTransport, FpgaJobExecutor, FpgaJobV1, FpgaProtocolError, FpgaRegisterInput,
    FpgaResultV1, FpgaTransport, MAX_PROGRAM_WORDS, MAX_REGISTER_INPUTS, MONITOR_ERROR,
    encode_i32_buffer_as_register_inputs,
};
use cml::ir::BufferLiteral;

#[test]
fn job_v1_matches_the_real_uart_bootloader_frame() {
    let job = FpgaJobV1 {
        program_words: vec![0x1122_3344, 0xaabb_ccdd],
        register_inputs: vec![],
        result_register: 9,
    };
    assert_eq!(
        job.bootloader_frame().unwrap(),
        vec![2, 0, 0x44, 0x33, 0x22, 0x11, 0xdd, 0xcc, 0xbb, 0xaa]
    );
    assert_eq!(job.result_query().unwrap(), [0x01, 9]);
    assert_eq!(MONITOR_ERROR, 0x04);
}

#[test]
fn job_v1_encodes_isa_1_1_tagged_register_inputs() {
    let job = FpgaJobV1 {
        program_words: vec![0xd201_0000, 0xb000_0000],
        register_inputs: vec![
            FpgaRegisterInput {
                register: 0,
                tagged_word: 3,
            },
            FpgaRegisterInput {
                register: 1,
                tagged_word: 4,
            },
        ],
        result_register: 2,
    };
    assert_eq!(
        job.bootloader_frame().unwrap(),
        vec![
            0x02, 0x80, 0x02, 0x00, 0x03, 0x00, 0x00, 0x00, 0x01, 0x04, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x01, 0xd2, 0x00, 0x00, 0x00, 0xb0,
        ]
    );
}

#[test]
fn typed_i32_buffer_materializes_only_as_checked_fixnum_inputs() {
    let inputs =
        encode_i32_buffer_as_register_inputs(&BufferLiteral::I32(vec![-1, 3, 4]), 0).unwrap();
    assert_eq!(
        inputs,
        vec![
            FpgaRegisterInput {
                register: 0,
                tagged_word: 0x0fff_ffff
            },
            FpgaRegisterInput {
                register: 1,
                tagged_word: 3
            },
            FpgaRegisterInput {
                register: 2,
                tagged_word: 4
            },
        ]
    );
}

#[test]
fn typed_buffer_adapter_fails_closed_for_float_range_and_register_overflow() {
    assert_eq!(
        encode_i32_buffer_as_register_inputs(&BufferLiteral::I32(vec![]), 0),
        Err(FpgaProtocolError::EmptyInputBuffer)
    );
    assert_eq!(
        encode_i32_buffer_as_register_inputs(&BufferLiteral::F32(vec![0x3f80_0000]), 0),
        Err(FpgaProtocolError::UnsupportedInputBuffer)
    );
    assert_eq!(
        encode_i32_buffer_as_register_inputs(&BufferLiteral::I32(vec![1 << 27]), 0),
        Err(FpgaProtocolError::InputValueOutOfRange {
            index: 0,
            value: 1 << 27,
        })
    );
    assert_eq!(
        encode_i32_buffer_as_register_inputs(&BufferLiteral::I32(vec![1, 2]), 15),
        Err(FpgaProtocolError::RegisterRange {
            first: 15,
            count: 2
        })
    );
}

#[test]
fn invalid_jobs_fail_before_transport_side_effects() {
    assert_eq!(
        FpgaJobV1 {
            program_words: vec![],
            register_inputs: vec![],
            result_register: 9,
        }
        .validate(),
        Err(FpgaProtocolError::EmptyProgram)
    );
    assert_eq!(
        FpgaJobV1 {
            program_words: vec![0; MAX_PROGRAM_WORDS + 1],
            register_inputs: vec![],
            result_register: 9,
        }
        .validate(),
        Err(FpgaProtocolError::ProgramTooLong(MAX_PROGRAM_WORDS + 1))
    );
    assert_eq!(
        FpgaJobV1 {
            program_words: vec![0],
            register_inputs: vec![],
            result_register: 16,
        }
        .validate(),
        Err(FpgaProtocolError::InvalidResultRegister(16))
    );
    assert_eq!(
        FpgaJobV1 {
            program_words: vec![0],
            register_inputs: vec![
                FpgaRegisterInput {
                    register: 0,
                    tagged_word: 0,
                };
                MAX_REGISTER_INPUTS + 1
            ],
            result_register: 0,
        }
        .validate(),
        Err(FpgaProtocolError::TooManyRegisterInputs(
            MAX_REGISTER_INPUTS + 1
        ))
    );
    assert_eq!(
        FpgaJobV1 {
            program_words: vec![0],
            register_inputs: vec![
                FpgaRegisterInput {
                    register: 2,
                    tagged_word: 3
                },
                FpgaRegisterInput {
                    register: 2,
                    tagged_word: 4
                },
            ],
            result_register: 0,
        }
        .validate(),
        Err(FpgaProtocolError::DuplicateInputRegister(2))
    );
}

#[test]
fn monitor_words_preserve_hardware_error_and_signed_fixnum_semantics() {
    assert_eq!(
        FpgaResultV1::from_monitor_words(7, 0)
            .checked_fixnum()
            .unwrap(),
        7
    );
    assert_eq!(
        FpgaResultV1::from_monitor_words(0x0fff_ffff, 0)
            .checked_fixnum()
            .unwrap(),
        -1
    );
    assert_eq!(
        FpgaResultV1::from_monitor_words(7, 0x1000 | 23).checked_fixnum(),
        Err(FpgaProtocolError::HardwareError { pc: 23 })
    );
}

struct WitnessTransport {
    seen_frame: Option<Vec<u8>>,
    result: FpgaResultV1,
}

impl FpgaTransport for WitnessTransport {
    fn execute(&mut self, job: &FpgaJobV1) -> Result<FpgaResultV1, FpgaProtocolError> {
        self.seen_frame = Some(job.bootloader_frame()?);
        Ok(self.result)
    }
}

#[test]
fn executor_uses_transport_without_owning_serial_or_device_policy() {
    let transport = WitnessTransport {
        seen_frame: None,
        result: FpgaResultV1::from_monitor_words(7, 0),
    };
    let mut executor = FpgaJobExecutor::new(transport);
    let result = executor
        .execute_fixnum(&FpgaJobV1 {
            program_words: vec![0xf000_0000],
            register_inputs: vec![],
            result_register: 9,
        })
        .unwrap();
    assert_eq!(result, 7);
}

#[test]
fn command_transport_streams_a_versioned_job_without_a_shell_or_temp_file() {
    let helper = r#"
import struct, sys
request = sys.stdin.buffer.read()
assert request[:4] == b'CMLJ'
assert struct.unpack('<H', request[4:6])[0] == 1
assert request[6] == 9
assert request[7] == 0
assert request[8:] == struct.pack('<HII', 2, 0x11223344, 0xaabbccdd)
sys.stdout.buffer.write(b'CMLR' + struct.pack('<HII', 1, 7, 0))
"#;
    let mut transport = CommandFpgaTransport::new("python3", vec!["-c".into(), helper.into()]);
    let result = transport
        .execute(&FpgaJobV1 {
            program_words: vec![0x1122_3344, 0xaabb_ccdd],
            register_inputs: vec![],
            result_register: 9,
        })
        .unwrap();
    assert_eq!(result, FpgaResultV1::from_monitor_words(7, 0));
}

#[test]
fn command_transport_streams_extended_register_inputs() {
    let helper = r#"
import struct, sys
request = sys.stdin.buffer.read()
assert request[:8] == b'CMLJ' + struct.pack('<HBB', 1, 2, 0)
expected = (struct.pack('<HB', 0x8002, 2)
            + struct.pack('<BI', 0, 3)
            + struct.pack('<BI', 1, 4)
            + struct.pack('<II', 0xd2010000, 0xb0000000))
assert request[8:] == expected
sys.stdout.buffer.write(b'CMLR' + struct.pack('<HII', 1, 7, 0))
"#;
    let mut transport = CommandFpgaTransport::new("python3", vec!["-c".into(), helper.into()]);
    let result = transport
        .execute(&FpgaJobV1 {
            program_words: vec![0xd201_0000, 0xb000_0000],
            register_inputs: vec![
                FpgaRegisterInput {
                    register: 0,
                    tagged_word: 3,
                },
                FpgaRegisterInput {
                    register: 1,
                    tagged_word: 4,
                },
            ],
            result_register: 2,
        })
        .unwrap();
    assert_eq!(result.checked_fixnum().unwrap(), 7);
}

#[test]
fn command_transport_rejects_unversioned_or_truncated_responses() {
    let helper = "import sys; sys.stdin.buffer.read(); sys.stdout.buffer.write(b'bad')";
    let mut transport = CommandFpgaTransport::new("python3", vec!["-c".into(), helper.into()]);
    let error = transport
        .execute(&FpgaJobV1 {
            program_words: vec![0],
            register_inputs: vec![],
            result_register: 0,
        })
        .unwrap_err();
    assert!(matches!(error, FpgaProtocolError::Transport(_)));
}
