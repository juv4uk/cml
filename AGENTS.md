<!-- SENS-DOMAIN-LADDER-2026-10-08:BEGIN -->
## Чинна доменна доктрина SENS — для всіх агентів (2026-10-08)

**Пріоритет:** цей розділ замінює будь-які застарілі твердження нижче про Sens8/Sid8/Function8 як універсальну основу мови. Він не скасовує локальні правила безпеки, тестування, CI, координації та специфічні контракти репозиторію. Для змін, не пов'язаних із SENS, не нав'язуйте семантику SENS стороннім системам.

- **Першоджерело:** [SENS `language-contract.lisp`](https://github.com/juv4uk/sens/blob/main/language-contract.lisp) (чинний Contract 11.8), [карта повноважень](https://github.com/juv4uk/sens/blob/main/docs/semantic-authority-map.md), ратифіковані `contracts/dN-ratification.lisp` та `knowledge/dN-ratified.json`. Довідковий `AGENTS.md` не змінює мовний контракт.
- **Канонічна ідентичність:** точне двійкове значення + **точний домен** + прийнятий/доведений закон. Байт, `u8`, opcode, назва функції, таблиця поверхневих імен і однаковий числовий payload **не** створюють і не ототожнюють семантичні об'єкти.
- **Драбина:** D1 = 1 біт (PredicateBit: 1/0); D2 = 2 біти (структура: 00 пробіл, 01 закрити, 10 відкрити, 11 крапка); D3 = 3 біти (канонічне `000` = `()`; решта за ратифікованим законом); D4 = 4 біти; D5 = 5 бітів (32/32); D6 = 6 бітів (64/64); D7 = 7 бітів (126/128); D8 = 8 бітів (256/256); D9 = 9 бітів (512/512). **D1–D9 ратифіковані; D10 — лише дослідження, не ратифікований Core.** Ширина сама по собі не доводить membership, callable-механізм чи значення.
- **Історичний 8-бітний шар:** Sens8/Sid8/Function8 — лише явно обмежена сумісність, транспорт, архів, provenance або backend-проєкція. Заборонено впроваджувати нову плоску 8-бітну семантичну владу, дублювати реєстри і виводити домен зі старого коду.
- **Керування/синтаксис:** D2 володіє структурними керівними маркерами; не перетворюйте текстовий парсер, Rust, GPU, FPGA чи transport на джерело семантичного закону. `Core.D3 000` (порожня структура) ≠ `D1 0` (NO) ≠ історичне восьмибітне `00000000`.
- **Surface:** `lib/domains/d1.lisp` … `d9.lisp` у `sens` — людські проєкції у порядку `ук → укр → san → en → LISP → sym`; коди доменів первинні, людські імена — ні.
- **Джерельні файли:** для **нових виконуваних** програм SENS файл `ім'я.lisp` — канонічна людиночитана **українська проєкція `ук`** із ратифікованих таблиць доменів, а не англійський Lisp і не текстовий двійковий дамп. Файл `ім'я.sens` з тим самим stem — фізичні паковані двійкові слова D1–D9 у T5 транспорті. `ні`/`так` з D1 означають точні `0`/`1`; `за-умовою`/`перше` з D3 означають `110`/`100`. D2 залишається законом структури, а Lisp-дужки — лише людським синтаксисом. Для незіставлених surface-форм — **BLOCK**, без вигаданих координат. Історичні, архівні, табличні `.lisp` не переписувати мовчки та не вважати автоматично виконуваними.
- **Міграція:** не робити механічну заміну назв/ширин. Залишати оригінальні `.lisp`; новий same-stem `.sens` є двійковим артефактом лише після доведених parser/reader, oracle, provenance та CI-gates. Користуватися чинним `sens/scripts/migrate.py`, якщо він доступний у головній гілці; не вигадувати паралельний несумісний конвертер.
- Якщо інструкції нижче суперечать цим нормам, звірити з **поточним машинним контрактом** і виправити stale-текст окремою перевірюваною зміною, не підміняючи семантику.

<!-- SENS-DOMAIN-LADDER-2026-10-08:END -->

## Дисципліна співпраці з агентами — основний документ (2026-09-03)

**Статус: основний (primary) для всіх активних репозиторіїв екосистеми.** Цей розділ визначає, як агенти працюють із власником над кодом — і йде першим, перед будь-яким іншим вмістом цього файлу.

### Головний зсув: не "агент пише за мене", а "агент будує експеримент, а я розбираю, як ідея стала кодом"

```text
ідея
  ↓
агент пропонує реалізацію
  ↓
власник читає код
  ↓
власник пояснює його своїми словами
  ↓
дивиться ту саму ідею в іншій мові/субстраті (де це доречно)
  ↓
порівнює представлення
  ↓
тільки потім наступний крок
```

Агенти в цій екосистемі — не "програмісти замість власника". Вони:

```text
дослідник
+
лаборант
+
співстудент
+
рецензент
```

Власник лишається тим, хто формує концепцію й поступово вчиться читати її фізичне втілення.

### Не приховувати складність за готовим кодом

Якщо агент пише функцію чи будь-який нетривіальний фрагмент, він має розкласти рішення до рівня причин, не лише показати результат:

```text
тип
↓
параметри
↓
calling convention / представлення в пам'яті
↓
allocation
↓
memory layout
↓
returned value
```

Наприклад, не просто:

```c
typedef uintptr_t Value;
```

і далі — а з поясненням: чому саме цей тип, чому не альтернатива, скільки це байтів на цільовій архітектурі, що гарантує відповідний заголовок/стандарт, як це виглядає на рівні регістра. Власник сам вирішує, наскільки глибоко копати сьогодні — але агент завжди пропонує цей рівень деталізації, не ховає його.

### Після кожного невеликого фрагмента коду — 3-5 питань на розуміння САМЕ цього коду

Не абстрактний тест із мови загалом, а конкретні питання про щойно написаний фрагмент. Приклад формату:

```text
Чому тут саме цей тип, а не інша очевидна альтернатива?

Що саме зберігається в цій змінній — значення чи адреса?

Що означає ця конкретна операція/маска/умова?

Яка інструкція процесора приблизно відповідає цьому коду?

Яка частина цього рішення належить мові/предметній області, а яка — конкретній реалізації/субстрату?
```

### Крос-субстратне порівняння (де застосовно — переважно `my-lisp` і суміжні репозиторії мови)

Коли та сама ідея існує в кількох реалізаціях (наприклад, `my-lisp`: Rust, C, x86 asm, Guile, FPGA), корисний формат порівняння:

```text
1. LANGUAGE FACT       — що стверджує сама мова?
2. RUST REPRESENTATION — як це представлено зараз?
3. C REPRESENTATION    — як це можна представити в C?
4. ASM VIEW            — у що це реально перетворюється на цільовій архітектурі?
5. GUILE VIEW          — як та сама ідея виглядає на високому символьному рівні?
6. HARDWARE VIEW       — що з цього реально існує як біти, адреси, операції?
7. WHAT IS ESSENTIAL   — що належить мові, а що належить субстрату?
```

Мета — щоб після знайомства з однією ідеєю (наприклад, `cons`/pair) власник бачив не лише "що це працює", а що саме лишається незмінним у самій ідеї, а що є лише способом її представити на конкретному фізичному чи мовному субстраті. Це не обов'язковий ритуал для кожного репозиторію — застосовується там, де справді є кілька субстратів/реалізацій тієї самої ідеї для порівняння.

### Резюме принципу

Мета — не "вивчити мову X", а малими вертикальними зрізами повністю зрозуміти, як одна конкретна ідея проходить від задуму до фізичного втілення (біта в регістрі, гейта на кремнії, вузла в дереві коду). Генерувати можна багато — засвоювати варто малими, повністю зрозумілими кроками.

---
# AGENTS.md — ecosystem overview for agents working in this repo

This repo (`cml`) is one of a coordinated ecosystem (four core repos —
`my-lisp`, `fpga-lisp`, `cml`, `my-idea` — plus Sanskrit/Pāṇini research
siblings that don't touch this repo). If you're an agent (Codex, Claude
Code, or otherwise) picking up work here, read this first — it saves you
from re-deriving context another agent already has.

## Session start — join the swarm

Coordination lives on `swarm-node` (a separate binary from `:9999`), a P2P
journal/claim mesh — no agent relays for another. `127.0.0.1:9999`
(my-lisp's TCP server, `--protocol=sexpr`) is the **semantic oracle**
(`eval`/`parse`/`diagnose`), unrelated to coordination now. See my-lisp's
`docs/swarm-mesh-v2.md` for the full design.

```bash
swarm-node --port 9105 --node-id cml-1 --project cml \
           --data-dir ~/.swarm-node/cml-1 --connect 127.0.0.1:9101
```
(`127.0.0.1:9101` is my-lisp's own node — bootstrap through any one live
member, gossip connects you to the rest. Check `pgrep -af swarm-node`
first; don't start a second `cml-1`.) Then, sent to your *own* node's
port (9105, not 9101) — you can use `my-lisp --connect=127.0.0.1:9105`
(P2P client mode, forwards one sexpr line from stdin) instead of shelling
out to another language for this:

```
(join (capabilities (compiler rust lowering testing iverilog proof cml)) (roles (voter)))
(sync-tasks (file "/mnt/c/GitHub/cml/tasks.lisp"))
(next-best-action (from "cml-1"))
```

`tasks.lisp` (this repo root) is the durable plan of record — edit it,
re-`sync-tasks` after edits and after any node restart (in-memory swarm
state resets on restart; the journal replays via anti-entropy from
peers, but `tasks.lisp`'s `done`/`description` fields are what `sync-tasks`
reconciles against). A swarm event is a doorbell, never the fact itself —
verify against `evidence/`/a real commit before acting on one.


### Connector-only federation fallback

If this agent can use GitHub but cannot reach the owner WSL or `swarm-node`, do **not** work blind and do not invent a second coordination log. Read the canonical discovery board `juv4uk/sens#1599` before CLAIM. Post material findings/handoffs there with a unique `agent`/lane field plus `kind / claim / evidence / status / action`. The owner-WSL federation bridge mirrors that board into local durable inboxes and `AGENTS-LIVE-BUS`; local discoveries are mirrored back to #1599. Claims still belong to the repo task/swarm authority when that plane is reachable.

## The four repositories

- **my-lisp** — the semantic source of truth. Defines the language: parser,
  evaluator, exactness model (rationals, no floats), `lib/core.lisp` standard
  library. Language contract version **2.0** as of 2026-08-15
  (`language-contract.lisp`'s own `(major . 2) (minor . 0)` — **read that
  file directly**, never trust a number in prose, including this one; the
  1.0→2.0 break removed `'` as reader shorthand for `quote` — apostrophe
  is now a plain identifier character, see `src/parser.rs`'s own doc
  comment on `cml`'s side of that fix). Nothing else in the ecosystem may
  drift from what that repo says the language means.
- **fpga-lisp** — hardware implementation of the same language on an FPGA.
  Tracks an ISA contract (`isa-contract.lisp`; current reviewed CML target:
  **1.4** at `d1cb7eb79f675e8f2cc128c3b12918d6b08b9413`) against SENS semantics.
  Read that contract directly: the version here is only a navigation hint.
  `docs/lisp-machine-plan.md` there is the current authoritative status.
- **cml** (this repo) — an Ahead-of-Time compiler from my-lisp source,
  through a shared backend-neutral IR, to two targets today: fpga-lisp
  assembly (no runtime `eval`/`apply` loop on the hardware) and a minimal
  C emitter (`docs/heterogeneous-backends.md`). Tracks conformance
  against both sibling repos via [`compatibility.lisp`](compatibility.lisp).
  Has CI (`.github/workflows/ci.yml`) checking out both sibling repos
  fresh and running a real `iverilog` E2E simulation on every push/PR —
  see [`docs/testing.md`](docs/testing.md) for the full pipeline.
- **my-idea** — an observer/IDE layer, depends on my-lisp via
  cargo-git-dependency/submodule. Building toward a "System Observatory"
  panel.

## Machine-readable status

[`ecosystem-status.md`](ecosystem-status.md) in this repo is an append-only
chronological log of cross-session syncs (decisions, verification results,
open questions) — read it before assuming anything is stale or unverified.
my-lisp's `ecosystem-status.lisp` is the curated current-snapshot counterpart
(no history, just present state); prefer that one for "what's true right
now," this repo's `.md` for "how did we get here."

[`compatibility.lisp`](compatibility.lisp) is the actual contract: compiler
version, tested SHAs of my-lisp/fpga-lisp, supported language surface,
per-feature mechanism notes (e.g. `defmacro`, `equal?`), and known
limitations — a flat alist, read via `(read-file "compatibility.lisp")` from
my-lisp, never `(load ...)`-ed as executable source.

## Talking to my-lisp live

`my-lisp --tcp=9999 --protocol=sexpr` is the semantic oracle (loopback
only, no auth) — `eval`/`parse`/`diagnose`/`contract-version`, one
isolated environment per connection. Structured request/response, not a
raw REPL: `(request (id N) (op eval) (source "..."))` in, `(response (id
N) (status ok) (value ...) ...)` out. Use `my-lisp --connect=HOST:PORT`
(P2P client mode) to send one request without shelling out to another
language — see "Session start" above for the same mechanism used against
the swarm-node coordination plane. This is a **separate, unrelated**
thing from swarm-node coordination (see above) — don't confuse the two
ports/protocols.

## Conventions worth knowing before editing

- `defmacro` is a **compile-time-only** source transform
  ([`src/macros.rs`](src/macros.rs)) — it never reaches the FPGA compiler.
  See `compatibility.lisp`'s `defmacro` entry for the mechanism.
- `equal?` is a **native FPGA subroutine** (`cml_equal` in
  [`src/compiler.rs`](src/compiler.rs)), deliberately worklist-based (no
  `CALL`/`RET` recursion) so it doesn't depend on the still-maturing letrec
  mechanism in fpga-lisp.
- The conformance test (`tests/conformance_test.rs`) is a **blind
  adapter**: one fixed pipeline runs unmodified against every fixture — no
  fixture-specific branches inside the adapter itself. Fixtures live in the
  sibling `my-lisp` repo at `tests/fixtures/conformance.lisp`, not here.
- Requires, checked out as siblings of this repo: `../my-lisp` and
  `../fpga-lisp`, plus `python3` and `iverilog`/`vvp` on `PATH` to run the
  conformance test locally. CI provides all of this fresh on every run, so
  a missing local toolchain blocks local verification only, not landing a
  change — but install what you need yourself rather than skipping local
  verification by default.
- `cml` is no longer single-backend: `src/ir.rs`/`src/lower.rs` extract a
  backend-neutral IR from `ast::Expr`, and `src/compiler.rs` (fpga-lisp)
  and `src/c_backend.rs` (a minimal C emitter) both consume it —
  `docs/heterogeneous-backends.md` is the design doc, `docs/abi.md` the
  register-discipline reference for the fpga-lisp side specifically.
  `macros.lisp` is a from-scratch `.lisp`-hosted reimplementation of
  `src/macros.rs`'s `defmacro` expansion, proven correct by differential
  testing against the real my-lisp CLI but **not wired into the compile
  pipeline** — same status as fpga-lisp's `assembler.lisp` relative to
  `assembler.py`. See `docs/tooling-language-priority.md` before
  proposing moving more of `cml` itself to `.lisp`.

## Environment: WSL2 + Guix

Work in this repo from inside WSL2, under the Linux user named after this
repo (`cml`), not directly from Windows. Repos stay on the Windows
filesystem (`/mnt/c/GitHub/...`), not `~/projects` — enter the declared
environment before running anything:

```
wsl -u cml
cd /mnt/c/GitHub/cml
guix shell -m manifest.scm
```

[`manifest.scm`](manifest.scm) pins the full toolchain this repo needs to
be self-sufficient (`rust`, `cargo`, `git`, `make`, `gcc-toolchain` for
`src/c_backend.rs`'s target, `iverilog`, `python`) — verify with `guix
shell --pure -m manifest.scm -- cargo test --workspace` (not a bare
`guix shell`) before treating a result as evidence-grade, since `--pure`
is what actually catches an accidental dependency on the ambient shared
profile instead of this repo's own declared manifest.

## Cross-session coordination protocol (agreed with my-lisp/fpga-lisp)

1. Durable facts go in `ecosystem-status.md`/`ecosystem-status.lisp` —
   written after the fact (commit done, CI green), not "plan to do X".
2. Direct messages between sessions are for synchronous asks, not
   restating what's already in a status file.
3. Anchor claims to a commit sha or file:line, not a paraphrase from memory.
4. Don't block on confirmation before continuing your own work unless
   there's a real dependency.

## Agent Guard (M0 — PROPOSED, 2026-08-22)

План executable-constitution guardrails для агентських сесій:
`/home/agents/ecosystem/plans/AGENT-GUARD-M0.md`

Машинні гачки на C1/C7/C9/C11 (ox-alpha constitution v1.2):
tool wrapper + evidence ledger + claim gate. Статус: план,
реалізація не почата. Агенти, що заходять у репо: прочитайте
план перед write-heavy роботою; зауваження — у plans/ або
власнику напряму.


## NLP / Embeddings tooling (2026-08-22)

Для NLP-задач (ембедінги, семантична класифікація, BGE-M3): системний
python3 НЕ має torch. Використовуй
`/home/agents/GitHub/FlagEmbedding/.venv/bin/python`.
Конфіг і готові індекси: `/home/agents/GitHub/vault-semantic-mcp/`
(корпусні ембедінги вже в `data/sanskrit_embeddings.jsonl` — перевикористовуй).
GPU лише 4GB — батчі ≤4, fp16, не перераховувати зайве.
Повний рецепт: `/home/agents/ecosystem/memory/nlp-tooling-setup.md`.

### NLP consumer (2026-08-23)
Результати семантичної класифікації корпусу вже готові — дивись без GPU:
`python3 /home/agents/GitHub/vault-semantic-mcp/lookup_concept.py anumāna`
(режими: концепт / --file / --search). Епістеміка: semantic-suggest =
гіпотеза, не authority. Повний рецепт: ecosystem/memory/nlp-tooling-setup.md.
