# Аудит останніх комітів екосистеми juv4uk

**Автор:** Manus AI  
**Перевірено:** 18 серпня 2026 року через GitHub API  
**Метод:** перевірено HEAD default branch кожного публічного репозиторію власника; для активного ядра окремо прочитано три останні коміти.

## Висновок

Я перевірив **53** публічні репозиторії `juv4uk`. У шести HEAD-коміти датовані 18 серпня 2026 року, ще у трьох є коміти за 2026 рік, а останні 44 змінювалися 2024 року або раніше. Тому реально активне ядро зараз чітко утворюють шість репозиторіїв: `my-lisp`, `shiva-sutras`, `my-lisp-panini`, `fpga-lisp`, `cml` і `my-idea`.

> Остання хвиля змін — не хаотичне «скидання оглядів у репозиторії». Вона формує корисний цикл: зовнішній аналіз → власна перевірка тверджень → виправлення контракту чи документації → точно окреслена задача реалізації. Найважливіші зміни в коді — ідемпотентні task definitions у `my-lisp` і виправлення marker/sound collision у `pratyāhāra` в `shiva-sutras`.

## 1. Активне ядро: поточний HEAD default branch

| Репозиторій | HEAD | Час коміту, UTC | Основний зміст |
|---|---|---:|---|
| [`my-lisp`](https://github.com/juv4uk/my-lisp) | [`078fc9b`](https://github.com/juv4uk/my-lisp/commit/078fc9b57c70d6b2c352f81bdb02c2fe4a1c8677) | 20:30:04 | Однакові повторні `define-task` стали ідемпотентними. |
| [`fpga-lisp`](https://github.com/juv4uk/fpga-lisp) | [`e8238c2`](https://github.com/juv4uk/fpga-lisp/commit/e8238c2a545d7a3f047dcee624668d254daca4aa) | 20:25:56 | Власна перевірка оглядів і виправлення документаційного розходження; RTL не змінювався. |
| [`my-idea`](https://github.com/juv4uk/my-idea) | [`114cee3`](https://github.com/juv4uk/my-idea/commit/114cee3388ebeeed5a45ecdfcfef831ff5b2bb99) | 20:25:26 | Синтез оглядів і пріоритизація інтеграції з реальними даними екосистеми. |
| [`cml`](https://github.com/juv4uk/cml) | [`4e1c938`](https://github.com/juv4uk/cml/commit/4e1c938f00e06408d107ded054d7f8df4f7eeac6) | 20:24:10 | Висновки з оглядів; свідомо не почато передчасну реалізацію UPC lowering. |
| [`my-lisp-panini`](https://github.com/juv4uk/my-lisp-panini) | [`eef9a30`](https://github.com/juv4uk/my-lisp-panini/commit/eef9a30d226af61267964797cd907f9e503173d1) | 20:16:54 | Відновлення коректності agent/architecture документації. |
| [`shiva-sutras`](https://github.com/juv4uk/shiva-sutras) | [`e0c5b60`](https://github.com/juv4uk/shiva-sutras/commit/e0c5b60b213ce7041684f5871e62010a16d99ee8) | 20:12:31 | Епістемічні заголовки, виявлення дрейфу джерела та відтворюване середовище. |

## 2. Найважливіші зміни за змістом

### `my-lisp`: семантика журналу стала ближчою до дисципліни розподілених систем

Останній коміт [`078fc9b`](https://github.com/juv4uk/my-lisp/commit/078fc9b57c70d6b2c352f81bdb02c2fe4a1c8677) змінює `crates/swarm-node/src/main.rs` і додає інтеграційний тест. Тепер вхідний `define-task` порівнюється з поточним спроєктованим означенням задачі. Якщо всі поля тотожні, результат має форму `(ok (task …) (unchanged t))`, а **новий journal event не додається**. Якщо змінилося хоча б одне поле, новий event все одно коректно записується [1].

Це реальне семантичне покращення, а не лише документаційна правка. Воно не дає повторним swarm-notification назавжди дублювати в журналі той самий факт, але зберігає легітимні оновлення metadata. Межа навмисно вузька: це ідемпотентність точно однакового повторного оголошення, а не загальний алгоритм злиття чи примирення суперечливих визначень.

Два попередні коміти теж змістовні. [`0b20e46`](https://github.com/juv4uk/my-lisp/commit/0b20e46) зберігає власний огляд зовнішніх аналізів, а [`bbf2662`](https://github.com/juv4uk/my-lisp/commit/bbf2662) виправляє зовнішнє посилання на сутру Sarvam за першоджерелом [2] [3]. Разом вони показують правильну послідовність: спершу виправити факт за першоджерелом, потім покращувати operational behavior системи.

### `shiva-sutras`: P0-помилку pratyāhāra справді виправлено, а поточний HEAD додає epistemic/provenance guardrails

Ключове виправлення — передостанній коміт [`55d60a6`](https://github.com/juv4uk/shiva-sutras/commit/55d60a6) з назвою **«Fix pratyahara() marker/sound spelling-collision bug»** [4]. Актуальна реалізація `pratyahara()` тепер включає кожен реально перелічений звук від `start_idx` до `end_idx`; вона більше не пропускає звичайний звук лише тому, що його SLP1-написання збігається з маркером пізнішої сутри. Коментар у коді правильно перелічує collision family: `l`, `y`, `r`, `m`, `v`, `Y`.

Це пряме виправлення раніше знайденої систематичної проблеми: **81 mismatch серед 301 валідної форми** у вичерпному oracle. Воно важливе архітектурно, бо відновлює правильну межу між метаданим — фінальним маркером сутри — і входженням звуку в перелічену звукову послідовність.

Поточний HEAD [`e0c5b60`](https://github.com/juv4uk/shiva-sutras/commit/e0c5b60b213ce7041684f5871e62010a16d99ee8) робить три зрілі кроки [5]. По-перше, `prototype/upc8.py` прямо називає модель **інженерною** та **експериментальною** й заперечує претензію на universal phoneme inventory, на історичний задум Пāṇini та на FPGA performance result. По-друге, він додає `CANON_SOURCE_SHA256`, `canon_source_matches()` і тест, який помічає дрейф між вручну підтримуваною SLP1-транскрипцією та YAML-каноном. По-третє, додається Guix manifest; відсутність у Guix пакета `python-ortools` зафіксована чесно, а не замовчана.

SHA-перевірку commit описує правильно: це **детектор дрейфу**, а не доказ коректності ручної IAST→SLP1 транскрипції. Саме такою й має бути її епістемічна межа.

### `CML`: roadmap захищено від передчасної роботи над lowering

HEAD [`4e1c938`](https://github.com/juv4uk/cml/commit/4e1c938f00e06408d107ded054d7f8df4f7eeac6) додає огляди й `docs/manus-review-conclusions.md`. Власний висновок репозиторію правильний: target-aware diagnostics — це конкретна й актуальна прогалина CML; UPC data-section lowering принципово має сенс, але є передчасним, доки у `my-lisp` або `shiva-sutras` не існує авторитетного UPC format/profile contract [6]. `ir.rs`, `lower.rs` та emitter code у цьому коміті не змінювалися.

Ця стриманість важлива. Дизайн CML з `DataRef` або immutable data section стає стабільним лише тоді, коли byte grammar, profile identity та assignment versions існують у справді авторитетному upstream-шарі. Якщо реалізувати підтримку в CML першою, компілятор випадково стане власником фонологічного контракту.

Безпосередньо попередній коміт [`c970e2a`](https://github.com/juv4uk/cml/commit/c970e2a) додає автоматичну перевірку дрейфу версії language contract [7]. Це реальне покращення compiler integrity і продовження стратегії перетворювати cross-repo claims на виконувані перевірки.

### `fpga-lisp`: кращі докази та узгоджена документація, але без передчасного кроку в UPC RTL

HEAD [`e8238c2`](https://github.com/juv4uk/fpga-lisp/commit/e8238c2a545d7a3f047dcee624668d254daca4aa) додає пакет оглядів і власну відповідь із точковою перевіркою тверджень за реальними джерелами. Він знаходить і виправляє конкретний documentation drift: `docs/testing.md` перелічує **33 testbench-и**, тоді як попередній звіт називав 32. Також звіряються заявлені LUT/FF resource numbers [8].

Критично, що коміт **не** заявляє про нову RTL-реалізацію або FPGA-результат для UPC. Він фіксує спільний висновок: UPC має йти через P0/P1/P2 staging і чекати, доки `shiva-sutras` стабілізує дослідницьку та контрактну межу upstream. Це правильний порядок для системи, де всі 16 primary opcode slots вже зайняті.

Два попередні коміти додають важливу evidence infrastructure: [`73b8cb6`](https://github.com/juv4uk/fpga-lisp/commit/73b8cb6) оновлює звіт про справжні RTL-SIM проходження, а [`ba37d11`](https://github.com/juv4uk/fpga-lisp/commit/ba37d11) приймає scaffold Swarm Contract [9] [10].

### `my-idea`: фокус зміщується від мокованої візуалізації до реальної інтеграції екосистеми

HEAD [`114cee3`](https://github.com/juv4uk/my-idea/commit/114cee3388ebeeed5a45ecdfcfef831ff5b2bb99) додає огляди, документує власні незалежні перевірки, підіймає пріоритет swarm contract і додає `IDEA-REAL-ECOSYSTEM-INTEGRATION-TEST`, бо `eco-panel.test.mjs` нині перевіряє моковані дані [11]. Це корисне виправлення пріоритету: візуальна карта екосистеми стає архітектурно значущою лише тоді, коли споживає реальні дані `repo.my` і contracts.

Попередній коміт [`b13443a`](https://github.com/juv4uk/my-idea/commit/b13443a) — це фактичний product step: **Knowledge Graph phase 1**, вкладка з node/edge graph, похідним від `repo.my`. Отже, поточні документаційні та task-зміни підсилюють уже наявну можливість, а не замінюють реалізацію текстом [12].

### `my-lisp-panini`: operational architecture documentation ремонтується перед подальшим розширенням

HEAD [`eef9a30`](https://github.com/juv4uk/my-lisp-panini/commit/eef9a30d226af61267964797cd907f9e503173d1) виправляє застарілі твердження `AGENTS.md` про координацію та dependencies і додає recovery review. Два попередні коміти додають repo-wide validator evidence та виправляють застарілі посилання `foundation/` у 26 файлах після перейменування каталогу на `sastra/` [13] [14] [15].

Це не зміни derivation engine, але вони доречні. Дослідницько-компіляторний міст із кількома agents та named contracts потребує coordination documentation не менш надійної, ніж формальні правила. Застарілі інструкції для agents інакше стають джерелом процесних помилок і хибних припущень.

## 3. Що шість репозиторіїв говорять разом

| Загальна тема | Конкретні докази | Архітектурний наслідок |
|---|---|---|
| **Епістемічне маркування** | UPC headers у Shiva, SHA drift test, чіткі CML conclusions. | Твердження мають status і provenance замість видимості остаточності. |
| **Виконувані контракти** | CML language-contract drift check; my-lisp guard від duplicate task definition. | Cross-node та cross-repo комунікація отримує виконувані межі. |
| **Верифікація оглядів** | `fpga-lisp` і `my-idea` незалежно перевіряють claims; Panini використовує validator output. | Зовнішній аналіз стає входом до evidence, а не авторитетом сам по собі. |
| **Відсутність передчасної UPC-реалізації** | CML не змінює IR/emitter; FPGA не змінює RTL/opcode. | Дослідницький і format contract передують lowering та hardware. |
| **Наступний крок — реальна інтеграція** | `repo.my` graph phase 1 у `my-idea` + запланований nonmock integration test. | Observability екосистеми рухається від presentation до live data. |

Найсильніший безпосередній dependency chain тепер виглядає так:

```text
shiva-sutras
  виправляє семантичну помилку та позначає/прив’язує дослідження
        ↓
profile / UPC format / assignment contract
        ↓
semantic/provenance representation у my-lisp
        ↓
typed static data section у CML
        ↓
packed bank + strict decoder у fpga-lisp
        ↓
my-idea показує реальний стан репозиторіїв і контрактів
```

Ідемпотентність swarm events у `my-lisp` рухається паралельно з цим ланцюгом як спільна гігієна екосистеми, а не як UPC-специфічна функція.

## 4. Неключові репозиторії

Ще три репозиторії мають коміти за 2026 рік, але нині не входять до шестирепозиторного implementation chain: `tauricode` (2026-08-12), `my-ide` (2026-08-07) і `Clojure-code` (2026-08-07). Їхні останні зміни — upstream, documentation або CI work; вони не є новими залежностями активної екосистеми `my-lisp`.

Решта 44 публічні репозиторії мають HEAD commits 2024 року або раніше. У цьому аудиті вони розглядаються як historical projects, forks або archives; їхні останні коміти не змінюють архітектуру чи сумісність активної шестирепозиторної системи.

## 5. Підсумок

Останні коміти узгоджені між собою та мають адекватний темп. Важливе технічне виправлення з UPC-дослідження — pratyāhāra marker/sound collision — уже виправлене upstream. Найновіший коміт Shiva додає provenance й епістемічні guardrails навколо прототипу. CML і FPGA свідомо не почали реалізацію, доки не з’являться стабільні upstream contracts. `my-lisp` покращив реальну поведінку event journal, а `my-idea` рухається до реального графа на основі фактичних артефактів екосистеми.

Коротко: екосистема перейшла від **«кількох сильних репозиторіїв із пов’язаними ідеями»** до **«репозиторіїв, які дедалі більше перевіряють, документують і обмежують одне одного через contracts та evidence»**.

## Посилання на перевірені коміти

[1]: https://github.com/juv4uk/my-lisp/commit/078fc9b57c70d6b2c352f81bdb02c2fe4a1c8677 "my-lisp: ідемпотентний однаковий define-task"
[2]: https://github.com/juv4uk/my-lisp/commit/0b20e46 "my-lisp: власний огляд зовнішніх аналізів"
[3]: https://github.com/juv4uk/my-lisp/commit/bbf2662 "my-lisp: виправлення посилання Sarvam за першоджерелом"
[4]: https://github.com/juv4uk/shiva-sutras/commit/55d60a6 "shiva-sutras: виправлення marker/sound collision у pratyāhāra"
[5]: https://github.com/juv4uk/shiva-sutras/commit/e0c5b60b213ce7041684f5871e62010a16d99ee8 "shiva-sutras: епістемічні заголовки, provenance та Guix manifest"
[6]: https://github.com/juv4uk/cml/commit/4e1c938f00e06408d107ded054d7f8df4f7eeac6 "CML: висновки з оглядів"
[7]: https://github.com/juv4uk/cml/commit/c970e2a "CML: автоматична перевірка дрейфу language contract"
[8]: https://github.com/juv4uk/fpga-lisp/commit/e8238c2a545d7a3f047dcee624668d254daca4aa "fpga-lisp: відповідь на огляд і виправлення документаційного дрейфу"
[9]: https://github.com/juv4uk/fpga-lisp/commit/73b8cb6 "fpga-lisp: звіт про реальні RTL simulation проходження"
[10]: https://github.com/juv4uk/fpga-lisp/commit/ba37d11 "fpga-lisp: scaffold Swarm Contract"
[11]: https://github.com/juv4uk/my-idea/commit/114cee3388ebeeed5a45ecdfcfef831ff5b2bb99 "my-idea: аналіз оглядів і пріоритет реальної інтеграції"
[12]: https://github.com/juv4uk/my-idea/commit/b13443a "my-idea: Knowledge Graph phase 1 на основі repo.my"
[13]: https://github.com/juv4uk/my-lisp-panini/commit/eef9a30d226af61267964797cd907f9e503173d1 "my-lisp-panini: AGENTS.md і відновлення архітектурної документації"
[14]: https://github.com/juv4uk/my-lisp-panini/commit/be8e87c "my-lisp-panini: огляд усього репозиторію з validator output"
[15]: https://github.com/juv4uk/my-lisp-panini/commit/2b0fc06 "my-lisp-panini: виправлення застарілих посилань після перейменування на sastra"
