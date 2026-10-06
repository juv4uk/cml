# Аудит: GPU на self-hosted runner — стан у `cml` (2026-10-06)

**Статус:** research / аудит, не авторитет. Не змінює контракт чи рішення.
**Дата:** 2026-10-06. **Джерело даних:** `origin/master` @ `40536ea` (типова гілка — `master`). Локальна гілка `feat/476-gpu-worker-queue-evidence` відстає від `master` на 152 коміти, має 2 власні (#475/#476) і конфліктує з уже змерженим кодом; агент її не чіпав.

## Походження і межі довіри

Звіт склав агент-дослідник (ephemeral subagent, лише читання, модель не експонується середовищем). Координатор незалежно **не** перевіряв кожне твердження. Мітки: `SC` (source-confirmed — прочитано в коді/конфігу), `EC` (empirically-confirmed — виконано read-only команду), `NV` (not-verified).

Це перший прохід по **типовій гілці**. Гілки та відкриті PR охоплюються окремим другим проходом.

## Мета

Спільний, безпечний і справді використовуваний GPU-шлях для кількох репо (`sens`, `cml`, `ecosystem`, `fpga-lisp`, `wsm-graalvm`, `radio-log`) на одній фізичній GTX 1050 Ti 4 ГіБ (CUDA 12.6, WSL2).

## A. Що вже є

- Воркер `src/bin/cml-gpu-worker.rs` (`master`): allowlist опкодів `ping`/`probe`/`add-i32`/`chain-file-i32`/`chain-file-i32-provenance`; бінарний протокол CMLG v1, без shell. Обробка послідовна (один запит за раз). (SC)
- Provenance (`repository`/`run_id`/`job`/`case_id`, ASCII, ≤128 байт) і `admission_wait_ns` є лише в опкоді 5. (SC)
- Admission lease (#603 змерджено, `4143dc3`): `src/gpu_admission.rs`, `docs/gpu-admission-lease-v1.md` — `flock(LOCK_EX)` на `/run/cml-gpu-worker/gpu-admission.lock` (env `CML_GPU_ADMISSION_LOCK`) і sidecar `.owner`; крах власника звільняє замок. Тести: `tests/gpu_admission_contention_test.rs`. (SC)
- VRAM preflight `mem_get_info` + `preflight_device_memory` з reserve (#475 закрито): `src/gpu_cuda_runtime.rs:150-170`. Reserve — з `CML_CUDA_MEMORY_RESERVE_BYTES` або `serve --reserve-bytes`; дефолт 0. (SC)
- Кеш-ключі PTX/Driver-JIT на `master` вже передають `Some(nvrtc_version)` і `Some(driver_version)` (`gpu_cuda_runtime.rs:849-885`). Застереження з #370 про `None` для `master` застаріле. (SC)
- Диспетчери `gpu-cuda-live.yml`, `gpu-worker-live.yml` (обидва `workflow_dispatch`): concurrency `gpu-gtx-1050-ti`, runs-on gpu-мітки. (SC)
- `docs/local-gpu-lane.md` і `systemd/cml-gpu-worker.service` (system-unit у репо). (SC)

### Статус задач

| Issue | Стан | Реалізовано / бракує |
|---|---|---|
| #472 | OPEN | Є #603 lease, #475 reserve. Бракує: черги з бюджетом VRAM, health і device identity у кожному результаті, живого multi-client виміру. |
| #535 | OPEN | PR #573 OPEN, не змерджено. На `master` `CML_CUDA_HOST_PROBE` у коді немає (`git grep` порожній). |
| #450 | OPEN | Є adapter #520/#512. Бракує живого запуску в L3-sidecar; свіжий oracle заблоковано SENS #3766/#3777. |
| #597 | OPEN | Лише специфікація; чекає SENS #3845 та futhark #85/#86/#88. |
| #469 | OPEN | Є одна калібровка (однооперційна prepared-map, i32). Матриці CPU/GPU/TIE/BLOCKED немає. |
| #476 | OPEN | Канонічний PR #589 OPEN, CI cancelled; на `master` `CML_GPU_WORKER_TIMING` відсутній. |
| #471 | OPEN | Лише CLAIM від big-pickle; результатів немає. |
| #488–#491 | CLOSED | #496 змерджено; #503 дає бенч. |
| #324, #330 | CLOSED | Резидентний ланцюг і selective DtoH змерджені. |

### Виміри (з коментарів issue; EC лише на їхніх авторах)

- #469: prepared-GPU швидший за `cpu4` у 3,9–6,8× на 1k–10M i32 (cold JIT 30–200 мс).
- #324: ланцюг 1M×4 — 84→16 мс. #330: final-only дає до 6,36×.
- Це лише map і ланцюги i32. Негативних контролів і універсального порога немає.

## B. Що лише специфікація або частково

- Замок допуску реально використовують лише воркер і `sens-futhark`. Прямі `cargo test --include-ignored` у `gpu-cuda-live.yml` замка не беруть (у тестах немає `GpuAdmissionGuard`, SC). Між репо їх розділяє лише concurrency-група, а вона діє в межах одного репо.
- `issue-630-c1-driver.yml` має gpu-мітки і запускається автоматично на same-repo PR, але GPU не використовує (fmt і cargo C1): label-only (SC).
- В інших workflow `cml` немає ні воркера, ні замка.

## C. Що треба доробити (за порядком)

1. Прибрати дублікати працюючих служб (див. D1/D2). Це зміна host-конфігурації; потрібна авторизація.
2. Перевстановити воркер із `master`. Живий бінар від 2026-09-28 (EC, `strings`) не має provenance, reserve, admission-lock, timing. Потрібні reserve ≥ запас для display (наприклад `--reserve-bytes`) і `CML_GPU_ADMISSION_LOCK`.
3. Змерджити #573 (#535) і #589 (#476), тоді прогнати `gpu-worker-live` і зафіксувати `wait`.
4. Додати до `gpu-cuda-live.yml` і майбутніх GPU-job-ів `flock` на `CML_GPU_ADMISSION_LOCK`; для інших репо дати client-обгортку (за зразком `sens-futhark#108`).
5. Закрити прогалини #472: health/device identity у результаті, VRAM-бюджет у черзі.
6. Матриця #469, потім #450 і #597 (залежать від SENS #3845 і futhark #85/#88).

## D. Ризики та суперечності

1. Два однакові `cml-gpu-worker` працюють зараз (EC): system PID 547 (`/run/cml-gpu-worker/worker.sock`) і user PID 964 (`/run/user/1008/cml-gpu-worker.sock`); обидва відповідають `pong`. Коментар #472 від 2026-10-05T22:04Z каже, що user-дублікат вимкнено, але він `enabled` і піднявся після рестарту WSL — суперечність.
2. Два runner-listener на одну реєстрацію `cml` (EC): system `actions.runner.juv4uk-cml.wsm-i5-6400` (PID 625) і user `actions-runner-cml-gpu` (PID 150045). У логах: «A session for this runner already exists».
3. `actions-runner@.service` (core runners) досі вказує на user-сокет, а не на system, всупереч коментарю #472 (SC). Unit у репо вимагає `scripts/cuda-host-profile.sh`, якого немає в `/home/agents/ecosystem/scripts/` (EC): цей unit не стартував би.
4. Замок: файл `gpu-admission.lock` досі не створювався (EC, `ls`). Каталог `/run/cml-gpu-worker` має режим 0700 для `agents`; усі runner-и працюють як uid `agents`, тож проблем із правами немає. Але `RuntimeDirectory=` стирається при stop system-служби, і замок на видаленому inode не виключає новий (predicted).
5. `chain-file-i32` читає й пише довільні шляхи від імені користувача воркера (SC). `wsm-gpu` — окремий WSM-воркер (протокол WSMGPU1, `/tmp/wsm-gpu-1008.sock`, лише `add-one`), зараз DOWN (EC); до воркера `cml` стосунку не має й замка не бере.
6. `nvidia-smi` (EC): зайнято 598/4096 МіБ, display активний (`Disp.A On`), GPU-Util 18%. Reserve за замовчуванням 0, захисту від TDR у коді немає. Два CUDA-контексти (два воркери) їдять VRAM.

## E. Не перевірено

- Мітки core-runners на боці GitHub (`.runner` їх не містить).
- Чи `chain-file-i32-provenance` викликається будь-яким не-`cml` репо.
- Чи проходить #589 CI.
- Вимірювань `wait` при >1 клієнті немає.
- Питання власнику: вимкнути user-воркер і user-runner `cml`? Яке значення reserve для display?

---

## Другий прохід: усі гілки та відкриті PR (2026-10-06)

Джерело: агент-дослідник (лише читання, модель не експонується), після `git fetch origin`; `origin/master` = `40536ea`. Мітки як у першому проході; координатор окремо не перевіряв. Відкритих PR: 21; неузлитих віддалених гілок: 387 (≈55 GPU/runner, решту ≈330 агент не читав — лише імена).

### GPU-дотичні гілки та PR (відст./випер. відносно `origin/master`)

| гілка / PR | a/b | стан | що змінює | вердикт |
|---|---|---|---|---|
| `chatgpt/476-worker-timing-current-master` — PR #589 | 2/112 | DIRTY (конфлікт з `master`), test і verify CANCELLED | поля `server_service_ns`, `client_round_trip_ns`, `wait_protocol_ns` у `cml-gpu-worker.rs`; у `gpu-worker-live.yml` — крок unit-тестів і перевірка timing-полів. Змінної `CML_GPU_WORKER_TIMING` немає. 4 дублікати без PR: `integrate/476-worker-timing`, `requal/gpu-worker-timing-20261005`, `agent/476-worker-timing-{,v2,v3,v4}`, `chatgpt/476-worker-timing-replay` | needs-fix: rebase на `master` (де є #603 і `admission_wait_ns`), перезапустити CI |
| `agent/chatgpt-sol/535-gpu-host-consume-v2` — PR #573 | 8/114 | DIRTY, CANCELLED | `src/gpu_host.rs` (264 рядки), `tests/gpu_host_test.rs`, `CML_CUDA_HOST_PROBE`, fail-closed bootstrap, очищення сокета; конфліктує з `master` і з #589 у `worker.rs`, `gpu-worker-live.yml` | needs-fix: rebase; спершу #589, потім #573 |
| `agent/chatgpt-sol/468-worker-systemd-contract` | 6/118 | без PR | `systemd/cml-gpu-worker.service`, `docs/local-gpu-lane.md`, тест контракту — байт-у-байт ті самі на `master` (EC) | superseded-by `master` |
| `agent/chatgpt-sol/603-gpu-admission-lease` | 20/96 | без PR | замінено змерджованим #603 (`4143dc3`) | superseded-by `master` |
| `perf/gpu-offload-live-20261005` — PR #484 | 7/189 | DIRTY; test FAIL, cuda-live SUCCESS | `gpu-cuda-live.yml` ще й на `pull_request`, авто-requal воркера; суперечить принципу «один runner — одна GPU», замка не бере | needs-owner-decision |
| `ci/cml-cpu-lane` — draft PR #569 | 1/130 | DIRTY | `ci.yml`: `runs-on: [self-hosted, guix, cml-cpu]` — змінює мітки runner-а; runner із міткою `cml-cpu` на хості не перевірявся | needs-owner-decision |
| `feat/474…`, `475…`, `490…`, `491…`, `512…`, `488/489…`, `324…`, `330…`, `319…`, `322…`, `334…`, `316…`, `394…`, `test/318…`, `gpu2-e1/*` | — | без PR | старі версії вже змерджених робіт (кожну окремо не звіряли, NV) | stale / superseded |
| `ops/468-single-gpu-lane`, `ops/472-low-commit-gpu-runner`, `ci/gpu-runner-labels-20260927`, `requal/gpu-cuda-live-20261005`, `fix/369-*`, `fix/gpu-cuda-target-profile-isolation` | — | без PR | ранні варіанти workflow і лейблів `[self-hosted, gpu, gtx-1050-ti]` (NV: чи все вже на `master`) | stale |

Для #597, #469, #471, #450 відкритого PR чи гілки з роботою не знайдено (NV, лише за іменами гілок).

**Не-GPU гілки, що змінюють `runs-on`:** `chatgpt/cml-self-hosted-runner-fix-20261005`, `fix/local-runner-routing-20261005`, `chatgpt/runner-reroute-regression-20261005`, `chatgpt/active-focused-self-hosted`, `ci/main-self-hosted-local-witnesses` (PR #348), `ci/master-self-hosted-private-clones` (PR #378), `fix/route-ubuntu-workflows-to-local-runner`. Усі переводять `ubuntu-*` на `[self-hosted, guix]` або `self-hosted`; GPU не чіпають, але збільшують навантаження на спільні runner-и.

### Локальна гілка `feat/476-gpu-worker-queue-evidence`

- Локальна й `origin/feat/476-…` — той самий SHA `32904ec` (EC); 2/152, конфліктує з `master`; `worker.rs`, `gpu_cuda.rs`, `gpu_cuda_runtime.rs` (+325/−28).
- `1168746`: VRAM reserve #475 (`InsufficientDeviceMemory`) — той самий зміст уже на `master` (`9b67c0c`, `Merge #553`): замінено.
- `32904ec`: timing `service_ns`/`cuda_ns` (#476) — старіша версія #589: замінено; можна відкинути.
- Незакомичених змін у відстежених файлах немає; є 5 неслідкованих (`docs/GPU-RUNNER-AUDIT-2026-10-06.uk.md` — цей файл, `scripts/__pycache__/`, `tests/sens_codes_test.rs`, `tests/sens_primitive_basis_test.rs`, `tests/sens_translation_test.rs`); походження трьох тестів `sens_*` невідоме (NV).

### Оновлений список робіт

- **Вже в польоті:** #573 і #589 — rebase поверх `master` і CI; обидва DIRTY.
- **Не розпочато:** `flock`/lease у `gpu-cuda-live.yml` (у жодній прочитаній гілці немає `flock` чи `GpuAdmissionGuard`; PR #484 радше погіршує); VRAM-бюджет у черзі #472 (частково в #573: host+live device cross-check у `probe`); #469, #450, #597, #471.
- **Host-дії без гілки:** перевстановлення воркера з `master` (краще після мерджу #573 і #589, інакше знову відстане); усунення дублікатів воркерів і listener-ів; `RuntimeDirectory=` і життєвий цикл замка.
- **Виправлення першого проходу:** `scripts/cuda-host-profile.sh` НЕ відсутній — він є на `origin/master` репо `ecosystem` (коміт `7dfe467`, #60); локальний чекаут `ecosystem` позаду на 43 коміти. Потрібне оновлення чекауту (потребує авторизації), а не нова розробка.
- **Нова суперечність** (з `origin/master` ecosystem): `scripts/core-runners.sh` перевіряє `systemctl --user is-active cml-gpu-worker.service` (user-служба), тоді як repo-unit — системний. Потрібне рішення власника.

### Рекомендований порядок (лише рекомендація)

1. Рішення власника: який воркер лишається (system чи user); чи оновлювати чекаут `ecosystem`. 2. Rebase #589 на `master`, CI, merge. 3. Rebase #573 поверх, CI, merge; закрити дублікати timing/535 і локальну `feat/476`. 4. Перевстановити воркер, зафіксувати `wait` на live-прогоні. 5. Додати `flock`/клієнт-обгортку для `gpu-cuda-live.yml`; переглянути #484 до цього. 6. Вирішити #569 (мітка `cml-cpu`) і пачку `runs-on`-гілок.

### Живий стан, перевірений координатором (локальний запуск, найнижчий щабель доказів)

- `cml-gpu-worker` (`~/.local/bin`) без змінної `CML_GPU_WORKER_SOCKET` шукає `/tmp/cml-gpu-worker.sock`, якого немає; з `CML_GPU_WORKER_SOCKET=/run/cml-gpu-worker/worker.sock`: `ping` → `pong`; `probe` → GTX 1050 Ti, CC 6.1, 4294705152 байт; `add-i32 7 1 2 3 4` → `8 9 10 11`. user-воркер (`/run/user/1008/cml-gpu-worker.sock`) відповідає так само.
- `nvidia-smi --query-compute-apps`: обидва процеси (PID 547 системний і 964 user) тримають CUDA-контексти.
- `actions-runner@.service` виставляє `CML_GPU_WORKER_SOCKET=/run/user/1008/cml-gpu-worker.sock` (user-воркер).

---

## Стан після виконаних змін (2026-10-06, вечір)

Розділи вище — знімок на момент аудиту. Далі зафіксовано, що змінилось відтоді:

- **Воркер знову збирається з `gpu-cuda`** (#643, злито): додано явне перетворення `ClientProvenance → AdmissionProvenance` у `src/bin/cml-gpu-worker.rs`. 4 юніт-тести проходять; живий запуск на GTX 1050 Ti дав `ping` → `pong`, `add-i32 7 1 2 3 4` → `8 9 10 11`, ланцюг із provenance та `admission_wait_ns` у відповіді.
- **Автоматична GPU-перевірка** (#644, злито): `.github/workflows/gpu-worker-parity.yml` збирає воркер із голови PR, піднімає його на приватному сокеті (резерв 1 ГіБ, спільний замок) і ганяє `.github/scripts/gpu-worker-parity.py` (копія канонічного скрипта з `juv4uk/sens`). Перший прогін: `GPU_WORKER_PARITY_GREEN`, GTX 1050 Ti, `cuda_ns ≈ 71 мс`. Така перевірка ловила б поломку збірки, що ховалась на `master`.
- **Хост** (перевірено наживо): системний воркер замінено на нову збірку, у юніті `CML_CUDA_MEMORY_RESERVE_BYTES=1073741824` і `CML_GPU_ADMISSION_LOCK=/run/cml-gpu-worker/gpu-admission.lock`; файл замка створено; один CUDA-контекст замість двох. User-воркер `cml-gpu-worker.service` і user-listener `actions-runner-cml-gpu.service` вимкнено. Для runner-юнітів додано drop-in, що вказує на системний сокет (діє після перезапуску runner-ів).
- **Не змінилось:** #573 і #589 лишаються DIRTY і потребують rebase (спершу #589); `flock`/клієнт-обгортка для `gpu-cuda-live.yml`; VRAM-бюджет у черзі (#472); #469, #450, #597, #471. Резерв VRAM задано, але його спрацювання (відмова при нестачі пам'яті) на живій карті не перевірялось, щоб не ризикувати дисплеєм Windows.

---

## English mirror (short)

First-pass audit of the shared CUDA path in `cml` at `origin/master` @ `40536ea`, by a read-only subagent; not independently re-verified by the coordinator. The worker, admission lease and VRAM preflight exist on `master`, but the live worker binary (2026-09-28) predates them, two duplicate workers and two duplicate runner listeners are running, and the admission lock file has never been created. #535/#476 are still open PRs (#573, #589). Only the worker and `sens-futhark` take the lock; direct `cargo test` live workflows do not. Host-configuration changes (dedupe, reinstall, reserve) need owner authorization.
