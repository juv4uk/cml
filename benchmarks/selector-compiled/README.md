# #397 — compiled selector-path benchmark

## Українська

Цей стенд перевіряє вузьку гіпотезу з `cml#397`:

```text
канонічний selector root+suffix
  -> decode один раз
  -> exact CAR/CDR IR
  -> x86
  -> у runtime немає semantic-tree/path lookup
```

Семантичний закон належить SENS (#1975/#1988). CML тут не вигадує значення слова.
Поточний benchmark декодує тільки доведену selector-family: `101=CAR`, `110=CDR`, suffix `0=CAR`, `1=CDR`.

### Що перевіряє стенд

- execution parity через реальний x86 nucleus;
- emitted selector calls = `depth + 1`;
- runtime semantic-path operations = `0`;
- compile-time cost окремо: generate -> decode -> lower/emit;
- native differential: однаковий pair-tree baseline проти baseline + direct selector chain;
- object `.text` bytes та machine instruction count.

### Перший виміряний результат

Owner hardware: i5-6400 / WSL2, Valgrind 3.27.0, 3 samples, native batch 32.

Repeated-path representative rows:

| depth | steps | nested native I/eval | linear native I/eval | nested inst/step | linear inst/step | nested bytes/step | linear bytes/step |
|---:|---:|---:|---:|---:|---:|---:|---:|
| 0 | 1 | 8.344 | 7.344 | 4.0 | 3.0 | 18.0 | 11.0 |
| 4 | 5 | 44.344 | 39.344 | 4.0 | 3.0 | 19.2 | 11.0 |
| 16 | 17 | 152.344 | 135.344 | 6.0 | 3.0 | 24.0 | 11.0 |

Bounded result:

- both candidates have execution parity and zero runtime semantic-path operations;
- linear recipe removes **exactly 1 native I-ref per selector step** versus nested IR in these batched rows;
- linear static wrapper stays exactly **3 machine instructions + 11 .text bytes per step** at depth 0/4/16;
- generic nested lowering accumulates extra stack-slot machinery and grows worse at depth 16;
- compile-time lower+emit cost is not universally lower: linear has a fixed patch/recipe overhead at depth 0, but is materially cheaper by depth 4 and 16 in this harness.

This is mechanism evidence, not a semantic change and not a production cutover.

### Виявлений зайвий механізм

Generic nested-call lowering зараз матеріалізує кожне проміжне selector-value у stack slot:

```asm
movq %rax, SLOT(%rsp)
movq %r12, %rdi
movq SLOT(%rsp), %rsi
call wsm_car        # або wsm_cdr
```

Semantic path уже стертий, але generic argument machinery лишається.
Наступний falsifier — benchmark-only linear selector emitter, який тримає поточне значення у `%rax` і продовжує викликати ратифіковані `wsm_car/wsm_cdr` без дублювання їх tag/type-error семантики.

## English

This harness tests whether a proven selector root+suffix can be resolved once at compile time and emitted as direct CAR/CDR structure with zero runtime semantic-path operations.

The first slice proves erasure, not optimal code generation. Generic nested-call lowering still spills/reloads intermediate selector values; a later benchmark should compare it against a backend-local linear selector-chain emitter.

## Run

The CML-pinned `external/sens` submodule must be materialized. Set the exact nucleus path:

```bash
export WSM_NUCLEUS_ASM=/path/to/wsm-my-lisp/asm/nucleus.s
python3 benchmarks/selector-compiled/run.py --out /tmp/cml-397
```

`--smoke` uses depths 0/4/16, one Cachegrind sample and smaller phase counts.

Do not compare absolute I-ref counts directly with SENS #1988 C/Rust harnesses. The useful CML result is the internal differential: after compile-time path erasure, runtime contains no semantic-path lookup.
