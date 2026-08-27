# Аудит-нотатка: AGENTS.md стверджує застарілу версію ISA

**Статус:** ЗВІТ (виявлено, не виправлено).
**Джерело:** agent-team аудит суперечностей/застарілих пунктів, повний
звіт — `/home/agents/ecosystem/docs/agent-team-contradiction-audit-2026-08-27.md`
(§2.1).

## Знахідка

`AGENTS.md:53` стверджує: "Tracks an ISA contract (`isa-contract.my`,
version **1.0**) against my-lisp's semantics."

Живий `fpga-lisp/isa-contract.my`: `(version . (1 1))` (піднято комітом
`25c240a`, 2026-08-24 20:40). `cml/compatibility.my` вже коректно
оновлений (`isa . (1 1)`) і перевіряється тестом
`revision_contract_test.rs`, який проходить. Застаріла лише прозова
згадка в `AGENTS.md` — файл ніхто не торкався після 2026-08-23, до бампу
версії.

SUSPECTED (непідтверджено): `docs/fpga-job-protocol.md:4` теж каже
"ISA 1.0" — торкнутий за 20 хв до бампу, можливо описує саме
до-бамповий wire-формат свідомо, а не помилково застарів.

## Виправлення (не зроблено)

Одно-рядкова зміна `AGENTS.md:53`: "1.0" → "1.1".
