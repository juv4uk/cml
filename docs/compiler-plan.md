# Compiler plan / План компілятора

Status: executable roadmap for turning ordinary my-lisp source into standalone native binaries. This is the compiler track for `cml`, not a side experiment.

Authoritative machine-readable queue: `compiler-tasks.my`.

---

## Українська

### 1. Мета

Компілятор має пройти шлях:

```text
my-lisp source
    ↓
macro / language normalization
    ↓
semantic IR
    ↓
portable native backend (спочатку C)
    ↓
small target runtime ABI
    ↓
cc / clang / zig cc
    ↓
standalone executable
```

Вихідний бінарник не повинен містити Rust evaluator як прихований runtime.

Головний критерій правильності:

```text
same source
   ├── native Rust evaluator
   ├── Lisp my-eval
   └── compiled executable
           ↓
      same observation
```

`my-eval` входить у gate лише для вже доведеної ним поверхні. Непідтримувана форма ніколи не вважається PASS.

### 2. Принцип архітектури

Компілятор не повинен переносити Rust runtime у C рядок за рядком. Розділяємо:

```text
LANGUAGE MEANING                  TARGET MECHANISM
----------------                 ----------------
lexical binding                  allocation
macro semantics                  object representation
cond/evaluation policy           ABI calls
error identity                   process entry/exit
library semantics                raw syscalls/capabilities
```

Що може бути визначено один раз у Lisp або semantic IR, не дублюється в кожному backend.

### 3. Базовий target

Перший нормальний target: Linux x86_64 через C backend і системний C compiler.

LLVM, JIT і direct machine code не є передумовами. Direct x86_64/freestanding і FPGA залишаються паралельними backend-ами, але не блокують portable compiler-v1.

### 4. Рубежі

#### C0 — Triple oracle

Один harness порівнює native evaluator, `my-eval` і compiled executable. Результат структурований як value/error/unsupported.

#### C1 — One-command compiler

Одна команда:

```text
cml build program.my -o program
```

виконує parse/normalize/lower/codegen/link. Debug mode може залишати C/IR для аудиту.

#### C2 — Runtime ABI v0

Винести з `c_backend.rs` стабільний малий ABI: Value, constructors, predicates, call convention, allocation, entry point, structured failure channel.

Generated program і runtime мають збиратися окремо.

#### C3 — Core values

Довести representation для `()`, symbols, proper/dotted pairs, strings, exact integers і exact rationals. Inexact values — окреме рішення. Жодної silent truncation.

#### C4 — Closures and bindings

Fixed/dotted/all-rest params, lexical capture, first-class builtins, self-recursion, mutual recursion, exact arity behavior.

#### C5 — Errors as data

Compiled runtime повертає стабільні error identities. OS/libc/compiler prose лишається detail, не semantic discriminator.

#### C6 — Macro compiler stage

Macro expansion стає явною стадією pipeline. Доведений Lisp implementation рухається до authority, Rust лишається differential oracle до cutover.

#### C7 — Compile real Lisp libraries

`core.my` та інші language-owned libraries проходять звичайний compiler pipeline. Не створювати C helper для кожної Lisp-функції лише тому, що так простіше backend-у.

#### C8 — Memory discipline

Спочатку bounded arena/region mode з явним exhaustion. GC додається лише після доказу потреби на живому corpus.

#### C9 — Host capability ABI

Files/process/time/TCP підключаються як мінімальні механізми згідно з host portability contract. Lisp-інтерпретація не повертається назад у C runtime.

#### C10 — Conformance matrix

Для кожного constitutive fixture:

```text
fixture | native | my-eval | compiled | status | reason
```

Silent skip заборонений CI.

#### C11 — Second host

Той самий corpus компілюється другим toolchain/host, бажано Windows/MinGW після Linux, без переписування semantics.

#### C12 — compiler-v1 release

Release claim допускається лише коли вся заявлена поверхня проходить executable conformance gate.

#### C13 — Compiler in Lisp

Після стабілізації pipeline окремі pure compiler stages переносяться у my-lisp: normalization/lowering first. Кожна стадія повинна бути differential-equivalent до поточного reference implementation до cutover.

### 5. Що не є блокером compiler-v1

- LLVM backend;
- JIT;
- full self-hosted compiler;
- generational/moving GC;
- full OS boot;
- GPU/FPGA parity;
- aggressive optimization.

Ці речі можна розвивати пізніше, але вони не повинні затримувати перший надійний standalone compiler.

### 6. Definition of done

Compiler-v1 готовий, коли CI автоматично виконує:

```text
source
 → normalize/lower
 → generated target code
 → native executable
 → execute
 → structured observation
 → compare with language oracle
```

для всієї заявленої поверхні без silently skipped fixtures.

Сильне допустиме формулювання:

> CML compiles the declared my-lisp compiler-v1 surface to standalone native executables, and every admitted conformance fixture is differentially checked against the reference semantics.

Не сильніше.

---

## English

### Goal

Make `cml` a measured native compiler for my-lisp: source -> normalization -> semantic IR -> portable native backend -> small target runtime ABI -> standalone executable, without embedding the Rust evaluator.

Correctness is differential. The same source is observed through the native evaluator, the Lisp metacircular evaluator wherever supported, and the compiled executable. Unsupported is explicit and never counted as pass.

### Architecture

Keep language meaning in Lisp/semantic IR and target mechanism in the runtime adapter. C is the first portable native backend; LLVM and JIT are not prerequisites.

### Milestones

C0 triple-oracle harness; C1 one-command build; C2 runtime ABI v0; C3 core values; C4 closure/binding parity; C5 structured errors; C6 macro compiler stage; C7 compile real Lisp libraries; C8 bounded memory then GC only if evidence requires it; C9 portable host capability ABI; C10 no-silent-skip conformance matrix; C11 second-host proof; C12 compiler-v1 release gate; C13 move pure compiler stages into Lisp under differential proof.

### Definition of done

CI must perform source -> lower -> target code -> executable -> run -> structured observation -> oracle comparison for every admitted fixture. Release wording must remain no stronger than the executable evidence.
