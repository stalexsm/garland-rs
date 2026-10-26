# Исследование: драйвер адресной ленты WS2812/SK6812 на esp-hal 1.2 (ESP32-C6)

Дата: 2026-10-26. Контекст проекта: ESP32-C6, esp-hal 1.2 (в крейт-реестре локально 1.2.2), Embassy/async, лента WS2812B; возможный будущий апгрейд на SK6812 (RGBW).

Все утверждения снабжены ссылками на первоисточники; источники проверены на дату отчёта.

---

## 1. esp-hal-smartled 0.18.0 (esp-rs/esp-hal-community)

### 1.1. Совместимость с esp-hal 1.2 — подтверждена

- Версия 0.18.0 опубликована на crates.io 2026-09-05 — [crates.io/crates/esp-hal-smartled](https://crates.io/crates/esp-hal-smartled).
- Зависимость в манифесте: `esp-hal = { version = "~1.2", default-features = false, features = ["requires-unstable"] }`, т.е. `>=1.2, <1.3` — semver-совместимость с esp-hal 1.2.x — [Cargo.toml на теге esp-hal-smartled-v0.18.0](https://github.com/esp-rs/esp-hal-community/blob/esp-hal-smartled-v0.18.0/esp-hal-smartled/Cargo.toml).
- CHANGELOG 0.18.0: «Updated `esp-hal` to `v1.2.0` and updated related dependencies» — [CHANGELOG.md 0.18.0](https://github.com/esp-rs/esp-hal-community/blob/esp-hal-smartled-v0.18.0/esp-hal-smartled/CHANGELOG.md).
- Внимание: RMT в esp-hal 1.x — unstable API; крейт требует `requires-unstable`, а пользователю нужно включить feature `unstable` у esp-hal (так делает сам крейт в dev-dependencies) — [Cargo.toml 0.18.0](https://github.com/esp-rs/esp-hal-community/blob/esp-hal-smartled-v0.18.0/esp-hal-smartled/Cargo.toml).
- Требования к тулчейну: `edition = "2024"`, `rust-version = "1.95"` — [Cargo.toml 0.18.0](https://github.com/esp-rs/esp-hal-community/blob/esp-hal-smartled-v0.18.0/esp-hal-smartled/Cargo.toml).

### 1.2. Текущий API

Всё ниже — по исходнику [src/lib.rs на теге v0.18.0](https://github.com/esp-rs/esp-hal-community/blob/esp-hal-smartled-v0.18.0/esp-hal-smartled/src/lib.rs):

- Основной тип — `RmtSmartLeds<'d, const BUFFER_SIZE: usize, Mode, C, Order>`; режим (blocking/async) выводится из типа RMT-канала: `Blocking` → трейт `SmartLedsWrite` (smart-leds-trait 0.3), `Async` → `SmartLedsWriteAsync`.
- Конструктор: `RmtSmartLeds::new(timing: Timing, channel, pin, rmt_freq: Rate)` — тайминги передаются **значением** (runtime-структура, есть `set_timing()`), а `rmt_freq` должен совпадать с частотой, заданной при создании `Rmt::new` (это изменение 0.18.0, см. CHANGELOG: «Now when creating the driver, the rmt clock frequency set when creating the Rmt instance must be provided»).
- Буферы: без кучи, размер задаётся const-generic'ой `BUFFER_SIZE = buffer_size::<C>(led_count)` = `led_count * размер_канала_в_битах * число_каналов + 2` (reset + end marker); превышение → `AdapterError::BufferSizeExceeded`. Для длинных лент — `new_with_memsize(..., memsize: u8, ...)`: канал занимает несколько RAM-блоков RMT (на C6 один блок = 48 PulseCode — [docs.rs: `CHANNEL_RAM_SIZE`](https://docs.rs/esp-hal/1.2.2/esp_hal/rmt/constant.CHANNEL_RAM_SIZE.html)).
- Канал берётся «взаймы»: `Rmt::new(peripherals.RMT, Rate::from_mhz(80))` (для async — `.into_async()`), затем `rmt.channel0`; для повторного использования канала — `ChannelCreator::reborrow`.
- Рабочий пример создания (async + Embassy) из примера самого крейта [hello_rgb_async.rs v0.18.0](https://github.com/esp-rs/esp-hal-community/blob/esp-hal-smartled-v0.18.0/esp-hal-smartled/examples/hello_rgb_async.rs):

  ```rust
  let rmt = Rmt::new(peripherals.RMT, Rate::from_mhz(80))?
      .into_async();
  let mut led = RmtSmartLeds::<{ buffer_size::<RGB8>(LEDS) }, _, RGB8, color_order::Grb>::new(
      esp_hal_smartled::WS2812B_TIMING, // в примере — WS2812_TIMING; для нашей ленты — WS2812B
      rmt.channel0,
      led_pin,
      freq, // тот же Rate, что в Rmt::new
  )?;
  // loop:
  let fut = led.write(brightness(gamma(data.iter().cloned()), 10));
  let (_, res) = join(Timer::after_millis(20), fut).await;
  ```

- Async-`write` возвращает future; подготовка RMT-буфера выполняется **до** первого `.await` (дизайн «готовим несколько записей, диспатчим одновременно» — см. пример выше).
- Нюанс документации: пример в rustdoc крейта ([lib.rs, crate-level example](https://github.com/esp-rs/esp-hal-community/blob/esp-hal-smartled-v0.18.0/esp-hal-smartled/src/lib.rs)) устарел — использует `Ws2812Timing` как тип-параметр и вызов `new(channel, pin)`, что не соответствует фактической сигнатуре (`new(timing, channel, pin, rmt_freq)`). Ориентироваться на `examples/`, а не на док-пример.
- Комментарий в коде `flush()` фиксирует известный недочёт RMT API: `transmit` поглощает канал, и при ошибке передачи драйвер остаётся в промежуточном состоянии (канал возвращается через `Err`, но состояние помечено как «unsafe state» в комментарии) — [lib.rs, `flush`](https://github.com/esp-rs/esp-hal-community/blob/esp-hal-smartled-v0.18.0/esp-hal-smartled/src/lib.rs).
- docs.rs: [esp-hal-smartled 0.18.0](https://docs.rs/esp-hal-smartled/0.18.0/esp_hal_smartled/index.html).

### 1.3. Тайминги WS2812B и RGBW (SK6812)

- Тайминги — runtime-структура `Timing { time_0_low/high, time_1_low/high (нс), reset_us }` с готовыми константами: `WS2812B_TIMING` (0: 400/800 нс, 1: 850/450 нс, reset 300 мкс), `WS2812_TIMING` (350/700, 800/600, 80 мкс), `SK68XX_TIMING` (0: 320 нс, 1: 640 нс, период 1200 нс, reset 300 мкс), `WS2811_LOW_SPEED_TIMING`/`WS2811_TIMING`; есть `Timing::with_reset_us()` — [lib.rs](https://github.com/esp-rs/esp-hal-community/blob/esp-hal-smartled-v0.18.0/esp-hal-smartled/src/lib.rs).
- Значения `WS2812B_TIMING` совпадают с даташитными 400/850/800/450 нс, reset ≥ 50 мкс — те же числа приводит [ws2812-rmt, комментарий `Timing::WS2812B_AT_12_5NS_TICK_ESP32C3`](https://github.com/jcmendez/ws2812-rmt/blob/main/src/lib.rs) («WS2812B (4-pin variant) datasheet timings — 400/850/800/450ns, 50us reset»).
- Reset-пульс **кодируется явно в буфере** (два `PulseCode` low по `reset_us/2`, ограничение: макс. длительность одного `PulseCode` = `0xfffe / freq_МГц` ≈ 819 мкс при 80 МГц) — [lib.rs, `reset_pulse` и doc `Timing.reset_us`](https://github.com/esp-rs/esp-hal-community/blob/esp-hal-smartled-v0.18.0/esp-hal-smartled/src/lib.rs).
- RGBW: поддерживается — цветовые типы `RGBW<T>` (и `RGBCCT`, `White`, `CctWhite`) с порядками `color_order::Rgbw`/`Grbw`; для SK6812 комбинация `RGBW` + `Grbw` + `SK68XX_TIMING` — [lib.rs, `color_order` и impl `Color for RGBW`](https://github.com/esp-rs/esp-hal-community/blob/esp-hal-smartled-v0.18.0/esp-hal-smartled/src/lib.rs).

### 1.4. 0.18.0 — слияние esp-hal-smartled2

- CHANGELOG 0.18.0: «Merged the changes made in [esp-hal-smartled2](https://github.com/kleinesfilmroellchen/esp-hal-smartled) by @kleinesfilmroellchen … with further contributions»; изменения поверх: «Reset pulse, so now two consecutive or very near in time writes behave correctly», «Timing is a struct … set at runtime», «Correct ticks calculations for the pulses», «…rmt clock frequency … must be provided» — [CHANGELOG.md 0.18.0](https://github.com/esp-rs/esp-hal-community/blob/esp-hal-smartled-v0.18.0/esp-hal-smartled/CHANGELOG.md).
- Слияние оформлено как [PR #58 «Contribute changes from kleinesfilmroellchen/esp-hal-smartled ?»](https://github.com/esp-rs/esp-hal-community/pull/58) (закрыт 2026-09-05, в тот же день, что и релиз 0.18.0); обновление до esp-hal 1.2 — [PR #74 «Update to esp-hal 1.2»](https://github.com/esp-rs/esp-hal-community/pull/74).
- Сам форк ликвидирован: репозиторий `kleinesfilmroellchen/esp-hal-smartled2` отдаёт 404, крейта `esp-hal-smartled2` на crates.io больше нет («crate `esp-hal-smartled2` does not exist» — ответ API crates.io на дату отчёта). Живая реализация — только в esp-hal-community 0.18.0.

---

## 2. Известные баги: порча цветов при частых обновлениях на C6

### 2.1. Претензия ws2812-rmt (jcmendez)

- [README ws2812-rmt](https://github.com/jcmendez/ws2812-rmt#why-this-exists): «`esp-hal-smartled2` proved unable to reliably update a WS2812 pixel to a *changing* value on repeated writes — a single steady color always rendered correctly, but blink/breathe patterns … reliably corrupted: either sticking on a stale frame, or bleeding a previously-sent channel's value into later frames». Заявленный корень: драйвер полагался на idle-low **между** передачами вместо явного reset-пульса ≥ 50 мкс в буфере (как в эталонном энкодере [esp-idf `led_strip_simple_encoder`](https://github.com/espressif/esp-idf/blob/master/examples/peripherals/rmt/led_strip_simple_encoder/main/led_strip_example_main.c)).
- Важно: претензия адресована **форку esp-hal-smartled2** (до слияния), а не официальному 0.18.0; форк удалён, воспроизвести/перепроверить претензию против 0.18.0 автор ws2812-rmt в README не сообщает.

### 2.2. Заявлен ли фикс в 0.18.0

- По сути — да, тот же класс проблемы: CHANGELOG 0.18.0 явно перечисляет «Reset pulse, so now two consecutive or very near in time writes behave correctly», а код 0.18.0 добавляет явный reset-пульс в конец каждого кадра (`create_rmt_data` пишет reset-`PulseCode` + end marker; `WS2812B_TIMING.reset_us = 300`) — [CHANGELOG](https://github.com/esp-rs/esp-hal-community/blob/esp-hal-smartled-v0.18.0/esp-hal-smartled/CHANGELOG.md), [lib.rs](https://github.com/esp-rs/esp-hal-community/blob/esp-hal-smartled-v0.18.0/esp-hal-smartled/src/lib.rs). Это ровно тот механизм, отсутствие которого ws2812-rmt называет корнем бага.
- Независимого подтверждения (issue «починилось на 0.18.0») нет; открытых issue о мерцании/порче цветов на C6 **против 0.18.0** не найдено (полный список открытых issue репозитория на дату отчёта: #72, #65, #61, #55, #35, #32 — [issues esp-hal-community](https://github.com/esp-rs/esp-hal-community/issues)). Оговорка: 0.18.0 опубликован всего ~7 недель назад.

### 2.3. Открытые issue по мерцанию на C6

- [#32 «esp-hal-smartled does not support multiple LED's correctly»](https://github.com/esp-rs/esp-hal-community/issues/32) — **открыт** (создан 2025-06-22, обновлён 2026-02-24). ESP32-C6 (Waveshare C6-LCD-1.9): при записи нескольких LED async-API давало паузу ~850 мкс между пакетами (debug-сборка) — лента трактует паузу > 50 мкс как reset, кадры «залипают»/теряются (логические осциллограммы в issue). В треде: в release-сборке пауза падала до ~1 мкс и проблема у одного из участников исчезла ([комментарий clarkmcc, 2026-02-20](https://github.com/esp-rs/esp-hal-community/issues/32#issuecomment)); корень — ручная нарезка кадра на несколько RMT-передач и задержки между ними; отмечено, что [esp-hal PR #4049](https://github.com/esp-rs/esp-hal/pull/4049) (async «wrapping tx», вошёл в esp-hal 1.0.0-rc.1) «should also greatly help with #32, if someone creates a PR to use async wrapping tx». Актуален ли #32 после переписывания 0.18.0 (кадр передаётся одним `transmit`) — не подтверждено ни в какую сторону.
- [#65 «Async RBGW Chunking the Wrong Size»](https://github.com/esp-rs/esp-hal-community/issues/65) — **открыт** (2026-05-21): async-адаптер с `RGBA<u8>` (RGBW) использовал неверный размер чанка → `TransmissionError(EndMarkerMissing)`. Ссылка в issue ведёт на старый код (коммит 7350f88); в 0.18.0 кодирование переписано (без нарезки чанков), но фикс в changelog не упомянут и подтверждений нет.

---

## 3. Альтернативы

### 3.1. ws2812-rmt (jcmendez)

- Свежесть: v0.2.0 опубликован 2026-07-03, репозиторий создан 2026-07-03, последний пуш 2026-07-06, 0 звёзд — [crates.io/crates/ws2812-rmt](https://crates.io/crates/ws2812-rmt), [GitHub jcmendez/ws2812-rmt](https://github.com/jcmendez/ws2812-rmt).
- Зависимости (опубликовано): `esp-hal = "~1.1.0"` + `requires-unstable`/`unstable`, `smart-leds-trait = "0.3"` (только ради типа `RGB8`) — [Cargo.toml из крейта 0.2.0](https://crates.io/crates/ws2812-rmt/0.2.0). **`~1.1.0` несовместим semver с esp-hal 1.2.x** — как зависимость в наш стек не встанет без форка/апгрейда манифеста.
- API: только blocking (`Ws2812::<{ buffer_len(N) }>::new(channel, pin, timing, clk_divider)`, `write(&[RGB8]) -> Result<(), RmtError>`); RGB8-only (нет RGBW); явный reset-пульс в буфере; busy-poll `transmit(..).wait()` — [src/lib.rs](https://github.com/jcmendez/ws2812-rmt/blob/main/src/lib.rs).
- Ценность для нас: задокументированные отладкой на железе тайминги C6 (`Rmt::new(80 МГц)` + `clk_divider=2` → тик 12.5 нс, пресет `WS2812_AT_12_5NS_TICK_ESP32C6`) и разбор корня бага reset/idle — [README](https://github.com/jcmendez/ws2812-rmt#rmt-clock-tick-period-is-chip--and-configuration-specific--verify-it). CI только C3/C6.

### 3.2. ws2812-spi

- Свежесть: 0.5.1 от 2025-06-03 (до этого 0.5.0 — 2024-07-29) — [crates.io/crates/ws2812-spi](https://crates.io/crates/ws2812-spi); репозиторий [smart-leds-rs/ws2812-spi-rs](https://github.com/smart-leds-rs/ws2812-spi-rs) (README, Cargo.toml).
- Зависимости: `embedded-hal = "1.0.0"`, `smart-leds-trait = "0.3"` — HAL-агностик, без привязки к esp-hal.
- Механика ([README](https://github.com/smart-leds-rs/ws2812-spi-rs#readme)): кодирование WS2812 через SPI 2–3.8 МГц, данные формируются «на лету» (нужен достаточно быстрый CPU, ~48 МГц) либо prerendered-вариантом; feature `mosi_idle_high` для SPI с idle-high на MOSI; RGB-only (WS2812, 24 бита; RGBW-варианта в крейте нет).
- Для C6: работоспособно (SPI esp-hal умеет нужные частоты), но тратит CPU/шину на «пустые биты», чувствительно к частоте SPI и скорости ядра — сам README рекомендует проверку логическим анализатором. Резервный вариант, если RMT-каналы заняты.

### 3.3. Собственный минимальный RMT-TX драйвер на esp-hal 1.2

Оценка сложности — **низкая**: вся механика уже в HAL, драйверу остаётся только кодирование. По [esp-hal 1.2.2, модуль `esp_hal::rmt`](https://github.com/esp-rs/esp-hal/blob/esp-hal-v1.2.2/esp-hal/src/rmt.rs) (локальная копия: `~/.cargo/registry/src/index.crates.io-*/esp-hal-1.2.2/src/rmt.rs`, docs: [docs.rs rmt](https://docs.rs/esp-hal/1.2.2/esp_hal/rmt/index.html)):

- Создание: `Rmt::new(p.RMT, Rate::from_mhz(80))` (на C6; документированный пример TX — там же, «TX operation»), конфиг канала `TxChannelConfig::default().with_clk_divider(1).with_idle_output(true).with_idle_output_level(Level::Low)` → `configure_tx(...)?.with_pin(pin)`.
- Кодирование: `PulseCode::try_new(Level::High, ticks, Level::Low, ticks)` — поля длительности 15-битные (`MAX_LEN = 0x7FFF`), конец кадра — `PulseCode::end_marker()`; перевод нс→тики: `ns * freq_mhz / 1000` (та же формула, что в esp-hal-smartled).
- Передача: blocking — `channel.transmit(&buf)` → `TxTransaction::wait()`; async — `channel.transmit(&buf).await` (`impl Future`, Embassy-native, без embassy-time зависимостей).
- Длина кадра **не ограничена RAM-блоком**: TX-драйвер esp-hal 1.2 использует hardware wrap + дозапись половинами по threshold-прерыванию (`RmtWriter::write`, `start_send` ставит порог `memsize/2`; «wrapping tx is supported on all devices») — [src/rmt/writer.rs](https://github.com/esp-rs/esp-hal/blob/esp-hal-v1.2.2/esp-hal/src/rmt/writer.rs), [rmt.rs](https://github.com/esp-rs/esp-hal/blob/esp-hal-v1.2.2/esp-hal/src/rmt.rs). Т.е. длинные ленты передаются одним `transmit` без «дневных просветов», которые были причиной #32.
- Reset: кодировать явно в буфере (2 × `PulseCode` low; ≤ 0xfffe тиков на код ≈ 819 мкс при 80 МГц — достаточно для 300 мкс WS2812B).
- Частота: docs `Rmt::new` прямо предупреждают — фактическую частоту брать из `Rmt::frequency()`, она «not necessarily the APB clock frequency» ([rmt.rs, docs `Rmt::new`](https://github.com/esp-rs/esp-hal/blob/esp-hal-v1.2.2/esp-hal/src/rmt.rs)).
- Против пауз flash-кэша (см. #32) есть опция `place-rmt-driver-in-ram` («Places the RMT driver in RAM for better performance») — [esp_config.yml esp-hal 1.2.2](https://github.com/esp-rs/esp-hal/blob/esp-hal-v1.2.2/esp-hal/esp_config.yml).
- Объём работ: ~150–250 строк (кодирование WS2812B + опционально SK6812 RGBW + минимальный тип драйвера) + проверка на железе. По сути esp-hal-smartled 0.18.0 и есть такой драйвер (~400 строк с учётом цветовых порядков), поэтому свой вариант имеет смысл только при отказе от smart-leds-обвязки или как «план Б» при воспроизведении багов.

---

## 4. Выводы для проекта

1. **Основной вариант: `esp-hal-smartled 0.18.0`**. Официально совместим с esp-hal `~1.2`, поддерживает Embassy/async (нативный `SmartLedsWriteAsync`, future без аллокаций), готовые тайминги `WS2812B_TIMING` и порядок `color_order::Grb` (WS2812B — GRB), плюс готовый путь к RGBW (`RGBW` + `Grbw` + `SK68XX_TIMING`) при смене ленты на SK6812. Источники: §1.2–1.3.
2. Обязательные условия конфигурации: feature `unstable` у esp-hal; передавать в драйвер тот же `Rate`, что и в `Rmt::new`; размер буфера — `buffer_size::<C>(N)` (для гирлянды на сотни точек — десятки КБ статического буфера: 24·N+2 слова × 4 байта; для 300 LED ≈ 29 КБ); rust-toolchain ≥ 1.95 (edition 2024).
3. Претензия ws2812-rmt о порче цветов при частых обновлениях относилась к удалённому форку `esp-hal-smartled2`; в официальном 0.18.0 заявлен и реализован явный reset-пульс — тот же механизм, который ws2812-rmt считает фиксом. Независимого подтверждения нет → обязательно проверить на нашем железе паттернами blink/breathe (частые обновления) и оценить паузы на пине логическим анализатором (критерий: пауза между кодами кадра < 50 мкс).
4. **Запасной вариант: собственный минимальный RMT-TX драйвер на esp-hal 1.2** (WS2812B, при надобности RGBW). Сложность низкая: RMT API esp-hal 1.2 закрывает и кодирование (`PulseCode`), и async (`transmit().await`), и длинные кадры (hardware wrap), и проблему flash-пауз (`place-rmt-driver-in-ram`). Этот же вариант — референс для точечной замены, если у 0.18.0 воспроизведётся «залипание» кадров.
5. `ws2812-rmt` как зависимость непригоден (esp-hal `~1.1`, blocking-only, RGB8-only, нулевое adoption), но полезен как источник проверенных на C6 таймингов и обоснования reset-пульса. `ws2812-spi` — только как крайний резерв (RGB-only, нагрузка на CPU, требует проверки частоты SPI).

## 5. Открытые вопросы

1. Не подтверждено независимо, устраняет ли 0.18.0 коррупцию цветов при частых обновлениях, описанную ws2812-rmt для форка smartled2: changelog фикс заявляет, но публичных подтверждений/issue нет. Нужен Hardware-тест (blink/breathe на C6, логический анализатор).
2. Судьба issue [#32](https://github.com/esp-rs/esp-hal-community/issues/32) (паузы между LED в async-режиме на C6) после переписывания драйвера в 0.18.0 не ясна — issue открыт, но не переоткрыт против 0.18.0; нет ни отчёта «починилось», ни «не починилось».
3. RGBW/SK6812 на async: открытый [#65](https://github.com/esp-rs/esp-hal-community/issues/65) (неверный размер чанка для RGBA) относился к старому коду; подтверждения корректности RGBW+async в 0.18.0 отсутствуют — при переходе на SK6812 проверить в первую очередь.
4. Расхождение по реальной частоте RMT на C6: ws2812-rmt эмпирически требует `clk_divider=2` при `Rmt::new(80 МГц)` (ссылаясь на «фиксированные 80 МГц PLL»), тогда как esp-hal-smartled считает тики от переданных 80 МГц с `clk_divider=1`. Docs esp-hal 1.2 требуют брать фактическую частоту из `Rmt::frequency()`. Какое поведение в 1.2.2 на C6 — проверить замером (`Rmt::frequency()` + логический анализатор), прежде чем фиксировать тайминги.
5. Изменчивость младшего стека: 0.18.0 вышел недавно (2026-09-05), открытых регрессий по нему не видно, но история 0.15→0.17→0.18 (переписывания, слияние форка, issue #66 о несовместимости 0.17 с esp-hal 1.1) показывает высокую волатильность API — при апдейтах читать CHANGELOG/MIGRATION до поднятия версий.
6. Драйвер требует esp-hal feature `unstable` (RMT нестабилен в esp-hal 1.x) — принять это как постоянное условие сборки проекта или вынести в решение по зависимостям.
