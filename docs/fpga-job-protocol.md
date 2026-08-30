# CML to fpga-lisp job protocol

**Status:** M2c command transport, 2026-08-24.
**Wire authority:** `fpga-lisp` ISA 1.0 RTL and monitor protocol.

`FpgaJobV1` describes one already-assembled program image and the register
whose tagged Lisp word is the result. Version 1 deliberately mirrors the live
board instead of inventing a new packet:

```text
bootloader: u16-le instruction count + N u32-le instructions
wait:       board reaches HALT
monitor:    0x01 register -> u32-le tagged result word
monitor:    0x04          -> u32-le { error_flag bit 12, error_pc bits 11:0 }
```

The host validates `1 <= N <= 4095` and register `0..15` before transport side
effects. A successful FIXNUM result is decoded as signed 28-bit two's
complement. Hardware error, unexpected tag, transport failure, invalid job,
and timeout remain distinct outcomes.

`FpgaTransport` owns reset, serial-port access, HALT waiting, framing, exact
reads, and timeouts. CML semantics and the Execution Graph do not know whether
the implementation uses Windows COM4, native Linux serial, simulation, or a
future PCIe transport.

M2b attaches this executor to `ExecutionGraph` through two explicit value
variants: `GraphValue::Buffer` and `GraphValue::LispWord`. `FpgaProgram` emits
only the latter; numeric buffer maps consume and emit only the former. A mock
transport proves graph scheduling, tagged-word preservation, and atomic error
publication.

M2c adds `CommandFpgaTransport`, a shell-free process boundary that writes one
versioned binary job to stdin and accepts one fixed-size binary response from
stdout. The companion `fpga-lisp/job_transport.py` runs under native Windows
Python/pyserial, owns COM4 and timing policy, and preserves the monitor tool's
delayed input-buffer reset workaround. This avoids a Windows command-line
length ceiling and temporary files while keeping serial dependencies outside
CML's language/compiler core.

The ignored live graph test is intentionally operator-gated because the board
still requires a physical RESET press:

```bash
CML_FPGA_LIVE=1 \
CML_FPGA_PYTHON=/mnt/c/Users/user/AppData/Local/Programs/Python/Launcher/py.exe \
CML_FPGA_BRIDGE_WINDOWS='\\wsl.localhost\Ubuntu\home\agents\GitHub\fpga-lisp\job_transport.py' \
cargo test --test execution_graph_fpga_live_test -- --ignored --nocapture
```

Windows PnP reports USB Serial Converter B and COM4 healthy. On 2026-08-24 the
reset-gated test then completed the stronger proof: the registered
`fpga-lisp:com4` executor uploaded `bootstrap_add_demo.bin` to the physical
GW5A-25A board, read R9 as raw tagged word `0x00000007`, observed no hardware
error, and published `GraphValue::LispWord(7)`. The ignored test passed in
13.37 seconds. This is one live program-path observation, not blanket FPGA
backend conformance.

The checked-in fpga-lisp reference fixture corpus follows upstream my-lisp
contract 3.0 so unsupported cases remain visible. That reference pin is not a
capability claim: CML and its FPGA backend still declare contract 2.0 until
named-error conformance exists for every required backend path.

## M3 heterogeneous live graph

One operator-gated graph executed three ordered physical/runtime domains in
14.61 seconds: CPU mapped `[1,2,3]` to `[2,3,4]`, the GTX 1050 Ti CUDA node
mapped that buffer to `[3,4,5]`, then the COM4 FPGA node returned tagged word
`7`. `execution_order()` was exactly CPU → CUDA → FPGA.

This proves one scheduler, dependency graph, backend registry, and atomic
result store can coordinate all three domains. It does **not** prove direct
GPU-to-FPGA data movement: the dependency currently carries ordering only and
the FPGA node executes its own preassembled program image. A typed transfer
edge or shared-memory transport is the next distinct capability.

## M4 data and control edges

Graph validation now rejects a consumed buffer unless it is either a declared
graph input or has a real producer that is also named as a dependency of the
consumer. Source order can no longer accidentally masquerade as data flow.

For current numeric nodes, the producer's typed `GraphValue::Buffer` is
materialized in the host value store before the next executor receives it;
CPU→CUDA is therefore a host-staged data edge. The CUDA→FPGA dependency in the
M3 proof is control-only because `FpgaProgram` has no buffer input. A future
FPGA payload edge must introduce an explicit typed input protocol rather than
reinterpreting this dependency.

---

# CML до fpga-lisp протокол завдань (Ukrainian)

**Статус:** M2c command transport, 2026-08-24.
**Wire authority:** ISA 1.0 RTL `fpga-lisp` та протокол монітора.

`FpgaJobV1` описує один уже асембльований образ програми та регістр, чиє
тег-слово (tagged Lisp word) є результатом. Версія 1 навмисно віддзеркалює
роботу живої плати замість винайдення нового формату пакетів:

```text
bootloader: u16-le кількість інструкцій + N u32-le інструкцій
wait:       плата досягає HALT
monitor:    0x01 регістр -> u32-le теговане слово результату
monitor:    0x04         -> u32-le { error_flag біт 12, error_pc біти 11:0 }
```

