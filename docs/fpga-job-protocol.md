# CML to fpga-lisp job protocol

**Status:** M2a host contract, 2026-08-24.  
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
publication. Physical COM4 transport remains pending and is not inferred from
the earlier manual monitor pass.
