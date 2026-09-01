# CODE-SURVEY-2026-09-01 — огляд коду cml

**Виконавець:** wsl-nidana-1
**Метод:** реальне читання `src/ir.rs`, `src/semantic.rs`,
`src/x86_freestanding.rs`, `src/gpu_cuda.rs`. Не exhaustive.

## `src/ir.rs` (146 рядків) — один плаский `Ir` на всі п'ять бекендів

Ядрові Lisp-форми (`Var/App/Lambda/Cond/Let/Def/Quote/Prim`) + невеликий
закритий набір `PrimOp` (Add/Sub/Cons/Car/Cdr/Eq/Atom/EqualP —
"mirroring what's actually implemented, not aspirational", за власним
коментарем файлу) + GPU-only вузли (`Map/Reduce/Scan/Index/
ParallelRegion`), дописані в кінець. **Не чисто-шаровий IR** — один плаский
sum-type, де кожен бекенд просто ігнорує variants, яких не обробляє.
Реальна спільна абстракція для Lisp-ядра, але GPU-розширення читається як
"додати cases в той самий enum", а не genuinely ортогональний
compute-IR-шар.

## `src/semantic.rs` (98 рядків) — малий, рекурсивний fail-closed гейт

Відхиляє лише дві форми: >1 body-вираз у lambda (`Ir::Lambda` тримає
рівно один `body: Box<Ir>`), і дублікати параметрів **після uppercase-
нормалізації** — бекендове symbol-представлення в верхньому регістрі,
тож `x`/`X` колізять, навіть якщо в джерелі написані по-різному — реальна,
неочевидна деталь коректності. Рекурсія у вкладені lambda коректна
(провалюється через generic `_` arm у `analyze_expr` на кожен аргумент,
який знову матчить `"lambda"`).

## `src/x86_freestanding.rs` (1124 рядки, найбільший файл)

Виклик closure (`emit_single_argument_closure_call`, ~рядок 755-822)
диспетчериться через **лінійний ланцюжок `cmpl`/`jne` проти КОЖНОГО
closure definition ID, коли-небудь скомпільованого в юніті**, провалюючись
у `wsm_fail(AbiViolation)` без збігу — не jump table, не hash dispatch.
Коректно і чесно fail-closed, але O(n) на call site, де n = загальна
кількість closures у програмі, не лише досяжних там. Це механізм під
"escaping closure" фікстурою з ранішої частини цієї сесії. Preflight
(`preflight_lambda_body`/`preflight_tail_body`) окремо відхиляє загальний
`App`/взаємну рекурсію заздалегідь — допускаються лише self-tail-calls
(`TailSelfCall`, register-reload + `jmp`, константна глибина стеку) і
single-arg closures. **Заведено як `CML-X86-CLOSURE-DISPATCH-SCALABILITY`
у `tasks.my`** (лише документувати межу + бенчмарк, не редизайнити без
доказу потреби).

## `src/gpu_cuda.rs` (71 рядок, малий) — навмисно вузький

Емітить лише `map`-kernel, лише для single-parameter kernel body, що є
або `CheckedAdd`/`ExactInteger`/`Parameter(0)` цілочисельним деревом
виразів, або (асиметрично) float-шляхом, обмеженим ОДНІЄЮ конкретною
формою — `x + constant` через `f32_affine_offset` — не тим самим
загальним деревом виразів, яке дозволяє int-шлях. Жодної загальної
float-арифметики. Чесний щодо обмеження (`NotEligible`/
`UnsupportedRegion` помилки), без тихого fallback. **Асиметрія ніде не
задокументована як навмисна** — заведено як
`CML-GPU-CUDA-INT-FLOAT-ADMISSION-DOCS` у `tasks.my`.

## Позначені занепокоєння (не виправлено)

- Лінійний closure-dispatch у `x86_freestanding.rs` — реальна межа
  масштабування, якщо кількість closures зросте.
- GPU compute-вузли `Ir` дописані в той самий плаский enum, що й ядрові
  Lisp-форми, замість окремого шару — кожен не-GPU бекенд змушений
  матчити (або ігнорувати) п'ять compute-variants, які ніколи не емітить.
- Float/int асиметрія в admitted kernel-формах `gpu_cuda.rs` ніде не
  позначена як навмисна — могла б виглядати недоглядом для майбутнього
  читача.
