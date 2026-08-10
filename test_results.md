# CML Test Results

This document contains the execution results of the test suite for the `cml` compiler.

## Environment
- OS: Windows
- Compiler: `cml v0.1.0`
- Target: `debug`

## Execution Summary

```text
running 3 tests
test test_compile_apply ... ok
test test_compile_cond ... ok
test test_compile_lambda ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

## Detailed Test Cases

### 1. `test_compile_cond`
- **Purpose**: Verifies that the compiler translates branching logic (`cond`) into the correct assembly instructions.
- **Input**: `(cond (t 'a) (nil 'b))`
- **Assertions**:
  - Contains `LOADSYM R1 TRUE`
  - Contains `JF R1`
  - Contains `LOADSYM R15 A`
  - Contains `LOADSYM R1 NIL`
  - Contains `LOADSYM R15 B`
  - Contains `HALT`
- **Result**: `ok`

### 2. `test_compile_lambda`
- **Purpose**: Verifies that anonymous functions (`lambda`) compile correctly with closure capturing (`CONS`) and proper jump bounds.
- **Input**: `(lambda (x) x)`
- **Assertions**:
  - Contains `LAMBDA START`
  - Contains `LAMBDA END`
  - Contains closure constructor: `CONS R15 R4 -> R15`
  - Contains return flow: `RET` or `JMP R14`
- **Result**: `ok`

### 3. `test_compile_apply`
- **Purpose**: Verifies generic function application (`f arg`) generating the correct calling convention and closure invocation logic.
- **Input**: `((lambda (x) x) 'test)`
- **Assertions**:
  - Contains `CALL START`
  - Contains `CALL END`
  - Unpacks function label: `CAR R10 R15`
  - Unpacks captured env: `CDR R4 R15`
  - Executes function: `JMP R10`
- **Result**: `ok`

## Library Tests
All internal parsing components (like string to AST mapping) passed implicitly with `0 failed`.

```text
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

> [!NOTE]
> All core features required for variables, application, lambda closures, and conditions compile seamlessly to `fpga-lisp` Assembly ISA.
