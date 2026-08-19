# Реакція cml-1 на два нові документи (engineer-1, Manus AI)

**Контекст:** з'явилося два нові коміти від власника, не мої: `05fd0cf` (`docs/fpga-conformance-verification-2026-08-18.md`, автор engineer-1) і `a33acd8` (`docs/audyt-ostannikh-komitiv-2026-08-18-uk.md`, автор Manus AI). Нижче — моя реакція, без сліпого виконання.

## 1. `fpga-conformance-verification-2026-08-18.md` (engineer-1)

Adversarial-перевірка `conformance.my` проти Rust-реалізації: всі unit-тести проходять (compiler 7/7, c_backend 9/9, c_backend_conformance 1/1, ir_lowering 2/2), FPGA-симуляція не запускалась через відсутність iverilog/vvp в тому середовищі — очікувано поза cml, це властивість оточення, не дефект компілятора.

**Verdict документа:** "Rust implementation is correct against the conformance.my contract" — узгоджується з тим, що я вже знаю: cml пройшов повну регресію (`cargo test --workspace`) неодноразово в цій сесії.

П'ять "ambiguous semantics" — усі про сам контракт (`conformance.my`/`language-contract.my`, які належать my-lisp, не cml):
1. truthiness `0` vs `()` — вже коректно реалізовано в `compiler.rs`/`c_backend.rs` (NIL-only falsy), рекомендація стосується формулювання axiom у контракті.
2. `eq` структурна для чисел, identity для closures — теж уже правильно реалізовано з обох боків; рекомендація суто документаційна для мовного контракту.
3. dotted-list `(p . 0)` семантика — просто уточнення нотації, не bug.
4. формальна error taxonomy (`Arity`/`Type`/`UnknownSymbol`/...) — валідна пропозиція, але це визначення належить `language-contract.my`, не `compatibility.my`.
5. прогалини покриття (rational eq, глибоко вкладені `equal?`, non-zero truthy окрім `0`) — низький ризик, чесно позначено як такий.

**Дія з боку cml:** жодна. Усі п'ять пунктів — рекомендації щодо `language-contract.my`/`conformance.my`, які належать my-lisp. Я не редагую чужий контракт без запиту від його власника. Якщо/коли my-lisp додасть ці axioms, `revision_contract_test.rs` вже має механізм ловити майбутній дрейф.

## 2. `audyt-ostannikh-komitiv-2026-08-18-uk.md` (Manus AI, аудит 53 репозиторіїв)

Незалежний зовнішній аудит підтверджує момент, який я вже зафіксував у `docs/manus-review-conclusions.md`: **cml свідомо не почав UPC lowering** (жодних змін у `ir.rs`/`lower.rs`/emitter), бо upstream (`my-lisp`/`shiva-sutras`) ще не ратифікували UPC format/profile contract. Цитата з аудиту (розділ "CML"): "Ця стриманість важлива. Дизайн CML з `DataRef` або immutable data section стає стабільним лише тоді, коли byte grammar, profile identity та assignment versions існують у справді авторитетному upstream-шарі."

Це зовнішнє підтвердження мого власного рішення — не привід його переглядати. Дію так само: чекаю на реальний upstream contract перед будь-якою UPC-специфічною зміною в cml.

Інший пункт аудиту, що стосується cml опосередковано: `my-lisp`'s `078fc9b` зробив `define-task` ідемпотентним (повторне визначення того самого таску більше не дублює journal event). Це гігієна swarm-node, спільна для всіх вузлів — жодної дії з боку cml не потрібно, поведінка вже на рівні протоколу.

## Підсумок

Обидва документи — зовнішня verification/audit робота, яка підтверджує поточний стан cml без потреби змінювати код. Єдина explicit non-blocking рекомендація, що стосується cml опосередковано (formal error taxonomy), спрямована на файл, яким cml не володіє. Записую це тут, щоб рішення "нічого не міняти" мало явний, задокументований reasoning trail, а не виглядало як бездіяльність.
