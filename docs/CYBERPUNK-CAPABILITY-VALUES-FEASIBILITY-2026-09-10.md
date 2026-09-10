# Дослідження: чи готовий cml codegen до Tag::Capability-значень у мод-скриптах

**Автор:** cml-1
**Дата:** 2026-09-10
**Статус:** дослідження (feasibility), НЕ реалізація

## Питання

Якщо AOT-компільований мод-скрипт колись використовуватиме capability-значення
(не лише fixnum/symbol/string) — чи codegen cml вже вміє емітувати
`Tag::Capability`-закодовані константи/виклики?

## Метод

Прочитав реальний код `src/x86_freestanding.rs` (не документацію) — конкретно
`platform_call_contract`, `emit_platform_call`, і місця виклику в preflight.
Перевірив, чи `c_backend.rs` (окремий representation domain) має щось подібне.

## Висновок: так, є проте вже проваджений патерн — але з важливим уточненням

**x86_freestanding.rs вже має робочий, перевірений механізм для
capability-подібних значень** — 5 реальних прикладів уже існують і проходять
тести:

```rust
fn platform_call_contract(func: &Ir) -> Option<(&'static str, usize, &'static str)> {
    match name.as_str() {
        "PCI-CONFIG-CAPABILITY" => Some(("pci-config-capability", 0, "wsm_pci_config_capability")),
        "PCI-CONFIG-READ16"     => Some(("pci-config-read16", 5, "wsm_pci_config_read16")),
        "MMIO-CAPABILITY"       => Some(("mmio-capability", 0, "wsm_mmio_capability")),
        "MMIO-READ32"           => Some(("mmio-read32", 2, "wsm_mmio_read32")),
        "MMIO-WRITE32"          => Some(("mmio-write32", 3, "wsm_mmio_write32")),
        _ => None,
    }
}
```

`emit_platform_call` викликає відповідний `wsm_*` extern-імпорт з опаковим
`RuntimeContext*` (r12) як першим аргументом, до 5 додаткових аргументів через
SysV-регістри (`%rsi %rdx %rcx %r8 %r9`), і повертає значення в `%rax` **без
жодної інтерпретації** — cml ніколи не декодує чи не конструює
`Tag::Capability`-бітовий патерн сам. Це повністю відповідальність
`wsm_*`-функцій на боці рантайму (wsm-os-lisp/wsm-my-lisp).

## Важливе уточнення: це allowlist за іменем, не generic-механізм

Це **не** "codegen розуміє Capability як тип даних" — це жорстко закодований
список конкретних імен символів (`PCI-CONFIG-CAPABILITY`, `MMIO-*`), кожне з
власною арністю й ABI-імпортом. Довільне нове ім'я capability-виклику НЕ
запрацює автоматично — воно має бути:

1. Додане як новий рядок у `platform_call_contract`'s match (механічно,
   кілька рядків коду);
2. Відповідний `wsm_*` extern-імпорт має існувати й бути частиною
   ратифікованого target ABI (той самий принцип "ratify-then-consume", який
   уже застосований до `TAG_BOXED`) — cml не винаходить нові runtime-імпорти
   сам, лише споживає вже узгоджені.
3. Арність обмежена: `emit_platform_call` підтримує максимум 5 аргументів
   (фіксований список SysV-регістрів), без варіативності.

## c_backend.rs: нуль підтримки, і це правильно

`c_backend.rs` — окремий, самодостатній C tagged-union рантайм, **не
споживає wsm-target-contract взагалі** (уже встановлено в попередньому
дослідженні ABI). Він не має жодного поняття Capability, PCI, MMIO чи
platform-виклику. Якби мод-скрипт для Cyberpunk колись компілювався через
C-бекенд (а не x86_freestanding), знадобився б **окремий, симетричний**
механізм — не перевикористання `platform_call_contract`, бо той прив'язаний
до x86-специфічного calling convention і `RuntimeContext*` контракту.

## Практичний висновок для Cyberpunk-теми

Якщо колись знадобиться, наприклад, `(game-entity-capability)` →
`(entity-get-health capability)`, шаблон уже є й перевірений — додавання
нового capability-виклику в x86_freestanding.rs є маленькою, механічною
зміною (аналогічною додаванню MMIO-READ32), **за умови**, що:

- відповідний `wsm_*` runtime-імпорт спершу ратифікований у
  wsm-target-contract/wsm-my-lisp (не cml вирішує форму цього імпорту);
- арність не перевищує 5 аргументів;
- це компілюється через x86_freestanding.rs, не c_backend.rs (для
  останнього такого шляху взагалі не існує).

Це не рекомендація щось починати зараз — лише підтвердження, що ґрунт уже є,
і показ точного місця й форми, куди така робота лягла б, коли реальна
потреба виникне.
