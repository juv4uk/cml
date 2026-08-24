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

Windows PnP reports USB Serial Converter B and COM4 healthy. On 2026-08-24 the
reset-gated test then completed the stronger proof: the registered
`fpga-lisp:com4` executor uploaded `bootstrap_add_demo.bin` to the physical
GW5A-25A board, read R9 as raw tagged word `0x00000007`, observed no hardware
error, and published `GraphValue::LispWord(7)`. The ignored test passed in
13.37 seconds. This is one live program-path observation, not blanket FPGA
backend conformance.

The checked-in fpga-lisp reference fixture corpus follows upstream my-lisp
contract 3.0 so unsupported cases remain visible. That reference pin is not a
capability claim: CML and its FPGA backend still declare contract 2.0 until
named-error conformance exists for every required backend path.

## M3 heterogeneous live graph

One operator-gated graph executed three ordered physical/runtime domains in
14.61 seconds: CPU mapped `[1,2,3]` to `[2,3,4]`, the GTX 1050 Ti CUDA node
mapped that buffer to `[3,4,5]`, then the COM4 FPGA node returned tagged word
`7`. `execution_order()` was exactly CPU → CUDA → FPGA.

This proves one scheduler, dependency graph, backend registry, and atomic
result store can coordinate all three domains. It does **not** prove direct
GPU-to-FPGA data movement: the dependency currently carries ordering only and
the FPGA node executes its own preassembled program image. A typed transfer
edge or shared-memory transport is the next distinct capability.

## M4 data and control edges

Graph validation now rejects a consumed buffer unless it is either a declared
graph input or has a real producer that is also named as a dependency of the
consumer. Source order can no longer accidentally masquerade as data flow.

For current numeric nodes, the producer's typed `GraphValue::Buffer` is
materialized in the host value store before the next executor receives it;
CPU→CUDA is therefore a host-staged data edge. The CUDA→FPGA dependency in the
M3 proof is control-only because `FpgaProgram` has no buffer input. A future
FPGA payload edge must introduce an explicit typed input protocol rather than
reinterpreting this dependency.
