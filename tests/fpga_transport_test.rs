use cml::fpga_transport::{
    CommandFpgaTransport, FpgaJobExecutor, FpgaJobV1, FpgaProtocolError, FpgaResultV1,
    FpgaTransport, MAX_PROGRAM_WORDS, MONITOR_ERROR,
};

#[test]
fn job_v1_matches_the_real_uart_bootloader_frame() {
    let job = FpgaJobV1 {
        program_words: vec![0x1122_3344, 0xaabb_ccdd],
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
fn invalid_jobs_fail_before_transport_side_effects() {
    assert_eq!(
        FpgaJobV1 {
            program_words: vec![],
            result_register: 9,
        }
        .validate(),
        Err(FpgaProtocolError::EmptyProgram)
    );
    assert_eq!(
        FpgaJobV1 {
            program_words: vec![0; MAX_PROGRAM_WORDS + 1],
            result_register: 9,
        }
        .validate(),
        Err(FpgaProtocolError::ProgramTooLong(MAX_PROGRAM_WORDS + 1))
    );
    assert_eq!(
        FpgaJobV1 {
            program_words: vec![0],
            result_register: 16,
        }
        .validate(),
        Err(FpgaProtocolError::InvalidResultRegister(16))
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
            result_register: 9,
        })
        .unwrap();
    assert_eq!(result, FpgaResultV1::from_monitor_words(7, 0));
}

#[test]
fn command_transport_rejects_unversioned_or_truncated_responses() {
    let helper = "import sys; sys.stdin.buffer.read(); sys.stdout.buffer.write(b'bad')";
    let mut transport = CommandFpgaTransport::new("python3", vec!["-c".into(), helper.into()]);
    let error = transport
        .execute(&FpgaJobV1 {
            program_words: vec![0],
            result_register: 0,
        })
        .unwrap_err();
    assert!(matches!(error, FpgaProtocolError::Transport(_)));
}
