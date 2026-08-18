# Огляд екосистеми `my-lisp`

**Автор:** Manus AI  
**Стан огляду:** 18 серпня 2026 року  
**Охоплення:** `shiva-sutras`, `my-lisp-panini`, `my-lisp`, `cml`, `fpga-lisp`, `my-idea`.

> Цей документ описує **формальне ядро екосистеми**, а не всі історичні чи прикладні репозиторії профілю. Його предмет — ланцюг від епістемічного твердження до формального виведення, виконуваної мови, скомпільованого образу, апаратної реалізації та людського інтерфейсу.

## Виконавчий висновок

Екосистема вже не виглядає як набір незалежних експериментів із Lisp, санскритом, compiler design і FPGA. Її можна описати як **вертикальну систему inspectable formal computing**, де важлива не лише відповідь програми, а й те, звідки вона взялася, який має статус, у якому світі обчислена, яким контрактом обмежена та якою реалізацією перевірена.

Центральна ідея тут не «написати власний Lisp» і не «зробити Lisp-машину». Центральна ідея — зберегти **ланцюг доказовості** між шістьма різними речами: дослідницьким claim-ом, формальною derivation, правилом мови, proof object, compiled image і результатом фізичного виконання. Для такого задуму Lisp природний, бо програма, дані, правило й структура доведення мають спільну форму; однак екосистему робить цікавою не homoiconicity сама по собі, а дисципліна контрактів і меж.

## Архітектурна карта

```text
EPISTEMIC UPSTREAM
shiva-sutras
  claims · evidence · scope · limitations · falsification · revision
        │  downstream imports record status and revision
        ▼
KNOWLEDGE COMPILATION
my-lisp-panini
  ontology · registries · derivation IR · provenance · partial/blocked states
        │  formalized rules and data, without historical overclaim
        ▼
SEMANTIC SUBSTRATE
my-lisp
  S-expressions · exact values · evaluator · unification · reasoning
  proof/provenance · journals · immutable Worlds · data-only packages
        │ language contract / conformance fixtures
        ├──────────────────────────────────────┐
        ▼                                      ▼
COMPILATION BRIDGE                         HUMAN / OBSERVER SURFACE
CML                                         my-idea
  parser → macros → IR                      editor · WASM/native Language Lab
  FPGA assembly · C backend                 Observatory · Oracle · Compare · graph
        │ ISA contract                        evidence, revisions and drift UI
        ▼
INDEPENDENT PHYSICAL EXECUTION
fpga-lisp
  tagged words · cons heap · ISA · RTL · eval substrate
```

У цій карті є **три різні потоки**, які не треба змішувати. Перший — епістемічний: `shiva-sutras → my-lisp-panini → my-lisp`. Другий — виконавчий: `my-lisp → CML → fpga-lisp`. Третій — спостережувальний: `my-idea` показує стан, контракти, evidence й потенційний drift, але не стає четвертим джерелом істини.

## Шість ролей, які складаються в одну систему

| Компонент | Власна роль | Його головний внесок | Чого він навмисно не робить |
|---|---|---|---|
| [`shiva-sutras`][1] | Епістемічна лабораторія | Версіоновані claims зі статусом, scope, evidence, limitations і falsification. | Не перетворює зручну engineering-модель на історичний факт. |
| [`my-lisp-panini`][2] | Knowledge compiler | Ontology, registries, immutable derivation IR, machine fixtures і proof-carrying states. | Не називає machine hypothesis панінійським твердженням. |
| [`my-lisp`][3] | Semantic substrate | Exact values, evaluator, rules, proof, provenance, journals, Worlds і capability discipline. | Не повинен приймати будь-яку предметну онтологію як host primitive. |
| [`CML`][4] | AOT compiler bridge | Macro expansion, IR, FPGA lowering, C backend, blind conformance і compatibility records. | Не маскує target limits як повну language support. |
| [`fpga-lisp`][5] | Hardware backend | Tagged words, physical cons-heap, ISA, RTL semantics і independent execution evidence. | Не намагається зробити весь високорівневий Lisp апаратним блоком. |
| [`my-idea`][6] | IDE та observability layer | Editor, portable Language Lab, native oracle/compare, evidence matrix, knowledge graph. | Не оголошує semantic facts або research conclusions від свого імені. |