Хост перевіряє `1 <= N <= 4095` та регістр `0..15` перед будь-якими побічними
ефектами транспорту. Успішний результат `FIXNUM` декодується як знакове 28-бітне 
число у доповнювальному коді (two's complement). Апаратна помилка, неочікуваний 
тег, збій транспорту, невалідне завдання та тайм-аут залишаються окремими 
наслідками (outcomes).

`FpgaTransport` володіє скиданням (reset), доступом до послідовного порту,
очікуванням HALT, фреймінгом, точним зчитуванням та тайм-аутами. Семантика 
CML та Граф Виконання (Execution Graph) не знають, чи використовує реалізація
Windows COM4, нативний послідовний порт Linux, симуляцію або майбутній 
PCIe транспорт.

M2b приєднує цей виконавець (executor) до `ExecutionGraph` через два явні
варіанти значень: `GraphValue::Buffer` та `GraphValue::LispWord`. `FpgaProgram`
генерує лише останнє; числові вузли (numeric buffer maps) споживають і генерують
лише перше. Mock-транспорт доводить планування графа, збереження тегованого
слова та атомарну публікацію помилок.

M2c додає `CommandFpgaTransport` — вільну від shell (shell-free) межу процесу,
яка записує одне версіоноване бінарне завдання у stdin і приймає одну
бінарну відповідь фіксованого розміру з stdout. Супутній `fpga-lisp/job_transport.py`
запускається під нативним Windows Python/pyserial, володіє COM4 та політикою 
таймінгів, а також зберігає обхідний маневр монітора для затримки скидання 
буфера вводу. Це дозволяє уникнути обмеження довжини командного рядка у Windows 
та тимчасових файлів, зберігаючи залежності послідовного порту поза ядром мови/компілятора CML.

Ігнорований тест живого графа навмисно обмежений (operator-gated), оскільки плата
все ще вимагає фізичного натискання RESET:

```bash
CML_FPGA_LIVE=1 \
CML_FPGA_PYTHON=/mnt/c/Users/user/AppData/Local/Programs/Python/Launcher/py.exe \
CML_FPGA_BRIDGE_WINDOWS='\\wsl.localhost\Ubuntu\home\agents\GitHub\fpga-lisp\job_transport.py' \
cargo test --test execution_graph_fpga_live_test -- --ignored --nocapture
```

Windows PnP повідомляє, що USB Serial Converter B та COM4 справні. 2026-08-24 
цей тест (за умови фізичного скидання) виконав сильніший доказ: зареєстрований
виконавець `fpga-lisp:com4` завантажив `bootstrap_add_demo.bin` на фізичну плату
GW5A-25A, зчитав R9 як сире теговане слово `0x00000007`, не виявив жодних
апаратних помилок і опублікував `GraphValue::LispWord(7)`. Ігнорований тест
успішно завершився за 13,37 секунди. Це одне живе спостереження за виконанням
програми, а не заява про повну відповідність (blanket conformance) FPGA backend-а.

Зафіксований (checked-in) корпус еталонних фікстур `fpga-lisp` дотримується
апстрім-контракту `my-lisp` версії 3.0, тому непідтримувані випадки залишаються
видимими. Ця фіксація (reference pin) не є заявою про повну підтримуваність: CML
та його FPGA backend все ще оголошують контракт 2.0, доки відповідність іменних
помилок (named-error conformance) не буде доведена для кожного необхідного шляху backend-а.

## M3 гетерогенний живий граф

Один operator-gated граф виконав три впорядковані фізичні/виконавчі домени (domains)
за 14,61 секунди: CPU перетворив `[1,2,3]` на `[2,3,4]`, вузол GTX 1050 Ti CUDA
перетворив цей буфер на `[3,4,5]`, після чого вузол COM4 FPGA повернув теговане слово
`7`. `execution_order()` був точно таким: CPU → CUDA → FPGA.

Це доводить, що один планувальник (scheduler), граф залежностей, реєстр бекендів
та атомарне сховище результатів можуть координувати всі три домени. Це **не**
доводить пряму передачу даних GPU-в-FPGA: залежність наразі несе лише інформацію
про порядок виконання, а FPGA-вузол виконує власний попередньо зібраний образ програми. 
Типізована передача даних (typed transfer edge) або транспорт через спільну пам'ять
(shared-memory transport) — це окрема, майбутня можливість.

## M4 ребра даних та керування (data and control edges)

Валідація графа тепер відхиляє спожитий (consumed) буфер, якщо він не є оголошеним
входом графа, або не має реального вузла-виробника (producer), який також зазначений
як залежність споживача (consumer). Порядок у сирцях більше не може випадково маскуватися
під потік даних (data flow).

Для поточних числових вузлів, типізоване значення `GraphValue::Buffer` від
вузла-виробника матеріалізується у сховищі значень хоста (host value store) перед
тим, як наступний виконавець його отримає; отже, CPU→CUDA є поетапним 
host-staged ребром даних. Залежність CUDA→FPGA у доказі M3 є виключно ребром
керування (control-only), оскільки `FpgaProgram` не має буфера на вході. 
Майбутнє ребро даних для FPGA-корисного навантаження (payload edge) має впровадити
явний типізований протокол вводу, а не переосмислювати цю залежність.
