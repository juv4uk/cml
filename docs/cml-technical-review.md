# Технічний огляд `CML`

**Автор огляду:** Manus AI  
**Стан джерел:** гілка `master`, переглянута 18 серпня 2026 року  
**Репозиторій:** [juv4uk/cml][1]

## Висновок у двох реченнях

`CML` уже не виглядає як «генератор асемблера для одного demo». Це **AOT-компіляторний міст** між language contract `my-lisp`, машинним contract `fpga-lisp` і, віднедавна, другим C target: він перетворює одні й ті самі source semantics на різні execution backends та перевіряє FPGA-result через blind end-to-end adapter [2] [3].

Найсильніше тут не саме lowering у `CONS`/`CAR`/`JF`, а дисципліна меж: pinned compatibility record, version drift guard, macro expansion before lowering, structured result decoder і чітко зафіксовані representational gaps target-а.

> **CML — це місце, де `my-lisp` перестає бути лише runtime-ом і стає мовним контрактом, з якого можна будувати різні обчислювальні реалізації.**

## 1. Архітектурна роль

Первинний target CML — не interpreter на FPGA. Компілятор робить host-side роботу заздалегідь, генерує fixed `fpga-lisp` assembly/image, а FPGA виконує machine code через власний bootloader, control FSM та heap. Це прибирає runtime `eval/apply` overhead там, де програма вже відома, але не скасовує Lisp substrate `fpga-lisp` [1] [4].

```text
my-lisp source + language contract
             │
             ▼
      parser / macro expansion
             │
             ▼
     backend-neutral IR (Ir)
        ┌────┴────────┐
        ▼             ▼
fpga-lisp assembly   C source
        │             │
 assembler + UART    gcc + run
        ▼             ▼
 fpga-lisp RTL      C runtime
        │             │
        └── canonical result comparison ──┘
```

| Шар | Реальна відповідальність | Джерело істини |
|---|---|---|
| `my-lisp` | Мовна семантика та `conformance.my` fixtures | `language-contract.my`, fixtures |
| CML frontend | Reader, macro expansion, AST → IR | `parser.rs`, `macros.rs`, `lower.rs` |
| CML FPGA backend | Register/stack/environment discipline та assembly emission | `compiler.rs`, `docs/abi.md` |
| `fpga-lisp` | ISA, UART image protocol, execution, heap result | `isa-contract.my`, RTL |
| C backend | Незалежний generated-C execution path | `c_backend.rs` |

`compatibility.my` добре фіксує цю межу: CML v0.3.0 заявляє language contract 2.0, fpga-lisp ISA 1.0, tested SHAs, pipeline `parse → macro-expand → compile → assemble → simulate → compare`, supported surface і known target limitations [2]. Це робить сумісність перевірюваною претензією, а не формулою в README.

## 2. Frontend і IR: правильний напрямок від monolithic compiler

Production CLI читає файл, запускає `MacroExpander`, lower-ить програму у `Vec<Ir>`, а вже потім передає IR до `Compiler::compile` [5]. Тобто IR справді вже стоїть на live path FPGA compilation, а C backend споживає той самий `Ir` [5] [6].

| IR форма | Семантична роль | FPGA lowering |
|---|---|---|
| `Int`, `Nil`, `True`, `Quote` | Literal/data layer | `LOADI`/`LOADSYM`/CONS-built structure |
| `Var` | Lexical/global lookup | `cml_lookup` over alist environment |
| `Lambda`, `App` | Closures and call | label pointer + captured env + register/R11 protocol |
| `Cond` | Control flow | labels + NIL-only branch semantics |
| `Let` | Derived syntax | immediately applied lambda |
| `Def` | Top-level self recursion | placeholder pair + `SETCDR` backpatch |
| `Prim` | Closed primitive surface | direct target-specific emission |

Це хороший поділ. `let` не отримує hardware primitive тільки для зручності компілятора; він лишається lambda application. `defmacro` не опускається на FPGA взагалі: macros collect top-level definitions, bind raw unevaluated call-site ASTs, evaluate a small tree-walking meta-language та recursively re-expand output [2]. Це тримає compile-time semantics на frontend boundary.

Один малий documentation cleanup вже назрів: header `ir.rs` досі говорить, що «нічого не споживає IR», хоча `main.rs` давно маршрутизує normal compile pipeline через `lower_program`, а C backend безпосередньо працює з `Ir` [5] [6]. Код тут уже попередив prose.

