# Власний оптимізувальний компілятор SENS — перший експериментальний прохід

**Дата:** 12.10.2026. **Статус:** дослідний прохід у чинному master CML,
НЕ зміна виробничої семантики SENS, НЕ готовий native/GPU/FPGA бекенд.

## Чому не копія SBCL

SBCL є корисним донором архітектури оптимізувального компілятора:
проміжні представлення, аналіз, проходи й спеціалізовані бекенди.
Але CML/SENS не приймає семантику Common Lisp. Значення операції
визначає тільки виконуваний закон SENS та її точна DomainIdentity,
не англійська назва, історична восьмибітна адреса чи хостова істинність.

Схема:

    T5 .sens ----> canonical T5 reader SENS <---- F3 .senc
                      ^                         SHA з довіреного manifest
                      |
              точний DomainIdentity
                      |
             виконуваний закон D3/D4
                      |
              SENS AST → CML IR
                      |
              O1: SENS-оптимізатор
             /        |         \
          перевірка  свідчення   cost
          ролей      правил     IR-вузлів
                      |
             окремі target бекенди
             (ще потрібні тести)

T5 залишається типовим форматом, F3 — тільки явно для малих програм,
із перевіркою фізичного SHA-256 перед декодуванням. Нових форматів
носія, резидентів або значень доменів O1 не вводить.

## O1 — безпечні селектори цитованої пари

Код: src/current_exact_optimizer.rs.
Два правила:

    HEAD(PAIR(QUOTE a, QUOTE b)) => QUOTE a
    TAIL(PAIR(QUOTE a, QUOTE b)) => QUOTE b

HEAD, TAIL, PAIR і QUOTE тут лише пояснювальні імена ролей,
НЕ текстове джерело SENS. Алгоритм отримує всі ролі з
SENS executable compiler law через CoreDomainIdentity і перевіряє
мовний SHA-256. Механізм і кількість аргументів мають збігатися.

Обидва аргументи щойно створеної пари зобов'язані бути QUOTE,
інакше оптимізація заборонена. Зокрема,
HEAD(PAIR(expr, expr)) не скорочується: другий expr може
викликати ефект або помилку, які не можна приховувати.

Не оптимізуємо COND, ATOM/EQ, істинність D1, D24+ числа,
замикання та невідомі домени. Залишковий список не стає
виконуваною головою. Цитоване залишається даними.

Для безпечного шаблону наша локальна модель ціни фіксує
зменшення IR із 4 до 1 вузла. Це НЕ виміряне прискорення
CPU/GPU/FPGA та НЕ економія байтів фізичного T5/F3.

## Доказова дисципліна

Кожний rewrite створює Replayable RewriteWitness: закон,
чотири точні доменні ідентичності, SHA контракту, вартість до/після.
Є детермінована replay-перевірка. Це журнал обґрунтованих кроків,
а НЕ формальний доказ для довільної програми. Тест оракула SENS
порівнює виконання початкової програми з незалежною еквівалентною
програмою для обох селекторів.

Захисні умови:
- Підроблена роль/хеш, неправильна арність або механізм — відмова.
- Завелика/занадто глибока структура — відмова.
- Немає допустимих правил — результат тотожний оригінальному IR.
- Фізичні байти та контракт SENS не переписуються.
- CI зеленіє лише після оракульної перевірки O1 та існуючого
  повного корпусу 23 T5 / 9 SHA-verified F3.

Тести: integration/current-sens-transport/tests/optimizer_exact_law.rs.
CI: .github/workflows/current-sens-transport-candidate.yml.

## Відтворення експерименту

Після checkout чинного master CML із точною кандидатною SENS-залежністю:

    cargo test --manifest-path integration/current-sens-transport/Cargo.toml \
      --test optimizer_exact_law -- --nocapture

    cargo run --manifest-path integration/current-sens-transport/Cargo.toml \
      --bin optimize_exact -- --t5 program.sens

    cargo run --manifest-path integration/current-sens-transport/Cargo.toml \
      --bin optimize_exact -- --f3 tiny.senc TRUSTED_PHYSICAL_SHA256

**Важливо:** TRUSTED_PHYSICAL_SHA256 потрібно брати з незалежного
довіреного / підписаного manifest. Оптимізатор не виводить digest з
неперевіреного файлу й не визначає F3 автоматично. Команди лише
аналізують typed IR і друкують rewrite witnesses — не записують
новий фізичний файл і не генерують native виконуваний код.

[Кандидатний GitHub CI #38188542026](https://github.com/juv4uk/cml/actions/runs/38188542026)
**SUCCESS**: 7/7 dedicated optimizer tests, D3 HEAD і TAIL диференційно
порівняні з оракулом SENS, кожне правило 4→1 IR вузлів,
T5/F3 CLI positive та wrong-SHA/format negative; повний 23 T5 / 9 F3
корпус лишився green.
[Першоджерельні журнали і stage-manifest](https://github.com/juv4uk/cml/actions/runs/38188542026/artifacts/11719934985).

## Подальший розвиток

1. Окремі O0/O1 бенчмарки: вартість аналізу, allocations, IR,
   тільки потім runtime на реальних native-бекендах.
2. Версійний, SENS-owned каталог допустимих rewrite-законів.
3. Доведені правила для числових доменів і замикань — не вгадувати.
4. Повна міграція історичних CML backend споживачів; лише після
   differential parity — рішення про production cutover.

## Актуальні дослідження

- SBCL, compiler/news (архітектурний донор): https://www.sbcl.org/news.html
- Willsey та ін., egg: Fast and Extensible Equality Saturation,
  Communications of the ACM, 29.07.2026:
  https://doi.org/10.1145/3815481
- Yin та ін., HEC: Equivalence Verification Checking for Code
  Transformation via Equality Saturation, USENIX ATC 2025:
  https://www.usenix.org/conference/atc25/presentation/yin
- Cranelift, IR/backend verification: https://cranelift.dev/