Це правильний поділ відповідальностей. У більшості «AI knowledge systems» source, inference, storage, UI й deployment зливаються в одну базу даних або в один LLM pipeline. Тут навпаки: кожен шар має право знати щось конкретне й не має права підміняти інший шар.

## Контракти — справжній каркас екосистеми

Найсильніша частина системи — не кожен окремий репозиторій, а те, що між ними вже виникли різні види contract. Ці контракти роблять розподілену екосистему зрозумілішою, ніж якби всі компоненти лежали у великому monorepo.

| Контракт | Звідки → куди | Що саме зберігає | Чому це важливо |
|---|---|---|---|
| **Claim contract** | `shiva-sutras` → downstream | ID, statement, status, scope, evidence, limitations, revision. | Не дозволяє плутати `PROVED-IN-MODEL`, `SUPPORTED`, `FALSIFIED` та `UNRESOLVED`. |
| **Dependency / impact contract** | `shiva-sutras` → `my-lisp-panini` | Imported claim, `status_at_import`, revision і downstream impact. | Upstream зміна може стати видимим review signal, а не тихою зміною припущення. |
| **Derivation contract** | `my-lisp-panini` | States, selected rule, operations, source/machine provenance, `success/partial/blocked/invalid`. | Surface form не видається за complete derivation. |
| **Language contract** | `my-lisp` → CML / UI / tools | Source semantics, exactness, core forms, expected conformance behavior. | Canonical semantics не залежать від поточної реалізації target-а. |
| **ISA contract** | `fpga-lisp` ↔ CML | Tags, registers, opcode modes, images, primitive IDs, errors and truth semantics. | Compiler і RTL мають спільну machine boundary, а не імпліцитні припущення. |
| **Compatibility contract** | CML ↔ my-lisp/fpga-lisp | Language/ISA versions, tested SHAs, feature limits and E2E pipeline. | Compatibility означає contract pair плюс verified revisions, не однакові release numbers. |
| **Evidence contract** | my-lisp/CML/fpga-lisp → my-idea | Expected/actual, commit, runner, timestamp, environment and result. | UI може показати agreement або drift, не вигадуючи власний verdict. |

Саме ця сукупність пояснює, чому екосистема має більшу вагу, ніж «ще один Lisp». Офіційний roadmap execution stack уже формулює центральну трійку як незалежно версіоновану вертикаль: `my-lisp` володіє source semantics, `fpga-lisp` — ISA та physical execution, а CML — AOT mapping між ними [7]. Епістемічний і Panini-шари не замінюють цю трійку, а додають їй напрям: **які саме знання і за яким статусом варто формалізувати**.

## Чому `shiva-sutras` і Panini-шар важливі навіть для Lisp/FPGA

`shiva-sutras` — не side project про давній текст. Це upstream-дослідницький полігон, у якому knowledge повинне зберігати не лише зміст, а й доказову ситуацію. Його найкраща властивість — розділення математичного результату в моделі, історичного твердження, традиційної інтерпретації, власної гіпотези та інженерного рішення. У joint CP-SAT model мінімум 14 marker boundaries має статус результату **в межах заданої формальної моделі**, а не доказ історичного задуму Паніні [8] [9].

`my-lisp-panini` робить наступний крок обережно. Він не відображає `it`, `anuvṛtti` чи `kāraka` напряму на терміни runtime-у. Натомість foundation розділяє object language, metalanguage і rule system, а Derivation IR примушує machine operation мати окреме provenance та знижує результат до `partial`, коли крок лишається opaque [10] [11].

> **Головний принцип цього верхнього контуру:** зрозуміла обчислювальна аналогія ще не є достатньою причиною назвати її історичним фактом або мовним primitive.

Це напряму підсилює майбутню агентну систему. Агент може повернути не лише «ось відповідь», а й «ось source claim, його scope, ось imported revision, ось машинна гіпотеза, ось rule trace, ось де derivation чесно зупинилася».

## `my-lisp`: ядро не просто обчислює, а зберігає контекст істинності