## 3. FPGA emitter: це компілятор з ABI, а не printer інструкцій

`compiler.rs` демонструє найважливіше: emission не може бути наївним, бо target має 16 registers, R11 software stack, R4 environment, R14 link, R15 return value та nested computations, які легко затирають scratch state [7].

| Механізм | Що захищає | Чому потрібний |
|---|---|---|
| `push` / `pop` на R11 | Значення між nested expression calls | Primitive emission використовує fixed scratch registers. |
| `preserve_across` | Попередньо обчислений аргумент | Наступний аргумент може знищити R1–R3. |
| `call_subroutine` | Return address у R14 | `cml_lookup` і `cml_equal` themselves return through R14. |
| `R0` list | Повний evaluated argument list | Dotted/bare-symbol lambda params не можуть жити лише в registers. |
| Closure pair | `(label-pointer . captured-env)` | Indirect `RET` jumps in body, lexical env відновлюється caller-ом. |

Це добре видно в generic call lowering: кожен argument одразу push-иться на R11, function expression обчислюється, arguments pop-яться у calling registers, потім CML будує structural list у R0, зберігає current environment і link, дістає label/captured env із closure та викликає code body [7]. Це реальний compiler ABI, а не поверхнева трансляція syntax.

### `def` і self recursion

`compile_def` перевикористовує the same foundational idea, яку довів `fpga-lisp`: до compilation value створюється placeholder `(name . NIL)`, environment розширюється ним, lambda захоплює вже розширений frame, а потім `SETCDR` backpatch-ить value [2] [7]. Так підтримується self recursion і посилання на earlier defs; дві взаємно-рекурсивні defs, де перша має викликати ще не оголошену другу, поки лишаються за межами через відсутність two-pass forward declaration [2].

### `equal?` без рекурсивного call/ret

Цікавий вибір — CML не залежить від self-recursive closure для structural equality. Він emit-ить `cml_equal` як iterative worklist із пар `(a . b)` на R11; worklist exhaust-иться навіть після mismatch, щоб stack state був balanced для caller [7]. Це зменшує залежність compiler feature від рекурсивного mechanism target-а й дає окремий доказ `equal?`.

## 4. Blind E2E conformance: найпереконливіша частина

`tests/conformance_test.rs` — не набір bespoke demos. Він читає shared `my-lisp/tests/fixtures/conformance.my`, запускає один фіксований pipeline та не має fixture-specific compiler branches [3] [8].

```text
fixture line
  → parse
  → macro expand
  → classify static error, if visible
  → AST → IR
  → fpga-lisp assembly
  → fpga assembler → .bin
  → UART load into tb_cml_e2e
  → RESULT_TAG / RESULT_VAL / RESULT_ERROR / HEAP
  → canonical Lisp decoder
  → expected or error comparison
```

Decoder повертає atoms, fixnums, proper lists і dotted lists із machine heap; він також explicitly detects cyclic heap shapes instead of looping forever [8]. Це важлива деталь: E2E harness перевіряє не тільки R15 scalar, а форму створених Lisp data structures.

Статичні arity/unknown-symbol errors класифікуються frontend-ом; runtime Type results повертаються з FPGA через structured error channel. Це правильне розділення: не намагатися штучно вигадати machine runtime, коли form може бути відхилена семантично до compilation [3] [8].

## 5. Compatibility discipline

`revision_contract_test.rs` показує зрілий погляд на живу екосистему. Test робить hard failure, якщо constants усередині CML не узгоджені з `compatibility.my`, якщо fpga-lisp перестав заявляти ISA 1.0/NIL-only JF semantics, або якщо claimed my-lisp language-contract version відрізняється від actual `language-contract.my` [9].

Водночас checked-out sibling SHA movement — лише informational note, а не failure. Це правильно: SHA drift у системі, де репозиторії рухаються незалежно, не тотожний semantic break. CML не плутає «минув час» із «контракт став хибним» [9].

Особливо корисний приклад — reader migration: після my-lisp contract 2.0 leading apostrophe перестав бути quote sugar і став identifier character. CML parser був оновлений до explicit `(quote ...)`, а contract-version test тепер не дає старій compatibility claim вижити непомічено [2] [9].

## 6. Поточні межі — чіткі, але їх треба зробити compile-time visible

