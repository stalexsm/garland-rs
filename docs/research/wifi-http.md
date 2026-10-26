# Исследование: Wi-Fi (esp-radio beta) + HTTP-сервер (embassy-net) на ESP32-C6

Дата: октябрь 2026. Область: Wi-Fi STA + локальный HTTP-сервер для заливки прошивки (OTA) через браузер.
Метод: первоисточники — API crates.io, архивы `.crate` (Cargo.toml.orig), исходники GitHub по тегам релизов.
Проверка версий выполнена запросами к `https://crates.io/api/v1/crates/...` (даты ниже — из ответов API).

---

## 1. esp-radio (beta)

### Версии

| Крейт | Актуально | Дата релиза | Примечание |
|---|---|---|---|
| `esp-radio` | **1.0.0-beta.1** | 2026-09-16 | текущая beta; `1.0.0-beta.2` от 2026-09-17 **yanked** ([versions API](https://crates.io/api/v1/crates/esp-radio/versions)) |
| `esp-radio` (стабильная ветка) | 0.18.0 | 2026-04-16 | не-beta альтернатива ([crates.io](https://crates.io/crates/esp-radio)) |
| `esp-hal` | 1.2.2 (т. 1.2.x) | — | esp-radio 1.0.0-beta.1 требует `esp-hal = "~1.2.0"` ([Cargo.toml.orig из архива 1.0.0-beta.1](https://static.crates.io/crates/esp-radio/esp-radio-1.0.0-beta.1.crate)) |

Пара «esp-hal 1.2.2 + esp-radio 1.0.0-beta.1» — согласованная. `esp-radio` 1.0.0-beta.1 требует `rust-version = 1.95.0`, edition 2024 — свежий тулчейн обязателен ([Cargo.toml.orig](https://static.crates.io/crates/esp-radio/esp-radio-1.0.0-beta.1.crate)).

### Feature flags (для ESP32-C6)

Полный список фич версии 1.0.0-beta.1 ([features API](https://crates.io/api/v1/crates/esp-radio/1.0.0-beta.1) и [Cargo.toml.orig](https://static.crates.io/crates/esp-radio/esp-radio-1.0.0-beta.1.crate)):

- `esp32c6` — выбор чипа (обязательная).
- **`wifi` — стабильная фича**: `wifi = ["dep:enumset", "dep:embassy-net-driver-02", "dep:xarxa-driver"]`. В Cargo.toml.orig прямо написано: *«`wifi` is the only radio feature that does not pull in `unstable`»* — в beta-серии Wi-Fi больше не требует `unstable`.
- `default = ["esp-alloc"]` — esp-radio по умолчанию использует esp-alloc как глобальный аллокатор (буферы Wi-Fi берутся из него).
- Остальное радио — нестабильно (тянет `unstable`): `ble`, `wifi-eap`, `coex`, `esp-now`, `sniffer`, `csi`, `ieee802154`. Для проекта не нужно.

Важно: Wi-Fi-драйвер реализует `embassy-net-driver 0.2` → напрямую совместим с `embassy-net` 0.9. Параллельно появился `xarxa-driver 0.1.0` (embassy-rs, новый стек, создан 2026-07-27, [crates.io](https://crates.io/api/v1/crates/xarxa-driver)) — на будущее наблюдение, сейчас не требуется.

### Минимальный код подключения (STA)

Официальный пример: [`examples/wifi/embassy_dhcp/src/main.rs` @ esp-hal-v1.2.2](https://github.com/esp-rs/esp-hal/blob/esp-hal-v1.2.2/examples/wifi/embassy_dhcp/src/main.rs) (199 строк; на C6 фильтруется `//% CHIP_FILTER: wifi_driver_supported`). Схема:

```rust
// 1. Аллокатор (см. «Heap» ниже)
esp_alloc::heap_allocator!(#[ram(reclaimed)] size: 64 * 1024);
esp_alloc::heap_allocator!(size: 36 * 1024);

// 2. RTOS-слой (обязателен для esp-radio), [Cargo.toml примера](.../Cargo.toml):
//    esp-rtos с features = ["esp-radio", "embassy"], embassy-executor = "0.10"
esp_rtos::start(timg0.timer0, peripherals.FROM_CPU_INTR0);

// 3. Конфиг станции и контроллер
let station_config = Config::Station(
    StationConfig::default()
        .with_ssid(SSID.try_into().unwrap())
        .with_authentication(AuthenticationMethodConfig::Wpa2Personal(PASSWORD.try_into().unwrap())),
);
let wifi_interface = esp_radio::wifi::Interface::station();
let mut controller = esp_radio::wifi::WifiController::new(
    peripherals.WIFI,
    ControllerConfig::default().with_initial_config(station_config),
).unwrap();

// 4. embassy-net поверх драйвера
let (stack, runner) = embassy_net::new(
    wifi_interface,
    embassy_net::Config::dhcpv4(Default::default()),
    mk_static!(StackResources<3>, StackResources::<3>::new()),
    seed_from_rng, // esp_hal::rng::Rng
);
spawner.spawn(net_task(runner)).unwrap();
spawner.spawn(connection(controller)).unwrap();
stack.wait_config_up().await; // получили IP по DHCP
```

Задача подключения из того же примера: `controller.connect_async().await`; при ошибке — повтор через 5 с; после успеха — `controller.wait_for_disconnect_async().await` и переподключение. Сканирование: `controller.scan_async(&ScanConfig::default().with_max(10)).await`.

### Heap, стек, таймауты

- **Heap**: пример выделяет суммарно **100 КБ** — `#[ram(reclaimed)] 64 * 1024` (из reclaimed RAM) + `36 * 1024` ([embassy_dhcp/main.rs](https://github.com/esp-rs/esp-hal/blob/esp-hal-v1.2.2/examples/wifi/embassy_dhcp/src/main.rs)). Буферы Wi-Fi идут из esp-alloc-heap (`default = ["esp-alloc"]`, [Cargo.toml.orig](https://static.crates.io/crates/esp-radio/esp-radio-1.0.0-beta.1.crate)). Формального минимума в проверенных источниках не нашёл — берём конфигурацию примера как рабочую baseline (см. «Открытые вопросы»).
- **RTOS/стек задач**: обязателен `esp_rtos::start(...)` до создания `WifiController` (пример, ссылка выше); `esp-rtos` с features `["esp-radio", "embassy"]` ([Cargo.toml примера](https://github.com/esp-rs/esp-hal/blob/esp-hal-v1.2.2/examples/wifi/embassy_dhcp/Cargo.toml)).
- **Таймауты**: у подключения в примере встроенного таймаута нет — только retry-цикл 5 с. TCP-таймауты настраиваются на сокете: `embassy_net::tcp::TcpSocket::set_timeout(Option<Duration>)` ([embassy-net-v0.9.1/src/tcp.rs, стр. 352](https://github.com/embassy-rs/embassy/blob/embassy-net-v0.9.1/embassy-net/src/tcp.rs)).

## 2. embassy-net и TCP-сервер

- Актуальная версия **0.9.1**; зависимости: `embedded-io-async ^0.7.0`, `smoltcp ^0.13.0`, `heapless ^0.9`, `embassy-net-driver ^0.2.0` ([deps API](https://crates.io/api/v1/crates/embassy-net/0.9.1/dependencies)). Т.е. связка esp-radio 1.0.0-beta.1 ↔ embassy-net 0.9 ↔ embedded-io-async 0.7 — бесшовная.
- **TCP-сервер есть и минимален**: `TcpSocket::new(stack, rx/tx buffers)` → `socket.accept(local_endpoint)` (async-принятие соединения на порт) → работа через `TcpReader`/`TcpWriter` (`socket.split()`), реализуют `embedded-io-async` (`read`/`write`/`flush`) ([tcp.rs: `accept` стр. 275, `split` стр. 238](https://github.com/embassy-rs/embassy/blob/embassy-net-v0.9.1/embassy-net/src/tcp.rs)).
- **HTTP-сервера в примерах esp-hal 1.x НЕТ**: в каталогах `examples/wifi` тегов v1.0.0, v1.1.2, v1.2.0, v1.2.2 присутствуют только `80211_tx`, `embassy_access_point*`, `embassy_coex`, `embassy_dhcp`, `embassy_sntp`, `sniffer` (в v1.0.0 ещё `static_ip`, `dhcp`, `access_point`) — проверено содержимым [examples/wifi @ esp-hal-v1.2.2](https://github.com/esp-rs/esp-hal/tree/esp-hal-v1.2.2/examples/wifi) и аналогичных каталогов остальных тегов. Исторический пример `http_server` в текущих тегах отсутствует; ближайший по смыслу пример — `embassy_dhcp`, но он показывает только **клиент** (HTTP GET через `reqwless` 0.14).
- **DHCP hostname**: в embassy-net 0.9 есть фича `dhcpv4-hostname`; `embassy_net::Config::hostname: Option<heapless::String<MAX_HOSTNAME_LEN>>` — «Our hostname. This will be sent to the DHCP server as Option 12» ([embassy-net-v0.9.1/src/lib.rs, стр. 71–156](https://github.com/embassy-rs/embassy/blob/embassy-net-v0.9.1/embassy-net/src/lib.rs)). Устройство появится в роутере под заданным именем. (Значение `MAX_HOSTNAME_LEN` не проверял — см. «Открытые вопросы».)

## 3. HTTP-стек для одного endpoint'а заливки файла

### picoserve

- **Жив и совместим**: `picoserve 0.20.1`, обновлён 2026-09-21; зависимости включают `embassy-net ^0.9.1` (optional), `embedded-io-async ^0.7.0`, `embassy-time ^0.5.1`, плюс `serde`, `futures-util`, `heapless` ([deps API](https://crates.io/api/v1/crates/picoserve/0.20.1/dependencies), [crates.io](https://crates.io/crates/picoserve)). Прямое совпадение версий с embassy-net 0.9 / embedded-io-async 0.7.
- Минусы для нашего случая: тянет serde/futures-util (размер бинарника), а нужен один POST-endpoint со **стримингом тела прямо в флеш-слот** — из picoserve потребуется написать handler-экстрактор поверх тела запроса всё равно.

### Свой мини-парсер (рекомендация)

Задача: `GET /` → HTML-страница с `<input type=file>` + `fetch('/ota', {method:'POST', body: file})`; `POST /ota` → стрим тела в свободный OTA-слот. При такой реализации браузера:

- **Content-Length** всегда известен (fetch с `body: File` шлёт точный Content-Length, не chunked) → чанковую кодировку можно не реализовывать и отвечать `411 Length Required` на её отсутствие.
- **Multipart не нужен** — multipart/form-data появляется только у `<form>`-сабмита; через `fetch` тело приходит «как есть».

Оценка объёма (строки, нет зависимости):

| Часть | Оценка |
|---|---|
| Чтение до `\r\n\r\n` (буфер 1–2 КБ, таймаут) | 40–60 |
| Разбор request-line + заголовков (Path, Content-Length, Connection) | 50–80 |
| Маршрутизация на 2 маршрута | 20 |
| Стриминг тела в OTA-слот + лимит размера + прогресс | 80–120 |
| HTML-страница (инлайн, с прогресс-баром) | 60–80 |
| **Итого** | **≈ 250–350 строк** |

Вывод: свой мини-парсер проще и легче; picoserve — запасной вариант «не писать своё», если требования к endpoint'ам вырастут.

## 4. mDNS (`garland.local`) и DHCP hostname

- **В esp-radio mDNS нет**: в списке фич 1.0.0-beta.1 отсутствует какая-либо `mdns` ([features API](https://crates.io/api/v1/crates/esp-radio/1.0.0-beta.1)).
- **Готового no_std-респондера тоже нет** (проверка crates.io, октябрь 2026): `mdns-sd 0.21.6`, `simple-mdns 0.7.0` (зависит от `socket2` → std, [deps API](https://crates.io/api/v1/crates/simple-mdns/0.7.0/dependencies)), `libmdns 0.10.1` (tokio/std) — все рассчитаны на ОС-сокеты и на ESP32 не работают.
- Варианты для `garland.local`: (а) свой минимальный mDNS-респондер на UDP-сокете embassy-net (ответ A-запросом; ~200–400 строк); (б) отказаться от mDNS: печатать IP на дисплее/в лог, задать **DHCP hostname** через embassy-net `dhcpv4-hostname` (см. §2) — имя будет видно в роутере. Для MVP достаточно (б); mDNS — отдельная небольшая задача.

## 5. OTA поверх этой связки

- **Официальный путь esp-rs**: `esp-bootloader-esp-idf 0.6.0` (активен, обновлён 2026-08-26, [crates.io](https://crates.io/crates/esp-bootloader-esp-idf)) + `esp-storage` (`FlashStorage`). Официальный пример OTA: [`examples/ota/update` @ esp-hal-v1.2.2](https://github.com/esp-rs/esp-hal/blob/esp-hal-v1.2.2/examples/ota/update/src/main.rs) — `read_partition_table()`, `OtaUpdater::new(&mut flash, &mut buffer)`, `ota.next_partition()`, запись образа **чанками по 4096 байт**, управление `OtaImageState` (New/PendingVerify/Valid) для auto-rollback. `esp-ota 0.2.2` — устаревший (обновлён 2025-01-31, [crates.io](https://crates.io/crates/esp-ota)); брать `esp-bootloader-esp-idf`.
- **Разметка флеша**: [partitions.csv примера](https://github.com/esp-rs/esp-hal/blob/esp-hal-v1.2.2/examples/ota/update/partitions.csv): `factory 0x100000 (1MB)` + `ota_0 1MB` + `ota_1 1MB` + служебные → верх таблицы `0x310000 ≈ 3,1 MB`. На 4 MB флеш помещается при слотах по 1 MB; на 8 MB — комфортный запас (слоты можно увеличить). Размер нашего `.bin` пока неизвестен (замерить после первой сборки) — слоты 1 MB достаточны, если образ ≤ 1 MB.
- **HTTP vs HTTPS**: для локальной сети OTA по **HTTP** достаточно и не добавляет зависимостей. HTTPS потребовал бы TLS-стек: `esp-mbedtls` на crates.io **не опубликован** (проверено: «crate `esp-mbedtls` does not exist», API-ответ от 2026-10) — только git-зависимость, плюс заметный расход heap. Для страницы заливки с ноутбука это лишнее.
- **Скорость заливки**: подтверждённых цифр TCP-throughput для esp-radio на C6 в проверенных источниках нет. Порядок величины для ~1 MB образа: при 0,1–2 MB/s это от ~1 до ~10 с. Замерить на железе (см. «Открытые вопросы»).

---

## Выводы для проекта

1. **Стек**: `esp-hal 1.2.2` + `esp-radio 1.0.0-beta.1` (фичи: `esp32c6`, `wifi` — стабильна, `default`/`esp-alloc`) + `embassy-net 0.9` (features `dhcpv4`, `medium-ethernet`, `tcp`) + `embedded-io-async 0.7` + `embassy-executor 0.10` + `esp-rtos` (`esp-radio`, `embassy`). Toolchain Rust ≥ 1.95.
2. **Heap**: стартовать с конфигурации примера — 64 КБ (reclaimed RAM) + 36 КБ; при нехватке — увеличить обычный heap.
3. **Подключение STA**: по образцу `embassy_dhcp` — задача-коннектор с retry (5 с) и `wait_for_disconnect_async`; IP по DHCP; добавить `set_timeout` на TCP-сокете сервера.
4. **HTTP-сервер**: свой мини-сервер на `TcpSocket::accept` (~250–350 строк): `GET /` → страница, `POST /ota` → стрим тела в OTA-слот по Content-Length; чанки/multipart не поддерживать (отказ 411/400). picoserve 0.20.1 совместим и годится как запасной путь.
5. **OTA**: `esp-bootloader-esp-idf 0.6.0` + `esp-storage`, таблица разделов как в примере (factory + ota_0 + ota_1 по 1 MB) — влезает в 4 MB, на 8 MB есть запас. По HTTP, не HTTPS.
6. **Имя устройства**: DHCP hostname через `dhcpv4-hostname`; `garland.local` без mDNS не заработает — mDNS (свой респондер или отказ) вынести в отдельную задачу.

## Открытые вопросы

1. Минимально достаточный размер esp-alloc-heap для Wi-Fi + OTA-сервера на C6 — 100 КБ из примера как baseline; уточнить экспериментом (и что даёт `alloc-hooks` из примера для профилирования аллокаций).
2. Реальная скорость приёма по TCP (OTA-заливка) на C6/esp-radio — нет опубликованных цифр; замерить на железе.
3. Реальный размер релизного `.bin` гирлянды (влезет ли в слот 1 MB; если нет — пересчёт таблицы под 8 MB).
4. Значение `MAX_HOSTNAME_LEN` и поведение `dhcpv4-hostname` (фича не включена в примерах esp-hal — протестировать).
5. `xarxa-driver` (новый стек embassy-rs, уже в зависимостях esp-radio beta) — следить: возможен будущий уход с embassy-net/smoltcp.
6. Есть ли смысл держать fallback на стабильный `esp-radio 0.18.0`, если beta.1 окажется проблемной (там иная история стабильности Wi-Fi — требует отдельной проверки).
7. Нужен ли mDNS-респондер в MVP или достаточно hostname в роутере + отображение IP.

## Источники

- crates.io API: [esp-radio/versions](https://crates.io/api/v1/crates/esp-radio/versions), [esp-radio/1.0.0-beta.1 features](https://crates.io/api/v1/crates/esp-radio/1.0.0-beta.1), [архив esp-radio-1.0.0-beta.1.crate (Cargo.toml.orig)](https://static.crates.io/crates/esp-radio/esp-radio-1.0.0-beta.1.crate), [esp-hal](https://crates.io/api/v1/crates/esp-hal), [embassy-net deps 0.9.1](https://crates.io/api/v1/crates/embassy-net/0.9.1/dependencies), [picoserve deps 0.20.1](https://crates.io/api/v1/crates/picoserve/0.20.1/dependencies), [xarxa-driver](https://crates.io/api/v1/crates/xarxa-driver), [simple-mdns deps 0.7.0](https://crates.io/api/v1/crates/simple-mdns/0.7.0/dependencies), [esp-bootloader-esp-idf](https://crates.io/api/v1/crates/esp-bootloader-esp-idf), [esp-ota](https://crates.io/api/v1/crates/esp-ota)
- esp-hal @ `esp-hal-v1.2.2`: [examples/wifi/embassy_dhcp/src/main.rs](https://github.com/esp-rs/esp-hal/blob/esp-hal-v1.2.2/examples/wifi/embassy_dhcp/src/main.rs), [его Cargo.toml](https://github.com/esp-rs/esp-hal/blob/esp-hal-v1.2.2/examples/wifi/embassy_dhcp/Cargo.toml), [examples/wifi (список)](https://github.com/esp-rs/esp-hal/tree/esp-hal-v1.2.2/examples/wifi), [examples/ota/update/src/main.rs](https://github.com/esp-rs/esp-hal/blob/esp-hal-v1.2.2/examples/ota/update/src/main.rs), [examples/ota/update/partitions.csv](https://github.com/esp-rs/esp-hal/blob/esp-hal-v1.2.2/examples/ota/update/partitions.csv)
- embassy-net @ `embassy-net-v0.9.1`: [src/tcp.rs (accept/split/set_timeout)](https://github.com/embassy-rs/embassy/blob/embassy-net-v0.9.1/embassy-net/src/tcp.rs), [src/lib.rs (dhcpv4-hostname)](https://github.com/embassy-rs/embassy/blob/embassy-net-v0.9.1/embassy-net/src/lib.rs)