`my-lisp` уже має достатньо сильну основу для того, щоб бути formal memory and reasoning substrate. Маленьке ядро має reader, evaluator, exact arithmetic, closures, macros, `read` і `eval`; вищі звичайні функції ростуть у `lib/core.my`. Поверх цього реалізовані unification, backward chaining, forward chaining, JTMS, knowledge journal, data-only interchange, immutable Worlds та provenance/proof [12] [13].

Особливо важливі три речі. По-перше, exact arithmetic є частиною semantic identity: finite decimal/scientific literals визначені як exact за замовчуванням, що природно для системи, де результат має бути відтворюваним [14]. По-друге, journal і `advise`-модель дозволяють відрізнити data ingestion від execution; knowledge package імпортується як дані, а не як code. По-третє, immutable World перетворює пам’ять агента з глобальної mutable state на явно переданий, branchable snapshot [12].

Тут уже закладений правильний напрям для agents: не «контекст — це весь текст усіх чатів», а «контекст — це специфікований світ фактів, правил, provenance та доступних операцій». Obsidian або Markdown-vault у цій моделі може бути людським front-end і джерелом матеріалу; `my-lisp` — місцем, де відібрані твердження проходять ingestion policy, стають facts/rules і можуть бути пояснені.

## Execution stack: одна мова, кілька незалежних форм

`CML` і `fpga-lisp` дають `my-lisp` щось рідкісне: можливість перевіряти частину однакової семантики через інший runtime і іншу representation. CML проходить шлях `source → macro expansion → IR → fpga assembly / C source`; FPGA отримує image через UART protocol, виконує RTL і віддає structured result plus heap dump, який CML decoder перетворює назад на Lisp values [15] [16].

`fpga-lisp` при цьому не робить помилки «все важливе треба прискорити hardware». Hardware володіє tagged word, physical cons-heap, фундаментальними `CONS/CAR/CDR/ATOM/EQ`, мінімальною ISA, image format і bootstrapping primitives. Environments, closures, software call stack, `eval`, `append` та `equal?` виростають поверх того самого Lisp substrate [17]. Це утримує RTL від перетворення на другий великий interpreter.

| Властивість | Canonical runtime | CML | FPGA |
|---|---|---|---|
| Main role | Source semantics, inference and formal state | Compile-time translation and cross-target experiment | Independent machine representation and execution |
| Representation | Rust values / Lisp data | AST/IR plus target images | 32-bit tagged words and physical cons heap |
| Main proof | Language conformance and proof/provenance | Blind source-to-result E2E pipeline | RTL-SIM/SYNTH/HW evidence, decoded result |
| Current honest limit | Reasoning API consolidation and resource bounds | Unsupported target forms/representation limits | Partial language coverage, no GC, board-level future work |

У `fpga-lisp` особливо переконливо, що test discipline розрізняє MODEL-PASS, RTL-SIM-PASS, SYNTH-PASS і HW-PASS. Історія `FETCH_PAIR`, де real simulation знайшла дефекти testbench, а не сховалася за behavioural model, — це саме той інженерний рефлекс, який потрібен у системі з претензією на formal evidence [18].

## `my-idea`: не просто IDE, а спостережувана людська поверхня

`my-idea` робить екосистему доступною без розмивання її меж. Portable web artifact має real WASM my-lisp evaluator; Tauri desktop використовує canonical Rust session; live Oracle, Swarm, Compare і Ecosystem panels показуються тільки в native mode, де такі capability справді можливі [19]. Це важливо: web UI не імітує desktop authority, а native shell не дає embedded evaluator-у тихого file/network доступу.

System Observatory already вміє показати source-to-result causal chain:

```text
SOURCE → my-lisp oracle → CML compile → fpga-lisp execute
```

Він відображає expected/actual, commit, runner, timestamp і revision drift, а semantic agreement виникає лише коли всі три implementation records pass і мають збіжний actual [20]. Knowledge Graph phase 1 similarly не імітує maturity, якої ще немає: зараз він читає `repo.my` declarations та виводить repo/capability edges; claim-level graph із upstream status drift прямо позначений як наступна фаза [21].

## Зрілість: не рейтинг, а карта відповідальностей