| Межа | Стан | Рекомендація |
|---|---|---|
| Inexact/rational numbers | Target representation їх не має; relevant fixtures skip-яться | Залишати skip explicit до появи target tags/contracts. |
| Strings | Source string нижчиться до target symbol | Вважати це representational substitution, не повною string support. |
| Generic call arity | Реально зберігаються лише перші 8 args; extras не відхиляються | **Додати explicit compile error** до початку emission. |
| Integer literal range | `Ir::Int` — `i64`, але `LOADI` має 16-bit zero-extended immediate; negatives emulated through SUB | **Додати range validation або multiword constant synthesis**; не допускати silent truncation. |
| Mutual recursion | Self-recursion працює через one placeholder; forward mutual pair не реалізована | Додати declaration pass лише коли з’явиться реальний source use case. |
| Macro hygiene | Немає `gensym`/hygiene | Чесно documented; не робити implicit promise of Scheme-like macro system. |
| C backend Type errors | C helpers для `car`/`cdr` не спершу dynamic-check tag | Не використовувати C target як oracle runtime-error parity, доки не буде guards. |

Є також одна хороша маленька оптимізація. `compile_cond` все ще коментує, що fpga-lisp JF нібито робить `0` false, та генерує explicit NIL comparison навколо кожної clause [7]. Але ISA 1.0 вже зафіксував JF as NIL-only [2] [9]. Code is semantics-preserving, але comment stale, а direct `JF` on evaluated result може зменшити generated program size — особливо важливо у 4096-word target imem.

## 7. Як я бачу системну цінність CML

| Без CML | З CML |
|---|---|
| `my-lisp` — canonical runtime, `fpga-lisp` — hand-assembled experiments | Є repeatable route from language fixture to real hardware model. |
| Нові Lisp features потребують ручного ASM evidence | IR/backends дозволяють перевіряти, чи feature взагалі lowering-иться. |
| FPGA і Rust можуть розходитись непомітно | Shared conformance fixtures and result decoding роблять розбіжність visible. |
| Hardware target один | C backend already creates a second independent execution model. |

Тобто CML не мусить конкурувати з `my-lisp` evaluator. Його природна роль — **компіляторна експериментальна лабораторія контракту**, де одна source semantics проходить через різні representations: Rust values, FPGA tagged words, C tagged union, а в майбутньому потенційно ще один target.

## Пріоритетні наступні кроки

Першим я б додав target-aware diagnostics: error on more-than-eight arguments, explicit integer range policy і source-span diagnostics for unsupported target forms. Це захистить language contract від silent lowering loss.

Другим — зробив би `compiler.rs` depend on a tiny backend-neutral ABI description where можливо: reserved registers, call convention, primitive IDs, stack invariants. Навіть якщо emitter залишається target-specific, small typed contract зменшить ризик, що зміна fpga-lisp mode semantics silently conflicts with compiler assumptions.

Третім — розширював би blind E2E matrix не кількістю випадкових demos, а за contracts: quote/list shape, Nil truth, fixed/dotted/bare lambda args, self-recursive def, error channel, structural equal?, nested macro expansion. А C backend треба довести до same selected subset including Type behavior before treating it as full differential checker.

## Підсумок

`CML` уже має власну ідентичність: **не просто assembler frontend, а version-aware multi-backend compiler layer для my-lisp ecosystem**. Його найкраща риса — він не приховує hardware limits за абстракцією; вони живуть у compatibility contract, E2E decoder і tests. Якщо наступним кроком зробити всі збережені today restrictions explicit compile-time diagnostics, CML стане значно міцнішим посередником між evolving language і physical machine.

## References

[1]: https://github.com/juv4uk/cml "CML repository"
[2]: https://github.com/juv4uk/cml/blob/master/compatibility.my "CML compatibility contract"
[3]: https://github.com/juv4uk/cml/blob/master/docs/testing.md "CML test pipeline"
[4]: https://github.com/juv4uk/cml/blob/master/README.md "CML README"
[5]: https://github.com/juv4uk/cml/blob/master/src/main.rs "CML CLI pipeline"
[6]: https://github.com/juv4uk/cml/blob/master/src/ir.rs "CML intermediate representation"
[7]: https://github.com/juv4uk/cml/blob/master/src/compiler.rs "FPGA assembly emitter"
[8]: https://github.com/juv4uk/cml/blob/master/tests/conformance_test.rs "Blind FPGA conformance adapter"
[9]: https://github.com/juv4uk/cml/blob/master/tests/revision_contract_test.rs "Revision and contract drift tests"
