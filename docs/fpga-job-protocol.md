# CML to fpga-lisp job protocol

**Status:** M2c command transport, 2026-08-24.
**Wire authority:** `fpga-lisp` ISA 1.0 RTL and monitor protocol.

`FpgaJobV1` describes one already-assembled program image and the register
whose tagged Lisp word is the result. Version 1 deliberately mirrors the live
board instead of inventing a new packet:

```text
bootloader: u16-le instruction count + N u32-le instructions
wait:       board reaches HALT
monitor:    0x01 register -> u32-le tagged result word
monitor:    0x04          -> u32-le { error_flag bit 12, error_pc bits 11:0 }
```

The host validates `1 <= N <= 4095` and register `0..15` before transport side
effects. A successful FIXNUM result is decoded as signed 28-bit two's
complement. Hardware error, unexpected tag, transport failure, invalid job,
and timeout remain distinct outcomes.

`FpgaTransport` owns reset, serial-port access, HALT waiting, framing, exact
reads, and timeouts. CML semantics and the Execution Graph do not know whether
the implementation uses Windows COM4, native Linux serial, simulation, or a
future PCIe transport.

M2b attaches this executor to `ExecutionGraph` through two explicit value
variants: `GraphValue::Buffer` and `GraphValue::LispWord`. `FpgaProgram` emits
only the latter; numeric buffer maps consume and emit only the former. A mock
transport proves graph scheduling, tagged-word preservation, and atomic error
publication.

M2c adds `CommandFpgaTransport`, a shell-free process boundary that writes one
versioned binary job to stdin and accepts one fixed-size binary response from
stdout. The companion `fpga-lisp/job_transport.py` runs under native Windows
Python/pyserial, owns COM4 and timing policy, and preserves the monitor tool's
delayed input-buffer reset workaround. This avoids a Windows command-line
length ceiling and temporary files while keeping serial dependencies outside
CML's language/compiler core.

The ignored live graph test is intentionally operator-gated because the board
still requires a physical RESET press:

```bash
CML_FPGA_LIVE=1 \
CML_FPGA_PYTHON=/mnt/c/Users/user/AppData/Local/Programs/Python/Launcher/py.exe \
CML_FPGA_BRIDGE_WINDOWS='\\wsl.localhost\Ubuntu\home\agents\GitHub\fpga-lisp\job_transport.py' \
cargo test --test execution_graph_fpga_live_test -- --ignored --nocapture
```

Windows PnP currently reports USB Serial Converter B and COM4 healthy. That is
device-presence evidence only; the M2c live graph result remains pending until
the reset-gated test returns R9 = FIXNUM(7).