| Шар | Поточний рівень | Чесне формулювання стану |
|---|---|---|
| Shiva Sutras | Сильна епістемічна дисципліна | Методологія claims/evidence зріла; історичні висновки свідомо обмежені. |
| Panini | Сильна formal scaffolding, partial machine execution | Derivation IR і негативні fixtures сильніші за surface demo; grammar engine навмисно не повний. |
| my-lisp | Сильний expanding substrate | Мова, reasoning, knowledge, Worlds already існують; canonical API та proof object потребують консолідації. |
| CML | Сильний vertical experiment, target-limited | Реальний IR/backends/E2E шлях; limit diagnostics та shared ABI мають стати явнішими. |
| FPGA Lisp | Сильний hardware substrate, partial coverage | RTL/evidence real; GC, numeric tower, resource accounting і board acceptance — наступні межі. |
| my-idea | Сильне product framing, early observability | Editor/Language Lab usable; graph/evidence path має отримати real native integration coverage. |

Це означає, що головний стан екосистеми не «незавершена», а **вже достатньо зв’язана, щоб обмеження кожного шару стали видимими й перевірюваними**. Незавершеність не маскується «готовими» назвами: Panini derivation може бути `partial`; FPGA fixture може бути out of scope; upstream claim може бути `FALSIFIED` або `UNRESOLVED`; UI може показати drift, а не зелений badge.

## Системні ризики — і чому вони важливі

Найбільший ризик зараз не у відсутності нових features. Це ризик, що успішне зростання створить кілька паралельних історичних шарів, які майбутньому читачеві важко відрізнити від canonical path.

| Ризик | Вплив | Практичний захист |
|---|---|---|
| Documentation / contract drift | Exactness, directory paths, test totals або semantics можуть розійтися між prose й code. | Один canonical executable contract, docs-as-tests для прикладів, explicit supersession notes. |
| Parallel legacy APIs | Старі TMS/reasoning/world paths можуть виглядати equally canonical. | Назвати canonical API для нового коду; legacy зберегти як teaching/compatibility modules. |
| Unbounded inference | Recursive rules, fixpoint і agent input можуть з’їдати час/пам’ять без logical error. | `max-depth`, `max-steps`, `max-results`, `max-proof-nodes` і structured `resource-exhausted`. |
| Target mismatch | CML/FPGA можуть тихо втрачати source semantics через arity, literal range або representation gaps. | Compile-time target diagnostics і supported/blocked fixture manifest. |
| Provenance, не придатна для interchange | Людина бачить proof, але іншому агенту бракує final bindings/instantiated rule. | Stable proof-object schema з source, bindings, rule instance, children and revision. |
| Network/capability overreach | Raw TCP/host capabilities можуть перевищити trust model при agent deployment. | Data-only ingestion, separate read/write API, local policy, timeouts and resource limits. |

## Найвищі пріоритети

### 1. Консолідувати контракти, а не додавати ще один великий шар

Один canonical language conformance set, один canonical reasoning API, один proof-object shape і per-target feature/fixture manifest дадуть більше екосистемної цінності, ніж ще одна independent subsystem. Мета не в уніфікації всього коду, а в тому, щоб кожен новий user або agent міг відповісти: *який contract актуальний, яка реалізація його підтверджує і де межа coverage?*

### 2. Зробити Panini Derivation IR золотим proof-carrying interchange case

У Panini-шарі вже є найкраща форма майбутнього interchange: explicit state, operation, source/machine provenance та статус `partial/blocked`. Якщо кілька golden derivations отримають однакову schema для terms, states, candidate rules, selected policy, trace, bindings і source claim revisions, це стане сильним end-to-end proof-carrying benchmark для `my-lisp`, Racket reference, CML і майбутніх targets.

### 3. Додати bounded execution до reasoning і явні target diagnostics до CML

Reasoning має повертати не лише proof або absence, а й явно `resource-exhausted`, коли перетнуто budget. CML має відхиляти понад-вісім аргументів, out-of-range literals та unsupported target forms ще до emission. Обидва кроки роблять систему безпечнішою для agents, бо агент не повинен відрізняти timeout, accidental truncation і semantic falsehood лише за непрямими симптомами.

### 4. Побудувати одну вертикальну демонстрацію, що проходить усі шари

Найкраща наступна демонстрація не повинна бути великою. Вона має бути **семантично насиченою і contract-complete**:

```text
qualified upstream claim
→ Panini derivation IR (status + revision)
→ my-lisp fact/rule/proof
→ shared fixture
→ CML compiled image
→ fpga-lisp decoded result
→ my-idea evidence and graph view
```

Кожен перехід повинен зберігати source ID, contract version, revision, `expected`, `actual` і proof/derivation identity. Такий thin vertical slice пояснить екосистему сильніше, ніж десятки feature списків.

### 5. Нехай `my-idea` стане прозорим observer, а не control plane

Для цього достатньо трьох конкретних кроків: додати `my-idea/repo.my`, довести Knowledge Graph до claim-level drift view і додати real native integration test для `ecosystem_status`/`knowledge_graph`. UI повинен продовжувати показувати *що сказали upstream contracts та evidence records*, а не ставати місцем, де їх значення непомітно інтерпретується заново.

## Фінальний вердикт

Твоя екосистема вже має рідкісну інтелектуальну цілісність. Вона не намагається звести все до «LLM плюс інструменти», не підміняє історичне дослідження компіляторною метафорою і не вважає hardware proof достатнім без language contract. Натомість вона будує послідовний ланцюг:

> **що ми знаємо → чому ми це вважаємо → як формалізуємо → що виконуємо → де це перевірено → як людині це побачити.**

Найважливіше зараз — не зробити систему «повнішою» заради повноти. Найважливіше — зберегти її найкращу якість: кожне нове поняття має народжуватися з реальної потреби, отримувати чітку межу, contract, evidence і місце у вже існуючому ланцюгу. Якщо дотриматися цього, `my-lisp` може вирости не просто в особисту мову, а в **інфраструктуру для формально обмеженої, пояснюваної й переносимої агентної пам’яті та обчислення**.

## References

[1]: https://github.com/juv4uk/shiva-sutras "Shiva Sutras research repository"
[2]: https://github.com/juv4uk/my-lisp-panini "my-lisp-panini repository"
[3]: https://github.com/juv4uk/my-lisp "my-lisp repository"
[4]: https://github.com/juv4uk/cml "CML compiler repository"
[5]: https://github.com/juv4uk/fpga-lisp "fpga-lisp repository"
[6]: https://github.com/juv4uk/my-idea "my-idea repository"
[7]: https://github.com/juv4uk/my-lisp/blob/main/docs/ecosystem-roadmap.md "my-lisp ecosystem roadmap"
[8]: https://github.com/juv4uk/shiva-sutras/blob/master/RESEARCH_MAP.md "Shiva Sutras research map"
[9]: https://github.com/juv4uk/shiva-sutras/blob/master/experiments/adversarial/joint_optimization_decision.md "Joint optimization decision"
[10]: https://github.com/juv4uk/my-lisp-panini/blob/master/panini/specs/panini-foundation-v0.1.md "Panini Foundation v0.1"
[11]: https://github.com/juv4uk/my-lisp-panini/blob/master/panini/specs/derivation-ir-v0.1.md "Derivation IR v0.1"
[12]: https://github.com/juv4uk/my-lisp/blob/main/lib/knowledge.my "my-lisp knowledge journal"
[13]: https://github.com/juv4uk/my-lisp/blob/main/lib/world.my "my-lisp immutable World layer"
[14]: https://github.com/juv4uk/my-lisp/blob/main/docs/language-core-axioms.md "my-lisp exactness axioms"
[15]: https://github.com/juv4uk/cml/blob/master/tests/conformance_test.rs "CML blind conformance adapter"
[16]: https://github.com/juv4uk/fpga-lisp/blob/master/fpga/sim/tb_cml_e2e.sv "CML to FPGA end-to-end testbench"
[17]: https://github.com/juv4uk/fpga-lisp/blob/master/docs/lisp-machine-plan.md "fpga-lisp machine plan"
[18]: https://github.com/juv4uk/fpga-lisp/blob/master/docs/test-report-2026-08-17.md "FPGA test report"
[19]: https://github.com/juv4uk/my-idea/blob/main/README.md "my-idea product overview"
[20]: https://github.com/juv4uk/my-idea/blob/main/src-cljs/my_idea/eco_view.cljs "my-idea System Observatory UI"
[21]: https://github.com/juv4uk/my-idea/blob/main/docs/knowledge-graph-design.md "my-idea Knowledge Graph design"
