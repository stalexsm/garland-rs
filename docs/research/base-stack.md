# Базовый no_std-стек Rust для ESP32-C6 (esp-hal 1.2.x) — состояние на сегодня

Дата исследования: по состоянию на релизы 2026-09-18. Все версии проверены по crates.io API и
первоисточникам (docs.rs, GitHub esp-rs с указанием тега и файла, официальная книга esp-rs).

## 1. Актуальные версии, MSRV, тулчейн

| Крейт | Версия | Дата публикации | rust-version (MSRV) |
|---|---|---|---|
| [esp-hal](https://crates.io/crates/esp-hal/versions) | **1.2.2** | 2026-09-18 | [`1.95.0`](https://github.com/esp-rs/esp-hal/blob/esp-hal-v1.2.2/esp-hal/Cargo.toml) |
| [esp-rtos](https://crates.io/crates/esp-rtos/versions) | **0.4.0** | 2026-08-26 | `1.95.0` ([Cargo.toml @ esp-hal-v1.2.2/esp-rtos](https://github.com/esp-rs/esp-hal/blob/esp-hal-v1.2.2/esp-rtos/Cargo.toml)) |
| [esp-backtrace](https://crates.io/crates/esp-backtrace/versions) | **0.20.0** | 2026-08-26 | 1.95.0 ([тег esp-backtrace-v0.20.0](https://github.com/esp-rs/esp-hal/blob/esp-backtrace-v0.20.0/esp-backtrace/Cargo.toml)) |
| [esp-alloc](https://crates.io/crates/esp-alloc/versions) | **0.11.0** | 2026-08-26 | 1.95.0 ([тег esp-alloc-v0.11.0](https://github.com/esp-rs/esp-hal/blob/esp-alloc-v0.11.0/esp-alloc/Cargo.toml)) |
| [esp-storage](https://crates.io/crates/esp-storage/versions) | **0.10.0** | 2026-08-26 | 1.95.0 ([тег esp-storage-v0.10.0](https://github.com/esp-rs/esp-hal/blob/esp-storage-v0.10.0/esp-storage/Cargo.toml)) |
| [esp-println](https://crates.io/crates/esp-println/versions) | **0.18.0** | 2026-08-26 | 1.95.0 ([тег esp-println-v0.18.0](https://github.com/esp-rs/esp-hal/blob/esp-println-v0.18.0/esp-println/Cargo.toml)) |
| [esp-generate](https://crates.io/crates/esp-generate/versions) | **1.4.0** | 2026-09-18 | тег [`v1.4.0`](https://github.com/esp-rs/esp-generate/blob/v1.4.0/Cargo.toml) |
| [esp-radio](https://crates.io/crates/esp-radio/versions) (beta) | **1.0.0-beta.1** | 2026-09-16 | [`1.95.0`](https://github.com/esp-rs/esp-hal/blob/esp-radio-v1.0.0-beta.1/esp-radio/Cargo.toml) |

Сопутствующие (по [официальному лендингу esp-rs docs](https://docs.espressif.com/projects/rust/)):
esp-bootloader-esp-idf 0.6.0, esp-config 0.8.0, esp-phy 0.3.0, esp-radio-rtos-driver 0.4.2, esp-rom-sys 0.1.5.
Крейта **esp-printf не существует** (поиск crates.io пуст, в монорепо esp-hal его тоже нет) —
функции печати/логирования выполняет esp-println 0.18.0 (см. «Открытые вопросы»).

История esp-hal 1.x: 1.0.0 (2025-10-30) → 1.1.x (2026-04…08) → 1.2.0 (2026-09-02) → 1.2.2 (2026-09-18),
см. [список версий crates.io](https://crates.io/crates/esp-hal/versions) и
[CHANGELOG @ esp-hal-v1.2.2](https://github.com/esp-rs/esp-hal/blob/esp-hal-v1.2.2/esp-hal/CHANGELOG.md).

**Тулчейн для ESP32-C6 (RISC-V):** stable (или nightly) + компонент `rust-src` +
`rustup target add riscv32imac-unknown-none-elf`. **espup не нужен** — он требуется только для
Xtensa-чипов (ESP32/S2/S3). Источник: книга «The Rust on ESP Book», разделы
[RISC-V Devices / Xtensa Devices](https://docs.espressif.com/projects/rust/book/print.html)
(репозиторий книги: [esp-rs/book](https://github.com/esp-rs/book)). Шаблон esp-generate фиксирует то же
в [`rust-toolchain.toml`](https://github.com/esp-rs/esp-generate/blob/v1.4.0/template/rust-toolchain.toml):
`channel = "stable"`, `components = ["rust-src"]`, `targets = ["riscv32imac-unknown-none-elf"]`.

## 2. Что генерирует esp-generate 1.4.0 для esp32c6 + wifi + embassy

Инструмент: `cargo install esp-generate --locked`
([книга](https://docs.espressif.com/projects/rust/book/print.html), раздел esp-generate).
Каждая версия esp-generate жёстко привязана к версии экосистемы: «Each version of esp-generate targets
a specific version of the ecosystem crates» — [книга, What esp-generate Сonfigures](https://docs.espressif.com/projects/rust/book/print.html).

### 2.1. Логика опций (TUI) — файл [template/template.yaml @ v1.4.0](https://github.com/esp-rs/esp-generate/blob/v1.4.0/template/template.yaml)

- опция `wifi` **требует**: `alloc` + `unstable-hal` + `embassy`;
- опция `embassy` **требует**: `unstable-hal`;
- `unstable-hal` = фича `unstable` крейта esp-hal («no stability guarantees» — текст help в template.yaml);
- логирование — взаимоисключающие `log` (фронтенд esp-println + espflash) или `defmt` (фронтенд rtt-target + probe-rs);
  паник-хендлер — `esp-backtrace` (espflash) или `panic-rtt-target` (probe-rs).

То есть проект «c6 + wifi + embassy» всегда получает esp-hal с фичей `unstable`, esp-alloc, esp-rtos, esp-radio.

### 2.2. Эффективный [Cargo.toml](https://github.com/esp-rs/esp-generate/blob/v1.4.0/template/Cargo.toml)
(вариант esp32c6 + wifi + embassy + alloc + log, разрешённые шаблонные условия):

```toml
[package]
name = "project-name"
version = "0.1.0"
edition = "2024"
rust-version = "1.95"

[dependencies]
esp-hal = { version = "~1.2.2", features = ["esp32c6", "unstable", "log-04"] }
esp-rtos = { version = "0.4.0", features = [
    "esp-radio", "esp-alloc", "embassy", "esp32c6", "log-04",
] }
esp-bootloader-esp-idf = { version = "0.6.0", features = ["esp32c6", "log-04"] }
log = "0.4.27"
esp-println = { version = "0.18.0", features = ["esp32c6", "log-04"] }
esp-backtrace = { version = "0.20.0", features = ["esp32c6", "panic-handler", "println"] }
esp-alloc = { version = "0.11.0", features = ["esp32c6"] }
embedded-io = "0.7.1"
embedded-io-async = "0.7.0"
embassy-net = { version = "0.9.1", features = ["tcp", "udp", "dhcpv4", "medium-ethernet", "log"] }
smoltcp = { version = "0.14.0", default-features = false, features = [
    "medium-ethernet", "multicast", "proto-dhcpv4", "proto-dns", "proto-ipv4",
    "socket-dns", "socket-raw", "socket-tcp", "socket-udp", "socket-icmp", "log",
] }
esp-radio = { version = "1.0.0-beta.1", features = [
    "esp32c6", "wifi", "esp-alloc", "unstable", "log-04",
] }
embassy-executor = { version = "0.10.0", features = ["log"] }
embassy-time = { version = "0.5.0", features = ["log"] }
static_cell = "2.1.1"
critical-section = "1.2.0"

[profile.dev]
opt-level = "s"
[profile.release]
codegen-units = 1
debug = 2
lto = 'fat'
opt-level = 's'
```

Обратите внимание: у esp-hal стоит `~1.2.2` (диапазон 1.2.x), у остальных — точные минорные версии.

### 2.3. Структура проекта ([дерево template/ @ v1.4.0](https://github.com/esp-rs/esp-generate/tree/v1.4.0/template))

```
.cargo/config.toml      # target riscv32imac-unknown-none-elf, runner (espflash/probe-rs),
                        # rustflags: -C force-frame-pointers (для esp-backtrace); include esp-config.toml
.cargo/esp-config.toml  # настройки esp-config (ESP_*_CONFIG_*)
build.rs                # линкер-скрипты: -Tlinkall.x (последним), -Tdefmt.x при defmt
rust-toolchain.toml     # stable + rust-src + riscv32imac-unknown-none-elf
src/bin/main.rs         # блокирующий вариант; при embassy — вариант из async_main.rs
src/lib.rs              # #![no_std] (общий код)
tests/                  # embedded-test (если выбран)
Embed.toml, wokwi.toml, diagram.json, .vscode/, .github/workflows/rust_ci.yml (опция ci)
```

Источники: [.cargo/config.toml](https://github.com/esp-rs/esp-generate/blob/v1.4.0/template/.cargo/config.toml),
[build.rs](https://github.com/esp-rs/esp-generate/blob/v1.4.0/template/build.rs).

## 3. Флаги `unstable` / `requires-unstable` в esp-hal 1.2

Определения в [esp-hal/Cargo.toml @ esp-hal-v1.2.2](https://github.com/esp-rs/esp-hal/blob/esp-hal-v1.2.2/esp-hal/Cargo.toml):

- `default = ["rt", "exception-handler", "float-save-restore"]`;
- `unstable` — «Enables APIs that are not stable and thus come with no stability guarantees.
  **Never enable this feature in a library crate** using esp-hal» (подключает embassy-embedded-hal,
  embedded-io 0.6/0.7, rand_core, digest и пр.);
- `requires-unstable` — «**Libraries** that depend on esp-hal should enable this feature to indicate
  their use of unstable APIs. However, they must **not** enable the `unstable` feature themselves»
  (для разработки — esp-hal как dev-dependency с `unstable`).

Кто что включает: esp-rtos 0.4.0 зависит от esp-hal `~1.2.0` с фичами `["requires-unstable", "rt"]`
([esp-rtos/Cargo.toml](https://github.com/esp-rs/esp-hal/blob/esp-hal-v1.2.2/esp-rtos/Cargo.toml));
esp-radio пишет в доках: «Note that this crate **currently requires you to enable the unstable feature
on esp-hal**» ([docs.rs/esp-radio/1.0.0-beta.1](https://docs.rs/esp-radio/1.0.0-beta.1/esp_radio/)).

**RMT в 1.2.2:** основной API стабилен — в [rmt.rs](https://github.com/esp-rs/esp-hal/blob/esp-hal-v1.2.2/esp-hal/src/rmt.rs)
`#[instability::unstable]` стоит только на `Rmt::set_interrupt_handler` (строка 942) и
`impl InterruptConfigurable for Rmt` (строка 953). Для обычной генерации/приёма RMT-сигналов `unstable`
на esp-hal не нужен; нужен, если требуется свой обработчик прерываний RMT или esp-radio.
«Отложенных задач» как отдельного API в esp-hal 1.2.2/esp-rtos 0.4.0 нет — в esp-hal-procmacros 0.23
нет макросов `task`/deferred, esp-rtos публикует только `main` и `CurrentThreadHandle`
([esp-rtos/src/lib.rs](https://github.com/esp-rs/esp-hal/blob/esp-hal-v1.2.2/esp-rtos/src/lib.rs)) —
см. «Открытые вопросы».

## 4. esp-rtos vs embassy-executor: официальная рекомендация (для Wi-Fi)

- Официальная книга: «esp-radio relies on an implementation of **esp-radio-rtos-driver**, which defines
  the scheduling and runtime interface the stack needs to run reliably. **esp-rtos is the default backend
  we ship and support**; … esp-generate will add esp-rtos automatically»
  ([The Rust on ESP Book, Ancillary Crates Overview](https://docs.espressif.com/projects/rust/book/print.html)).
- esp-rtos — не альтернатива embassy-executor, а его интеграция: «The esp-rtos crate provides integration
  between esp-hal and the Embassy asynchronous framework. It provides support for: Interrupt-mode
  executor, Multicore-aware thread-mode embassy executor, Embassy time driver, Timer waiter queue»
  ([книга, Async Options → Embassy](https://docs.espressif.com/projects/rust/book/print.html)).
- Правило из доков esp-rtos: «enable the `embassy` feature, and make the main function `async` …
  to create async tasks, you will need the `task` macro from the **embassy-executor** crate.
  **Do NOT enable any of the `arch-*` features on embassy-executor**»
  ([esp-rtos/src/lib.rs @ esp-hal-v1.2.2](https://github.com/esp-rs/esp-hal/blob/esp-hal-v1.2.2/esp-rtos/src/lib.rs)).

### Минимальный async main (шаблон esp-generate, [src/bin/async_main.rs @ v1.4.0](https://github.com/esp-rs/esp-generate/blob/v1.4.0/template/src/bin/async_main.rs))

```rust,ignore
use esp_hal::clock::CpuClock;
use esp_hal::timer::timg::TimerGroup;
use embassy_executor::Spawner;

esp_bootloader_esp_idf::esp_app_desc!();

#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 65536);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0, peripherals.FROM_CPU_INTR0);

    // … Wi-Fi/BLE инициализация, spawner.spawn(…), embassy_time::Timer::after(…) …
    loop { /* … */ }
}
```

Тот же контракт зафиксирован в доках esp-rtos: таймер `TimerGroup` + `FROM_CPU_INTR0` +
`esp_rtos::start(...)`, затем можно стартовать esp-radio
([esp-rtos/src/lib.rs, раздел Setup](https://github.com/esp-rs/esp-hal/blob/esp-hal-v1.2.2/esp-rtos/src/lib.rs)).
Прочее из доков esp-rtos: async-драйверы esp-hal не `Send` (привязаны к ядру через прерывания);
экспериментальный automatic light sleep через `esp_rtos::sleep::configure` + `start_with_idle_hook`;
публичный API задач — `esp_rtos::task::CurrentThreadHandle` (`delay`, `delay_until`, `set_priority`).

## 5. esp-radio (beta): подключение и матрица совместимости

Зависимости esp-radio 1.0.0-beta.1 ([docs.rs](https://docs.rs/esp-radio/1.0.0-beta.1/esp_radio/),
[Cargo.toml @ esp-radio-v1.0.0-beta.1](https://github.com/esp-rs/esp-hal/blob/esp-radio-v1.0.0-beta.1/esp-radio/Cargo.toml)):

| Зависимость | Требование | Роль |
|---|---|---|
| esp-hal | `~1.2.0` | ядро HAL; нужна фича `unstable` на стороне приложения |
| esp-radio-rtos-driver | `0.4.2` | интерфейс планировщика; реализацию даёт esp-rtos |
| esp-phy | `0.3.0` | PHY Wi-Fi/BLE |
| esp-wifi-sys-* | `0.3.0`, git rev `2ea8e3e` | прекомпилированные blobs (esp-wireless-drivers-3rdparty v5.1.2-602) |
| esp-alloc | `0.11.0`, опционально, в `default` | malloc/free для blobs (фича `esp-alloc`) |
| esp-rtos | `0.4.0` (dev-dep) | эталонная реализация rtos-driver |

Матрица версий (по tags/Cargo.toml и crates.io):

| Пакет | Версия | Совместим с esp-hal | Статус |
|---|---|---|---|
| esp-rtos | 0.4.0 | `~1.2.0` (1.2.0…1.2.2) | актуальная, требует фичу `esp-radio` для радиостека |
| esp-radio | 1.0.0-beta.0 | `~1.2.0` | 2026-06-03 |
| esp-radio | **1.0.0-beta.1** | `~1.2.0` | **актуальная** (2026-09-16) |
| esp-radio | 1.0.0-beta.2 | — | **yanked** (2026-09-17); именно её отдаёт docs-сайт |

Ссылки: [crates.io esp-radio/versions](https://crates.io/crates/esp-radio/versions),
[лендинг docs.espressif.com/projects/rust](https://docs.espressif.com/projects/rust/) (esp-radio 1.0.0-beta.2),
[esp-radio @ esp-hal-v1.2.2](https://github.com/esp-rs/esp-hal/blob/esp-hal-v1.2.2/esp-radio/Cargo.toml) (в дереве 1.0.0-beta.0).

Ключевые следствия для приложения:

1. esp-radio не собирается без реализации esp-radio-rtos-driver: без esp-rtos линкер падает на
   `esp_rtos_*` — шаблон esp-generate прямо подсказывает: «esp-radio has no scheduler enabled.
   Make sure you have initialized esp-rtos or provided an external scheduler»
   ([build.rs @ v1.4.0](https://github.com/esp-rs/esp-generate/blob/v1.4.0/template/build.rs)).
2. Порядок запуска: `esp_rtos::start(...)` **до** `esp_radio::init()` (пример Setup в доках esp-rtos).
3. Новое Wi-Fi API в бете (на шаблоне): `esp_radio::wifi::WifiController::new(peripherals.WIFI, Default::default())`
   и `esp_radio::wifi::Interface::station()`
   ([async_main.rs @ v1.4.0](https://github.com/esp-rs/esp-generate/blob/v1.4.0/template/src/bin/async_main.rs)).
4. Поддержка ESP32-C6 в esp-radio: Wi-Fi ✓, BLE ✓, Coex ✓, ESP-NOW ✓, IEEE 802.15.4 ✓
   ([docs.rs/esp-radio/1.0.0-beta.1](https://docs.rs/esp-radio/1.0.0-beta.1/esp_radio/)).
5. esp-rtos с фичей `esp-radio` сам тянет `alloc` и `esp-radio-rtos-driver`
   ([esp-rtos/Cargo.toml](https://github.com/esp-rs/esp-hal/blob/esp-hal-v1.2.2/esp-rtos/Cargo.toml)).

## Выводы для проекта

1. Базу проекта брать из **esp-generate 1.4.0** (chip esp32c6, опции `wifi` — она сама включит
   `alloc` + `unstable-hal` + `embassy`; логирование `log`+esp-backtrace для espflash либо `defmt`
   для probe-rs). Инструмент привязан к актуальным версиям — не гнать шаблон «вперёд» вручную.
2. Рабочий стек: esp-hal 1.2.2 (`unstable` обязателен: требует esp-radio и включает шаблон embassy),
   esp-rtos 0.4.0 (`embassy` + `esp-radio`), esp-radio **1.0.0-beta.1** (не beta.2 — yanked),
   esp-alloc 0.11.0, esp-backtrace 0.20.0 / esp-println 0.18.0, esp-bootloader-esp-idf 0.6.0.
   Версии фиксировать как в шаблоне (`~1.2.2` / точные миноры) — beta-крейт не покрыт SemVer-гарантиями.
3. Тулчейн: stable + `rust-src` + target `riscv32imac-unknown-none-elf`; MSRV стека **1.95.0**; espup не нужен.
4. Каркас main: `#[esp_rtos::main] async fn main(spawner)`, `esp_hal::init`, heap_allocator,
   `esp_rtos::start(timg0.timer0, FROM_CPU_INTR0)` до инициализации esp-radio; задачи —
   `#[embassy_executor::task]` + `spawner`, фичи `arch-*` у embassy-executor не включать.
5. RMT (для гирлянды): в 1.2.2 основной API стабилен без `unstable`; `unstable` нужен только для
   `set_interrupt_handler` — и он всё равно уже включён из-за wifi/embassy.
6. Вся экосистема, кроме esp-hal, формально Unstable (таблица в книге) — закладывать время на
   миграции при обновлениях (у крейтов есть MIGRATING-гайды в монорепо).

## Открытые вопросы

1. **esp-printf** из ТЗ не существует на crates.io и в монорепо; видимо, имелся в виду esp-println 0.18.0.
   Уточнить у постановщика, не имелся ли в виду какой-то форк/переименование.
2. **«Отложенные задачи»**: выделенного API (deferred tasks) в esp-hal 1.2.2 / esp-rtos 0.4.0 не найдено
   (проверены procmacros, lib.rs esp-rtos, CHANGELOG). Вероятно, имелась в виду отложенная постановка
   задач через `#[embassy_executor::task]`+`Spawner` либо `CurrentThreadHandle::delay`. Уточнить термин.
3. Каталог `examples/src/bin/wifi/` в монорепо на теге esp-hal-v1.2.2 не найден (файл dhcp.rs = 404;
   полный список каталогов получить не удалось — лимит GitHub API) — канонический Wi-Fi-пример сейчас
   лучше смотреть в шаблоне esp-generate и в docs esp-radio; проверить, куда переехали примеры.
4. Причина yank esp-radio 1.0.0-beta.2 (через день после публикации) не выяснена — сверить с
   release notes перед выбором версии.
5. `embassy-net 0.9.1` + `smoltcp 0.14.0` из шаблона — версии новые; для продакшн-выпуска гирлянды
   стоит отдельно проверить changelog embassy-net на ломающие изменения.
6. `esp-radio` в статусе beta: возможны ломки API (Wi-Fi API уже сильно отличается от esp-wifi 0.x,
   встречающегося в старых статьях); при обновлении сверяться с MIGRATING-гайдами в монорепо esp-hal.
