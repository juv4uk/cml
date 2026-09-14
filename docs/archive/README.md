# Архів документації — не-нормативний (Archive — Non-normative)

**Жоден документ у `docs/archive/` (та його піддиректоріях) не є джерелом чинних вимог чи специфікацій.**  
Усі матеріали тут зберігаються виключно для історичного контексту, аналізу та аудиту еволюції рішень. При будь-якому конфлікті між архівними документами та актуальними матеріалами беззастережно діють чинні документи з [`docs/CURRENT.md`](../CURRENT.md), канонічні контракти `my-lisp` та поточний код компілятора.

**Nothing in `docs/archive/` (or its subdirectories) is authoritative.**  
Everything here is preserved for historical context, retrospective analysis, and decision provenance only. On any conflict between an archived document and current material, current material wins unconditionally — active contracts from [`docs/CURRENT.md`](../CURRENT.md), `my-lisp` Canon/semantic-registry, and the actual codebase.

---

## Структура архіву (Archive Subdirectories)

### 1. `reviews/`
Історичні огляди, зовнішні аудити, реакції та рецензії агентами й людьми за серпень–вересень 2026 року:
* Огляди архітектури (Manus, Viveka, екосистемні аудити).
* Звіти та коментарі на попередні етапи розвитку CML.

### 2. `historical/`
Документи та звіти, які зафіксували стан репозиторію на певний момент у минулому, або чий контекст було закрито чи інтегровано в код:
* Опитування коду та документації (Code & Doc surveys).
* Ранні нотатки досліджень та архітектурні звіти.

### 3. `superseded/`
Документи, зміст яких повністю замінено новими рішеннями або застарів:
* Концепції та інтеграції UPC / UPC8 (повністю витіснені чистим Lisp + Canon).

### 4. `experiments/`
Пропозиції, спайки та прототипи, які завершили свою фазу або не були затверджені для продакшн-шляху:
* Серпневі прототипи та триажі.
* Пропозиції оптимізації запуску.

---

*Цей архів створено в межах загальноекосистемної політики `DOC-AUTHORITY-ARCHIVE` (`juv4uk/ecosystem#5`, задача `cml#17`).*
