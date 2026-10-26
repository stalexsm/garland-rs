# Исследование: OTA-обновление для no_std esp-hal на ESP32-C6

Дата: 2026-10-24. Метод: первоисточники — crates.io API, исходники crates.io / raw.githubusercontent.com (esp-rs/esp-hal, esp-rs/espflash), docs.rs, docs.espressif.com. Целевой стек проекта: esp-hal 1.2.x, no_std, ESP32-C6.

---

## 1. esp-bootloader-esp-idf: актуальная версия и API

### 1.1 Версия и статус

- Актуальная версия — **0.6.0** (crates.io, `max_version: 0.6.0`, публикация 2026-08-25; [crates.io/crates/esp-bootloader-esp-idf](https://crates.io/crates/esp-bootloader-esp-idf), CHANGELOG: `## [v0.6.0] - 2026-08-25`). MSRV 1.95.0 (README/Cargo.toml: `rust-version = "1.95.0"`).
- Крейт живёт в монорепо [esp-rs/esp-hal](https://github.com/esp-rs/esp-hal) (каталог `esp-bootloader-esp-idf/`), т.е. поддерживается той же командой и тем же CI, что и esp-hal (в `Cargo.toml` крейта: `repository = "https://github.com/esp-rs/esp-hal"`, чип-фичи `esp32c6` и др.).
- На `main` (на момент исследования) CHANGELOG `[Unreleased]` пуст, но код уже содержит нереleased'нутый рефакторинг: модуль `partitions.rs` разбит на `partitions/` (`entry.rs`, `table.rs`, `region.rs`, `types.rs`, …), в `ota.rs` появился `AutoFlashRegion` и стирание слота otadata перед записью (`write_slot` — erase 0x1000 + write). Ориентироваться в отчёте — на опубликованную 0.6.0.
- Официальная документация — [docs.espressif.com/projects/rust/esp-bootloader-esp-idf](https://docs.espressif.com/projects/rust/esp-bootloader-esp-idf/latest/); docs.rs-сборка свежих версий падает (на docs.rs у 0.6.0 висит `title="docs.rs failed to build esp-bootloader-esp-idf-0.6.0"` — [docs.rs/crate/esp-bootloader-esp-idf/0.6.0](https://docs.rs/crate/esp-bootloader-esp-idf/0.6.0)). Это норма для tier-2-целей; источник правды — исходники и docs.espressif.com.

### 1.2 Состав крейта (0.6.0)

Модули: `partitions`, `ota`, `ota_updater` (`lib.rs`: `pub mod ota; pub mod ota_updater; pub mod partitions;`), плюс `esp_app_desc!` — макрос заполнения ESP-IDF app descriptor (обязателен в образе приложения, его проверяет загрузчик; пример в doc-comment `lib.rs`).

`FlashStorage` — алиас: `pub type FlashStorage<'d> = esp_storage::FlashStorage<'d>;` (`src/flash/mod.rs`). Т.е. крейту нужен `esp-storage` 0.10, который создаётся из периферии: `esp_storage::FlashStorage::new(peripherals.FLASH)` (`esp-storage` 0.10, `src/common.rs:115`).

Конфигурация через esp-config (`esp_config.yml`): `mmu_page_size` (default `"64k"`, используется для app descriptor; C6 в списке поддерживаемых), `esp_idf_version`, `partition-table-offset` (default 0x8000), `secure_version` (default 0, «Used to enforce anti-rollback protection»).

### 1.3 `ota::Ota` — низкоуровневая работа с otadata

Источник: `src/ota.rs` (0.6.0).

- `OtaImageState` (`#[repr(u32)]`): `New = 0x0`, `PendingVerify = 0x1`, `Valid = 0x2`, `Invalid = 0x3`, `Aborted = 0x4`, `Undefined = 0xFFFFFFFF`. Док-комментарии: загрузчик переводит `New → PendingVerify` при включённом auto-rollback; прошивка ставит `Valid`, когда «found to be working», `Invalid` — когда нет.
- Ключевые методы:
  - `Ota::new(flash: FlashRegion<'a,'d>, ota_partition_count: usize) -> Result<Self, Error>` — регион должен быть `Data/Ota` размером ровно `0x2000`; счётчик слотов 1..=16.
  - `current_app_partition() -> Result<AppPartitionSubType, Error>` — по максимуму `ota_seq` в двух копиях записи (смещения `0x0000`/`0x1000`); если обе записи пусты — `Factory`.
  - `set_current_app_partition(app: AppPartitionSubType) -> Result<(), Error>` — классический алгоритм ESP-IDF: инкремент `ota_seq`, запись в «следующий» слот с CRC32 от `seq`. `Factory` сбрасывает otadata (две записи по 0xFF), `Test` запрещён (`Error::InvalidArgument`).
  - `set_current_ota_state(OtaImageState)` / `current_ota_state()` — состояние активного слота.
- Формат записи `OtaSelectEntry` (32 байта): `ota_seq: u32`, `seq_label: [u8; 20]`, `ota_state: OtaImageState`, `crc: u32` (CRC только от `ota_seq`); при чтении записи валидируются (CRC + допустимое состояние), иначе `Error::Invalid` — защита от мусора во флеше (фикс #5767 из CHANGELOG 0.6.0).

### 1.4 `ota_updater::OtaUpdater` — высокоуровневый API

Источник: `src/ota_updater.rs` (0.6.0).

- `OtaUpdater::new(flash: &'a mut FlashStorage<'d>, buffer: &'a mut [u8; PARTITION_TABLE_MAX_LEN]) -> Result<Self, Error>` — сам читает таблицу разделов; требует **≥2 OTA app-раздела** (не `factory`/`test`) и наличие `data,ota`, иначе `Error::Invalid`.
- `ota_data() -> Result<Ota>` — доступ к низкоуровневому `Ota`.
- `selected_partition() -> Result<AppPartitionSubType>`, `current_ota_state()`, `set_current_ota_state(state)`.
- `next_partition() -> Result<(FlashRegion<'_,'d>, AppPartitionSubType)>` — возвращает регион **неактивного** слота для записи (перескакивает раздел, с которого сейчас загрузились: см. `next_ota_part` — сверка с `booted_partition()`).
- `activate_next_partition()` — переключает otadata на следующий слот.
- `reset_data()` (новое в 0.6.0, #5767) — стирает otadata, выбирает `factory` (или `ota_0`, если factory нет).

### 1.5 `partitions` — что нужно знать

Источник: `src/partitions.rs` (0.6.0).

- `read_partition_table(flash: &mut FlashStorage<'d>, storage: &'a mut [u8]) -> Result<PartitionTable<'a>, Error>`; `PARTITION_TABLE_MAX_LEN = 0xC00` (буфер для таблицы — в примерах локальный массив на стеке). При включённом шифровании флеша чтение идёт через `flash_read_encrypted` (вызов `esp_storage::flash_encryption()`).
- `PartitionTable`: `iter()`, `find_partition(PartitionType)`, `booted_partition() -> Result<Option<PartitionEntry>>` (по MMU-маппингу, см. док-комментарий со ссылкой на `mmu_hal.c` ESP-IDF).
- `PartitionEntry`: `offset()`, `len()`, `label_as_str()`, `partition_type()`, `sha256(flash)` (новое в 0.6.0, #6061 — проверка целостности записанного образа), `as_flash_region(flash) -> FlashRegion`.
- `FlashRegion`: `read`/`write`/`erase(from, to)`/`capacity`; `as_nor_flash()` / `as_nor_flash_encrypted()` (0.6.0, #5857) — шифрование применяется прозрачно при записи региона, если включено шифрование флеша.
- `AppPartitionSubType`: `Factory = 0`, `Ota0..Ota15` (с `OTA_SUBTYPE_OFFSET`), `Test` — последний. `DataPartitionSubType::Ota = 0` — «partition should be 0x2000 bytes in size».

## 2. Таблица разделов и прошивка загрузчика

### 2.1 Минимальный CSV для OTA

Официальный пример [examples/ota/update/partitions.csv](https://github.com/esp-rs/esp-hal/blob/main/examples/ota/update/partitions.csv) (esp-hal main):

```csv
# Name,   Type, SubType, Offset,   Size,     Flags
nvs,      data, nvs,     0x9000,   0x4000,
otadata,  data, ota,     0xd000,   0x2000,
phy_init, data, phy,     0xf000,   0x1000,
factory,  app,  factory, 0x10000,  0x100000,
ota_0,    app,  ota_0,   0x110000, 0x100000,
ota_1,    app,  ota_1,   0x210000, 0x100000,
```

Требования из первоисточников:

- Минимум **два** app-слота `ota_X` и **один** `data,ota` — doc-comment `ota.rs` («requires … at least two OTA app slot partitions (i.e., ota_0 and ota_1) and an OTA Data Partition») и ESP-IDF OTA docs.
- `otadata` — ровно **0x2000** (два флеш-сектора по 0x1000 — две копии записи для отказоустойчивости; ESP-IDF: «The OTA data partition is two flash sectors (0x2000 bytes) in size»; то же требование в doc-comment `DataPartitionSubType::Ota` и в коде `Ota::new` — проверка `capacity() != 0x2000`).
- `OtaUpdater::new` требует ≥2 OTA-слотов (`if ota_count < 2 { return Err(Error::Invalid) }`) — т.е. минимальный набор: `nvs` + `otadata` + `ota_0` + `ota_1`; `factory` не обязателен (вариант на 4 МБ без factory и с меньшими слотами приводит, например, [faern/esp-ota README](https://github.com/faern/esp-ota#partition-table): `ota_0 0x10000 0x180000, ota_1 0x190000 0x180000`).

### 2.2 Подключение в проект на esp-hal

Из `.cargo/config.toml` официального примера — таблица подключается флагом espflash в runner:

```toml
[target.'cfg(target_arch = "riscv32")']
runner = "espflash flash --monitor --partition-table=examples/ota/update/partitions.csv"
```

В `Cargo.toml` приложения: `esp-bootloader-esp-idf = { version = "0.6", features = ["esp32c6"] }` (чип-фича обязательна, `Cargo.toml.orig`: «One of the following features must be enabled to select the target chip»), и `esp-storage` (для `FlashStorage`). В коде — `esp_bootloader_esp_idf::esp_app_desc!();` (см. примеры в doc-comment `lib.rs`).

Адрес таблицы разделов крейт берёт из esp-config `partition-table-offset` (default 0x8000 = 32768, `esp_config.yml`) — должен совпадать с тем, куда её прошил espflash.

### 2.3 Прошивка загрузчика: нужен ли отдельный шаг?

**Отдельный шаг не нужен**, если устраивает встроенный в espflash загрузчик:

- espflash содержит прекомпилированные ESP-IDF bootloaders на каждый чип: `const BOOTLOADER_ESP32C6: &[u8] = include_bytes!("../../resources/bootloaders/esp32c6-bootloader.bin")` ([espflash/src/image_format/idf.rs](https://github.com/esp-rs/espflash/blob/main/espflash/src/image_format/idf.rs)). Манифест сборки ([resources/bootloaders/manifest.yaml](https://github.com/esp-rs/espflash/blob/main/espflash/resources/bootloaders/manifest.yaml)): C6-загрузчик собран из **ESP-IDF `release/v6.1`** с sdkconfig `CONFIG_ESPTOOLPY_FLASHSIZE_64MB` (+64k MMU page у esp-hal — пояснение в [resources/README.md](https://github.com/esp-rs/espflash/blob/main/espflash/resources/README.md)).
- Команда `espflash flash` в один заход пишет bootloader (для C6 — по адресу 0x0, `chip.boot_address()`), таблицу разделов и приложение (`IdfBootloaderFormat::flash_segments()` возвращает ровно эти три сегмента — [idf.rs](https://github.com/esp-rs/espflash/blob/main/espflash/src/image_format/idf.rs)).
- **Свой загрузчик нужен** для auto-rollback: док-комментарий `ota.rs` — «The prebuilt bootloaders provided by `espflash` _might not_ include OTA support. In that case you need to build the bootloader yourself»; комментарий примера однозначен: «Mark the current slot as VALID — this is only needed if the bootloader was built with auto-rollback support. **The default pre-compiled bootloader in espflash is NOT** [built with auto-rollback support]». Т.е. переключение слотов по otadata prebuilt-загрузчик делает (это штатное поведение ESP-IDF bootloader'а), а откат при зависании нового образа — нет. Для rollback: собрать ESP-IDF bootloader с `CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE=y` ([ESP-IDF Kconfig](https://docs.espressif.com/projects/esp-idf/en/latest/esp32/api-reference/kconfig.html#config-bootloader-app-rollback-enable)) и передать его в espflash: CLI-флаг `--bootloader <FILE>` ([cli/mod.rs: `pub bootloader: Option<PathBuf>`](https://github.com/esp-rs/espflash/blob/main/espflash/src/cli/mod.rs)) или `espflash.toml`: `[idf] bootloader = "path/to/custom/bootloader.bin"` ([espflash README](https://github.com/esp-rs/espflash/blob/main/espflash/README.md)).
- Семантика rollback — по ESP-IDF docs ([ota.html](https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/ota.html#app-rollback)): при включённом rollback новое приложение стартует в `PENDING_VERIFY`; если прошивка не вызвала «mark valid» до перезагрузки — загрузчик помечает слот `INVALID` и грузит предыдущий. Состояние хранится не в образе, а в otadata.

## 3. Официальный пример `examples/ota/update`

Источник: [examples/ota/update/src/main.rs](https://github.com/esp-rs/esp-hal/blob/main/examples/ota/update/src/main.rs) (esp-hal main).

Что показывает:

1. Образ «новой прошивки» **вшит в бинарь**: `static OTA_IMAGE: &[u8] = include_bytes!("../../../../target/ota_image");`, а сам файл получается командой `espflash save-image --chip={CHIP} target/{TARGET}/release/<app> target/ota_image` (инструкция в doc-comment примера).
2. Последовательность работы: `read_partition_table` → `pt.booted_partition()` → `OtaUpdater::new` → если otadata невалиден, `reset_data()`; если текущее состояние `New|PendingVerify` — поставить `Valid` (только для rollback-загрузчика); по кнопке: `next_partition()` → стирание и запись образа **по секторам 4096 байт**, последний чанк добивается 0xFF до кратности 16 байт (требование шифрованной записи) → `activate_next_partition()` → `set_current_ota_state(New)`; RESET — загрузка нового слота.

Как адаптировать под HTTP: док-комментарий примера прямо говорит — «In a real world application you can get the image via HTTP(S), UART or from an sd-card etc.» Т.е. вместо `include_bytes!` пишем в `FlashRegion` от `next_partition()` чанки по мере получения из HTTP-ответа (erase сектора перед записью сохраняем; erase крупными блоками — можно стереть весь слот заранее). Сам OTA-слой транспорт-агностичен — HTTP-часть (Wi-Fi через esp-radio, TLS, сервер обновлений) в пример не входит и является отдельной задачей проекта.

## 4. Судьба сторонних OTA-крейтов

| Крейт | Последний релиз | Статус | Подтверждение |
|---|---|---|---|
| `esp-hal-ota` (filipton) | **0.7.2, 2026-10-17** (0.7.0/0.7.1 — 2026-10-16) | **Не заброшен** — активная разработка в октябре 2026 | [crates.io](https://crates.io/crates/esp-hal-ota) (версии 0.7.0–0.7.2 от 16–17.10.2026); коммиты: «feat!: cleaner api with mandatory verify», «update to esp-hal 1.2.2», «feat: esp32c5, esp32c61 and esp32s31 support» ([github.com/filipton/esp-hal-ota](https://github.com/filipton/esp-hal-ota)) |
| `esp-ota` (faern) | 0.2.2, 2025-01-31 (до этого 0.2.0 — 2023-07) | Заброшен-подобный темп, **но важнее другое: это не no_std** — зависимость `esp-idf-sys 0.36` + `embuild` | [crates.io esp-ota 0.2.2 dependencies](https://crates.io/crates/esp-ota/0.2.2/dependencies); [Cargo.toml](https://github.com/faern/esp-ota/blob/main/Cargo.toml) |
| `esp-ota-nostd` | 0.1.0, 2024-10-06, единственный релиз за всё время, 1210 загрузов | Фактически спящий эксперимент; в crates.io **нет ссылки на репозиторий** (`repo: None`) | [crates.io](https://crates.io/crates/esp-ota-nostd); исходники 0.1.0 (static.crates.io): жёстко зашиты ровно 2 слота (`(new_seq - 1) % 2`), async API (`embedded-io-async`), зависимость `esp-partition-table 0.1` |

Уточнения по `esp-hal-ota` (важно для C6):

- Претензия «docs.rs build fail» — **подтверждена**: все свежие версии 0.5.7–0.7.2 не собираются на docs.rs (`title="docs.rs failed to build esp-hal-ota-0.7.2"`, [docs.rs/crate/esp-hal-ota/0.7.2](https://docs.rs/crate/esp-hal-ota/0.7.2)); для сравнения, esp-hal 1.2.2 на docs.rs собирается. Документацию брать из исходников.
- Претензия «автор не тестирует C6» — **подтверждена дословно**, README, раздел Limitations: «I cannot test if it works properly on esp32s2, esp32c2, esp32c5, **esp32c6**, esp32c61, esp32h2 and esp32s31. But esp32s3, esp32c3 and esp32 are working perfectly fine» ([README.md](https://github.com/filipton/esp-hal-ota/blob/master/README.md)).
- Зависит от PAC-крейтов (`esp32c6 0.24`) и `embedded-storage 0.3.2`; требует opt-in фичу `embedded-storage` у `esp-storage` 0.10 (README «Getting started»).
- Вне esp-rs: репозиторий отдельный (`filipton/esp-hal-ota`, 33 stars), не входит в экосистемный CI esp-rs.

**Вывод:** писать свой OTA-слой не нужно — протокол otadata (две копии, seq+CRC, состояния) уже реализован в официальном `esp-bootloader-esp-idf`, который сопровождается в монорепо esp-hal с CI по всем чипам, включая C6. Сторонние крейты не дают преимуществ: `esp-ota` вообще для std/ESP-IDF, `esp-ota-nostd` — замороженный эксперимент, `esp-hal-ota` — живой, но C6 автором не тестируется и docs.rs-доки не собираются.

## 5. Как получить .bin для OTA-слота

- Сборка даёт ELF (`target/riscv32imac-unknown-none-elf/release/<app>`); образ приложения — командой **`espflash save-image`**: `espflash save-image --chip=esp32c6 <ELF> <out.bin>` ([espflash README, подкоманда save-image](https://github.com/esp-rs/espflash/blob/main/espflash/README.md)). Именно так готовит образ официальный пример (см. §3). Формат — стандартный ESP-IDF app image (заголовок 0xE9, сегменты, app descriptor); в CLI формат по умолчанию `--format esp-idf` ([bin/espflash.rs: `#[arg(long, default_value = "esp-idf")] format`)](https://github.com/esp-rs/espflash/blob/main/espflash/src/bin/espflash.rs).
- Параметры `save-image` ([cli/mod.rs `SaveImageArgs`/`ImageArgs`](https://github.com/esp-rs/espflash/blob/main/espflash/src/cli/mod.rs)): `--chip`, `--merge` (склеить bootloader+таблица+app в один образ; для OTA-слота **не нужно** — в слот кладётся именно app image, `IdfBootloaderFormat::ota_segments()`), `--mmu-page-size`, `--min-chip-rev`; espflash по умолчанию проверяет наличие и корректность app descriptor в образе (`check_app_descriptor`, default true).
- Альтернатива — `esptool.py elf2image` ([faern/esp-ota README](https://github.com/faern/esp-ota#app-image-format)).
- В образе обязательно должен быть app descriptor — т.е. в коде вызывается `esp_bootloader_esp_idf::esp_app_desc!()`; без него save-image отклонит образ.

---

## Выводы для проекта

1. **Библиотека:** официальный `esp-bootloader-esp-idf` 0.6.0 (+ `esp-storage` 0.10), feature `esp32c6`. Свой OTA-слой не писать; `esp-hal-ota`/`esp-ota`/`esp-ota-nostd` не использовать (см. §4).
2. **Таблица разделов:** минимально `nvs` + `otadata` (строго 0x2000) + `ota_0` + `ota_1`; `factory` опционален. Подключение — `--partition-table=partitions.csv` в runner espflash. Размеры слотов подобрать под свой образ с запасом (пример esp-hal: 0x100000 на слот; на 4 МБ-flash возможен layout как у esp-ota: слоты по 0x180000 без factory).
3. **Загрузчик:** отдельный шаг прошивки bootloader не нужен — `espflash flash` пишет встроенный ESP-IDF-загрузчик (release/v6.1), таблицу и приложение одной командой. Свой bootloader (`--bootloader`) нужен только если требуется auto-rollback (`CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE`), который в prebuilt-загрузчике espflash выключен.
4. **Логика приложения:** `FlashStorage::new(peripherals.FLASH)` → `OtaUpdater::new(&mut flash, &mut [u8; 0xC00])` → `next_partition()` → стереть/записать образ чанками (сектор ≤4096, последний чанк — паддинг до 16 байт) → `activate_next_partition()` → `set_current_ota_state(New)` → перезагрузка; после первой загрузки нового слота — `set_current_ota_state(Valid)`. `PartitionEntry::sha256()` доступен для контроля целостности записанного образа.
5. **Артефакт для слота:** `cargo build --release` + `espflash save-image --chip=esp32c6` (без `--merge`); в коде обязателен `esp_app_desc!`.
6. **HTTP-скачивание** — самостоятельная задача проекта (esp-radio/Wi-Fi на C6); OTA-слой от транспорта не зависит.

## Открытые вопросы

1. **Rollback-стратегия.** Нужен ли проекту auto-rollback? Если да — потребуется собирать свой ESP-IDF-bootloader (idf.py, `CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE=y`) и держать его в репо/CI; также решить, где живет «подтверждение валидности» (после какой самопроверки ставить `Valid`). Без этого prebuilt-загрузчик + `New`-состояние дают «прыжок» на новый слот без отката.
2. **Расхождение main ↔ 0.6.0.** Официальный пример компилируется против main: использует `esp_hal::flash::Flash` (в выпущенном esp-hal 1.2.2 модуля `flash` нет — проверено по локальному кэшу крейта) и передаёт его прямо в `read_partition_table`. В опубликованной 0.6.0 + esp-storage 0.10 рабочий путь — `esp_storage::FlashStorage::new(peripherals.FLASH)`. При пинировании версий (1.2.2/0.6.0) код примера придётся адаптировать; проверить компиляцию.
3. **MMU page size.** `mmu_page_size` esp-config (default `64k` для C6) попадает в app descriptor; espflash собирает C6-bootloader с flashsize 64MB именно под 64k-страницы. При собственном bootloader убедиться в согласованности (и с `--mmu-page-size` у save-image, если задавать вручную).
4. **docs.rs недоступен** и у `esp-bootloader-esp-idf`, и у `esp-hal-ota` — документацию читать в исходниках / docs.espressif.com; при апгрейдах сверять CHANGELOG.
5. **Шифрование флеша и secure boot.** Если на устройстве включится flash encryption — писать только через `FlashRegion` (прозрачное шифрование, кратность 16 байт). Подпись/валидация скачанного образа (secure boot, signed images) исследованием не покрыта — отдельная задача, если требуется.
6. **Update-сервер и транспорт** (HTTP/TLS, версия/размер/CRC образа, откат при обрыве связи — в 0.6.0 `activate_next_partition` вызывается после полной записи, дозаписи нет; у `esp-hal-ota` 0.7 есть resume) — выбрать требования до реализации.
