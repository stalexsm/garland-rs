//! Прошивка Гирлянды: первый свет на Стенде. Ядро — esp-hal/esp-rtos (async),
//! Лента — WS2812B через esp-hal-smartled (RMT-TX, порядок GRB).
//!
//! Жизненный цикл:
//!
//! 1. Тестовая сетка Раскладки (`TEST_GRID_SECS` секунд): белый бегунок идёт
//!    по Ленте и показывает порядок змейки, красный Пиксель 0 держит
//!    стартовый угол.
//! 2. Офлайн-гирлянда: Реестр эффектов под управлением кнопки — короткое
//!    нажатие включает следующий Эффект по кругу кроссфейдом (~0,8 с: два
//!    Фреймбуфера, 16-битная альфа), длинное (~0,8 с) переводит в Покой
//!    (чёрная Лента), короткое из Покоя возвращает Текущий эффект мгновенно.
//!    Номер Текущего эффекта живёт в NVS-разделе (вне Слотов): пишется при
//!    переключении, после подачи питания включается сохранённый. Сеть не
//!    нужна.
//! 3. Если при сборке задан `DRIVER_GATE=1`, вместо пункта 2 — непрерывный
//!    blink/breathe: железный гейт драйвера на залипания и порчу цветов при
//!    частых обновлениях.
//!
//! Параллельно с Лентой (когда заданы учётные данные) живёт сеть: Wi-Fi STA
//! с авто-переподключением, DHCP с именем `garland` и HTTP-сервер, чья
//! страница по IP Гирлянды показывает версию Прошивки и принимает образ
//! OTA-обновления (`POST /ota`): тело пишется в неактивный Слот
//! (`esp-bootloader-esp-idf`), Слот активируется, Гирлянда перезагружается;
//! обрыв или битый образ активации не получают, прежняя Прошивка продолжает
//! работать. После старта новая Прошивка подтверждает свой Слот (`Valid`) —
//! без подтверждения загрузчик с auto-rollback выполнит Откат; образ, битый
//! внутри, отсеет сам загрузчик. Автооткат при зависании даёт только свой
//! загрузчик с `CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE` (в prebuilt espflash
//! он выключен — см. ADR-0002). NVS-раздел — вне Слотов: обновление не
//! трогает память эффекта. Недоступная сеть не касается эффектов — задачи
//! независимы.
//!
//! Параметры задаются переменными окружения при сборке (см. `build.rs`),
//! дефолты — Стенд 8×8:
//!
//! | Переменная      | Значения                                                         | Дефолт            |
//! |-----------------|-------------------------------------------------------------------|-------------------|
//! | `MATRIX_W`      | ширина Матрицы в Пикселях                                         | `8`               |
//! | `MATRIX_H`      | высота Матрицы в Пикселях                                         | `8`               |
//! | `LAYOUT`        | `serpentine-rows`, `plain-rows`, `serpentine-cols`, `plain-cols`  | `serpentine-rows` |
//! | `START_CORNER`  | `top-left`, `top-right`, `bottom-left`, `bottom-right`            | `top-left`        |
//! | `LED_PIN`       | номер GPIO Ленты (ESP32-C6: 0..=30)                               | `10`              |
//! | `BUTTON_PIN`    | номер GPIO кнопки (ESP32-C6: 0..=30; BOOT на DevKitC-1 — `9`)      | `9`               |
//! | `MAX_BRIGHTNESS`| потолок яркости канала, 0..=255                                   | `64`              |
//! | `TEST_GRID_SECS`| длительность тестовой сетки, секунд (0 — выключить)               | `8`               |
//! | `DRIVER_GATE`   | `1` — после сетки непрерывный blink/breathe вместо гирлянды        | `0`               |
//! | `WIFI_SSID`     | имя домашней сети (пустое — офлайн-режим без сети)                | —                 |
//! | `WIFI_PASS`     | пароль Wi-Fi                                                      | —                 |
//!
//! Сборка и заливка — из каталога `fw` (target и runner прописаны в
//! `.cargo/config.toml`; заливка ставит таблицу разделов `partitions.csv` —
//! nvs + otadata + ota_0 + ota_1 под флеш 4 МБ — и свой Загрузчик с
//! автооткатом `bootloader/esp32c6-rollback.bin`):
//!
//! ```text
//! cargo build                          # Прошивка под Стенд 8×8
//! MATRIX_W=16 cargo build              # та же Прошивка под Матрицу 16×N
//! DRIVER_GATE=1 cargo run              # гейт драйвера: blink/breathe
//! WIFI_SSID=HomeNet WIFI_PASS=secret cargo run   # сеть + страница версии по IP
//! cargo run                            # сборка и заливка через espflash
//! ```
//!
//! Образ для OTA-заливки со страницы Гирлянды (или `curl --data-binary
//! @garland.bin http://<IP>/ota`):
//!
//! ```text
//! cargo build --release
//! espflash save-image --chip esp32c6 \
//!     target/riscv32imac-unknown-none-elf/release/fw garland.bin
//! ```
//!
//! На хост-таргетах крейт собирается пустой заглушкой — только чтобы гейты
//! workspace (`cargo fmt/clippy/test`) шли без железа; настоящий код прошивки
//! компилируется только под `riscv32`. Диагностические кадры (`diag`)
//! — чистые функции, их тесты идут на хосте.
#![cfg_attr(target_arch = "riscv32", no_std)]
#![cfg_attr(target_arch = "riscv32", no_main)]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

// На хосте модуль нужен только тестам: в bin-сборку хоста не включаем.
#[cfg(any(test, target_arch = "riscv32"))]
mod button;
#[cfg(any(test, target_arch = "riscv32"))]
mod control;
// Мини-HTTP — чистые функции, хост-тесты идут без железа.
#[cfg(any(test, target_arch = "riscv32"))]
mod diag;
// Мини-HTTP — чистые функции, хост-тесты идут без железа.
#[cfg(any(test, target_arch = "riscv32"))]
mod http;
// Запись OTA-образа: буферизация чистая — хост-тесты; носитель и подтверждение
// слота — device-часть модуля.
#[cfg(any(test, target_arch = "riscv32"))]
mod ota;
// Память эффекта (NVS) — только на устройстве: хост-покрытие шва хранилища
// живёт в тестах control (подставное хранилище).
#[cfg(target_arch = "riscv32")]
mod memory;
// Сеть — только на железе.
#[cfg(target_arch = "riscv32")]
mod net;

// Размер куч (конфигурация примера esp-hal для Wi-Fi, см.
// docs/research/wifi-http.md): reclaimed-RAM — под аллокации прошивки,
// обычный heap — под буферы Wi-Fi, которые esp-radio берёт из esp-alloc.
#[cfg(target_arch = "riscv32")]
const RECLAIMED_HEAP_SIZE: usize = 64 * 1024;
#[cfg(target_arch = "riscv32")]
const HEAP_SIZE: usize = 36 * 1024;

// Параметры из окружения сборки; мусор в конфиге ломает сборку, а не старт.
#[cfg(target_arch = "riscv32")]
const MATRIX_W: u16 = env_u16(env!("MATRIX_W"));
#[cfg(target_arch = "riscv32")]
const MATRIX_H: u16 = env_u16(env!("MATRIX_H"));
#[cfg(target_arch = "riscv32")]
const LED_PIN: u16 = env_u16(env!("LED_PIN"));
#[cfg(target_arch = "riscv32")]
const BUTTON_PIN: u16 = env_u16(env!("BUTTON_PIN"));
#[cfg(target_arch = "riscv32")]
const MAX_BRIGHTNESS: u8 = env_u8(env!("MAX_BRIGHTNESS"));
#[cfg(target_arch = "riscv32")]
const TEST_GRID_SECS: u16 = env_u16(env!("TEST_GRID_SECS"));
#[cfg(target_arch = "riscv32")]
const DRIVER_GATE: bool = env_bool(env!("DRIVER_GATE"));
#[cfg(target_arch = "riscv32")]
const WIFI_SSID: &str = env!("WIFI_SSID");
#[cfg(target_arch = "riscv32")]
const WIFI_PASS: &str = env!("WIFI_PASS");

// Ёмкости типов esp-radio ловятся на сборке: кривой конфиг ломает сборку,
// а не старт на устройстве.
#[cfg(target_arch = "riscv32")]
const _: () = {
    assert!(WIFI_SSID.len() <= 32, "WIFI_SSID: длиннее 32 байт");
    assert!(WIFI_PASS.len() <= 64, "WIFI_PASS: длиннее 64 байт");
};

// Сеть поднимается только с полными учётными данными: пустые значения —
// офлайн-режим, эффекты работают без сети.
#[cfg(target_arch = "riscv32")]
const WIFI_CREDENTIALS_SET: bool = !WIFI_SSID.is_empty() && !WIFI_PASS.is_empty();

// Число Пикселей Матрицы — размер статических буферов кадра.
#[cfg(target_arch = "riscv32")]
const PIXELS: usize = MATRIX_W as usize * MATRIX_H as usize;

// Такт обновления кадров, мс: около 30 кадров/с — частые записи греют гейт
// драйвера, для сетки точность не критична.
#[cfg(target_arch = "riscv32")]
const FRAME_TICK_MS: u32 = 33;

// Длительность фаз blink и breathe в цикле гейта, мс.
#[cfg(target_arch = "riscv32")]
const GATE_PHASE_MS: u32 = 5000;

// Частота RMT, МГц: канон таймингов WS2812B на ESP32-C6 (пример драйвера).
#[cfg(target_arch = "riscv32")]
const RMT_FREQ_MHZ: u32 = 80;

// Такт спящего цикла после включения статичного цвета, секунд.
#[cfg(target_arch = "riscv32")]
const IDLE_TICK_SECS: u64 = 3600;

#[cfg(target_arch = "riscv32")]
const MS_PER_SEC: u32 = 1000;

/// Десятичная строка окружения сборки → `u16` на этапе компиляции.
#[cfg(target_arch = "riscv32")]
const fn env_u16(s: &str) -> u16 {
    let bytes = s.as_bytes();
    let mut value: u16 = 0;
    let mut i = 0;
    while i < bytes.len() {
        assert!(bytes[i].is_ascii_digit(), "ожидалось число");
        value = value * 10 + (bytes[i] - b'0') as u16;
        i += 1;
    }
    value
}

/// То же с проверкой диапазона `u8`.
#[cfg(target_arch = "riscv32")]
const fn env_u8(s: &str) -> u8 {
    let value = env_u16(s);
    assert!(value <= u8::MAX as u16, "значение не помещается в байт");
    value as u8
}

/// Флаг окружения: включён только «1».
#[cfg(target_arch = "riscv32")]
const fn env_bool(s: &str) -> bool {
    let bytes = s.as_bytes();
    bytes.len() == 1 && bytes[0] == b'1'
}

#[cfg(target_arch = "riscv32")]
use embassy_executor::Spawner;
#[cfg(target_arch = "riscv32")]
use embassy_time::{Duration, Timer};
#[cfg(target_arch = "riscv32")]
use esp_backtrace as _;
#[cfg(target_arch = "riscv32")]
use esp_hal::clock::CpuClock;
#[cfg(target_arch = "riscv32")]
use esp_hal::gpio::{AnyPin, Input, InputConfig, Pin as _, Pull};
#[cfg(target_arch = "riscv32")]
use esp_hal::peripherals::RMT;
#[cfg(target_arch = "riscv32")]
use esp_hal::rmt::Rmt;
#[cfg(target_arch = "riscv32")]
use esp_hal::rng::Rng;
#[cfg(target_arch = "riscv32")]
use esp_hal::system::software_reset;
#[cfg(target_arch = "riscv32")]
use esp_hal::time::Rate;
#[cfg(target_arch = "riscv32")]
use esp_hal::timer::timg::TimerGroup;
#[cfg(target_arch = "riscv32")]
use esp_hal_smartled::{AdapterError, RmtSmartLeds, WS2812B_TIMING, buffer_size, color_order};
#[cfg(target_arch = "riscv32")]
use esp_storage::FlashStorage;
#[cfg(target_arch = "riscv32")]
extern crate alloc;
#[cfg(target_arch = "riscv32")]
use crate::button::Button;
#[cfg(target_arch = "riscv32")]
use crate::control::{Garland, REGISTRY};
#[cfg(target_arch = "riscv32")]
use alloc::boxed::Box;
#[cfg(target_arch = "riscv32")]
use garland_core::{Corner, Frame, Layout, Mapping};
#[cfg(target_arch = "riscv32")]
use log::{error, info, warn};
#[cfg(target_arch = "riscv32")]
use smart_leds::{RGB8, SmartLedsWriteAsync};

// Дескриптор приложения, обязательный для загрузчика esp-idf.
#[cfg(target_arch = "riscv32")]
esp_bootloader_esp_idf::esp_app_desc!();

/// Перекладывает Кадр в буфер Ленты и передаёт драйверу; ошибку записи
/// считаем фатальной. Кадр логический (строка-мажор), Лента — физический
/// порядок: перед отправкой работает Привязка координат ([`Mapping`]).
#[cfg(target_arch = "riscv32")]
async fn show(
    led: &mut impl SmartLedsWriteAsync<Color = RGB8, Error = AdapterError>,
    strip: &mut [RGB8],
    frame: &Frame<'_>,
    mapping: &Mapping,
) {
    let mut ordered = [garland_core::Rgb8::BLACK; PIXELS];
    diag::to_strip(frame, mapping, &mut ordered);
    for (dst, src) in strip.iter_mut().zip(ordered.iter()) {
        let color = diag::encode(*src, MAX_BRIGHTNESS);
        *dst = RGB8 {
            r: color.r,
            g: color.g,
            b: color.b,
        };
    }
    // Ошибка RMT не паникует: паника в задаче вешает устройство на застывшем
    // кадре без самовосстановления; перезапуск поднимает Гирлянду в тестовую
    // сетку.
    if let Err(e) = led.write(strip.iter().copied()).await {
        error!("запись Ленты не удалась: {e:?} — перезапуск");
        software_reset();
    }
}

/// Кадр на Матрице: возвращает Фреймбуфер, привязанный к срезу `frame_buffer`.
/// Макро — из-за заимствования буфера на каждую итерацию циклов.
#[cfg(target_arch = "riscv32")]
macro_rules! next_frame {
    ($frame_buffer:ident) => {
        Frame::new(MATRIX_W, MATRIX_H, &mut $frame_buffer).expect("размер Фреймбуфера")
    };
}

/// Тип драйвера Ленты этой Прошивки: async-канал RMT, тайминги WS2812B, порядок GRB.
#[cfg(target_arch = "riscv32")]
type StripDriver<'d> =
    RmtSmartLeds<'d, { buffer_size::<RGB8>(PIXELS) }, esp_hal::Async, RGB8, color_order::Grb>;

/// Создаёт драйвер Ленты и сразу кладёт его в кучу: его RMT-буфер для 64
/// Пикселей — около 6 КБ, и он не должен ни лежать в фрейме задачи, ни
/// делить его с конструированием. Отдельная обычная функция — тяжёлые
/// временные живут в её собственном фрейме.
#[cfg(target_arch = "riscv32")]
#[expect(
    clippy::large_stack_frames,
    reason = "разовая сборка драйвера держит временный RMT-буфер на ~6 КБ; сам драйвер уходит в кучу"
)]
fn init_strip_driver(
    rmt_peripheral: RMT<'static>,
    led_pin: AnyPin<'static>,
) -> Box<StripDriver<'static>> {
    Box::new({
        let rmt = Rmt::new(rmt_peripheral, Rate::from_mhz(RMT_FREQ_MHZ))
            .expect("RMT: инициализация не удалась")
            .into_async();
        RmtSmartLeds::new(
            WS2812B_TIMING,
            rmt.channel0,
            led_pin,
            Rate::from_mhz(RMT_FREQ_MHZ),
        )
        .expect("RMT: конфигурация канала не удалась")
    })
}

/// Жизненный цикл Ленты: тестовая сетка Раскладки, затем гейт драйвера или
/// офлайн-гирлянда — Реестр эффектов под управлением кнопки. Отдельная
/// embassy-задача: её фьючерс статический (bss), стек `main` не несёт ни
/// драйвера, ни буферов кадра.
// large_stack_frames меряет фьючерс задачи, а embassy выделяет его статически
// (bss), стек при работе не расходуется; ожидание (expect) этот поздний линт
// не связывает, поэтому прицельное разрешение (allow).
#[cfg(target_arch = "riscv32")]
#[allow(
    clippy::large_stack_frames,
    reason = "фьючерс embassy-задачи выделяется статически (bss), стек не расходуется"
)]
#[embassy_executor::task]
async fn strip_task(
    rmt_peripheral: RMT<'static>,
    led_pin: AnyPin<'static>,
    button_pin: AnyPin<'static>,
    flash: FlashStorage<'static>,
    mapping: Mapping,
) {
    let mut led = init_strip_driver(rmt_peripheral, led_pin);
    let mut frame_buffer = [garland_core::Rgb8::BLACK; PIXELS];
    let mut outgoing_buffer = [garland_core::Rgb8::BLACK; PIXELS];
    let mut strip_buffer = [RGB8::default(); PIXELS];

    // Фаза 1: тестовая сетка Раскладки.
    info!(
        "Тестовая сетка: {TEST_GRID_SECS} с, бегунок — порядок змейки, красный Пиксель 0 — стартовый угол"
    );
    let grid_end_ms = u32::from(TEST_GRID_SECS) * MS_PER_SEC;
    let mut elapsed_ms: u32 = 0;
    while elapsed_ms < grid_end_ms {
        let mut frame = next_frame!(frame_buffer);
        diag::test_grid(&mut frame, &mapping, elapsed_ms);
        show(led.as_mut(), &mut strip_buffer, &frame, &mapping).await;
        Timer::after(Duration::from_millis(u64::from(FRAME_TICK_MS))).await;
        elapsed_ms += FRAME_TICK_MS;
    }

    // Гейт драйвера (DRIVER_GATE=1) — пункт 3 жизненного цикла: непрерывный
    // blink/breathe вместо офлайн-гирлянды.
    if DRIVER_GATE {
        info!("Гейт драйвера: непрерывный blink/breathe, фазы по {GATE_PHASE_MS} мс");
        let mut elapsed_ms: u32 = 0;
        loop {
            // Часы фазы идут с нуля внутри каждой: breathe не стартует с
            // середины дыхания, blink — с начала вспышки.
            let phase_ms = elapsed_ms % (2 * GATE_PHASE_MS);
            let color = if phase_ms < GATE_PHASE_MS {
                diag::blink(phase_ms)
            } else {
                diag::breathe(phase_ms - GATE_PHASE_MS)
            };
            let mut frame = next_frame!(frame_buffer);
            frame.fill(color);
            show(led.as_mut(), &mut strip_buffer, &frame, &mapping).await;
            Timer::after(Duration::from_millis(u64::from(FRAME_TICK_MS))).await;
            // Циклы диагностических кадров короче переполнения u32 в
            // миллисекундах, так что обёртка счётчика не видна глазу.
            elapsed_ms = elapsed_ms.wrapping_add(FRAME_TICK_MS);
        }
    }

    // Фаза 2: офлайн-гирлянда. Кнопка замыкает Пин на землю: зажата —
    // низкий уровень (подтяжка вверх). Память Текущего эффекта — NVS-раздел
    // вне Слотов: индекс читается на старте и пишется при переключении.
    let button_gpio = Input::new(button_pin, InputConfig::default().with_pull(Pull::Up));
    let mut button = Button::new();
    let mut nvs = memory::NvsMemory::new(flash).await;
    let mut garland = Garland::new(REGISTRY, mapping, grid_end_ms, &mut nvs);
    info!(
        "Офлайн-гирлянда: эффектов в Реестре — {}, кнопка на GPIO{BUTTON_PIN}, сохранённый эффект — «{}»",
        REGISTRY.len(),
        garland.effect_name()
    );
    let mut now_ms = grid_end_ms;
    loop {
        if let Some(event) = button.update(button_gpio.is_low(), now_ms) {
            garland.press(event, now_ms);
            info!(
                "Кнопка: {event:?}; Текущий эффект — «{}»",
                garland.effect_name()
            );
        }
        let mut frame = next_frame!(frame_buffer);
        let mut outgoing = next_frame!(outgoing_buffer);
        garland.render(&mut frame, &mut outgoing, now_ms);
        show(led.as_mut(), &mut strip_buffer, &frame, &mapping).await;
        Timer::after(Duration::from_millis(u64::from(FRAME_TICK_MS))).await;
        // Шкала wrapping u32 переполняется за 49 суток; арифметика машин
        // кнопки и Гирлянды — на wrapping, переход незаметен.
        now_ms = now_ms.wrapping_add(FRAME_TICK_MS);
    }
}

/// Настоящая Прошивка — только под target riscv32imac-unknown-none-elf.
// large_stack_frames меряет фьючерс embassy-задачи main вместе с разовой
// инициализацией сети; embassy выделяет фьючерс статически (bss), стек при
// работе не расходуется; ожидание (expect) этот поздний линт не связывает,
// поэтому прицельное разрешение (allow).
#[cfg_attr(
    target_arch = "riscv32",
    allow(
        clippy::large_stack_frames,
        reason = "фьючерс embassy-задачи выделяется статически (bss), стек не расходуется"
    )
)]
#[cfg(target_arch = "riscv32")]
#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    esp_println::logger::init_logger_from_env();

    let peripherals = esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::max()));

    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: RECLAIMED_HEAP_SIZE);
    esp_alloc::heap_allocator!(size: HEAP_SIZE);

    // Периферия разбирается один раз: каждое поле одноразовое, поэтому все
    // GPIO стираются до [`AnyPin`] одним списком, и из него по номерам из
    // конфигурации сборки выбираются Пин Ленты и Пин кнопки.
    let esp_hal::peripherals::Peripherals {
        RMT,
        TIMG0,
        FROM_CPU_INTR0,
        WIFI,
        FLASH,
        GPIO0,
        GPIO1,
        GPIO2,
        GPIO3,
        GPIO4,
        GPIO5,
        GPIO6,
        GPIO7,
        GPIO8,
        GPIO9,
        GPIO10,
        GPIO11,
        GPIO12,
        GPIO13,
        GPIO14,
        GPIO15,
        GPIO16,
        GPIO17,
        GPIO18,
        GPIO19,
        GPIO20,
        GPIO21,
        GPIO22,
        GPIO23,
        GPIO24,
        GPIO25,
        GPIO26,
        GPIO27,
        GPIO28,
        GPIO29,
        GPIO30,
        ..
    } = peripherals;
    // Конфликт и диапазон пинов ловятся на сборке: оба номера известны
    // компилятору — мусор в конфиге ломает сборку, а не старт.
    const {
        assert!(LED_PIN != BUTTON_PIN, "LED_PIN и BUTTON_PIN совпадают");
        assert!(
            LED_PIN <= 30 && BUTTON_PIN <= 30,
            "номер Пина вне диапазона ESP32-C6 (0..=30)"
        );
    }
    let mut pins = [
        GPIO0.degrade(),
        GPIO1.degrade(),
        GPIO2.degrade(),
        GPIO3.degrade(),
        GPIO4.degrade(),
        GPIO5.degrade(),
        GPIO6.degrade(),
        GPIO7.degrade(),
        GPIO8.degrade(),
        GPIO9.degrade(),
        GPIO10.degrade(),
        GPIO11.degrade(),
        GPIO12.degrade(),
        GPIO13.degrade(),
        GPIO14.degrade(),
        GPIO15.degrade(),
        GPIO16.degrade(),
        GPIO17.degrade(),
        GPIO18.degrade(),
        GPIO19.degrade(),
        GPIO20.degrade(),
        GPIO21.degrade(),
        GPIO22.degrade(),
        GPIO23.degrade(),
        GPIO24.degrade(),
        GPIO25.degrade(),
        GPIO26.degrade(),
        GPIO27.degrade(),
        GPIO28.degrade(),
        GPIO29.degrade(),
        GPIO30.degrade(),
    ];
    pins.swap(0, LED_PIN as usize);
    // Кнопка, стоявшая в слоте 0, после первого обмена лежит на месте Ленты.
    let button_slot = if BUTTON_PIN == 0 {
        LED_PIN as usize
    } else {
        BUTTON_PIN as usize
    };
    pins.swap(1, button_slot);
    let [led_pin, button_pin, ..] = pins;

    let timg0 = TimerGroup::new(TIMG0);
    esp_rtos::start(timg0.timer0, FROM_CPU_INTR0);

    // Привязка координат — из параметров сборки; кривой конфиг падает на старте.
    let layout: Layout = env!("LAYOUT")
        .parse()
        .expect("LAYOUT: неизвестная Раскладка");
    let corner: Corner = env!("START_CORNER")
        .parse()
        .expect("START_CORNER: неизвестный стартовый угол");
    let mapping = Mapping::new(MATRIX_W, MATRIX_H, layout, corner);

    info!(
        "Матрица {MATRIX_W}×{MATRIX_H}, Раскладка {layout:?}, стартовый угол {corner:?}; Лента на GPIO{LED_PIN}, потолок яркости {MAX_BRIGHTNESS}"
    );

    // Сеть — параллельно с Лентой, только с учётными данными из сборки;
    // её сбои не касаются рендера (задачи независимы).
    // Флеш: подтверждение Слота после OTA и два потребителя — память
    // Текущего эффекта (задача Ленты) и заливка обновления (HTTP-сервер).
    // SAFETY: второй экземпляр флеша алиасит первый. Операции флеша
    // синхронные и не пересекают точки ожидания (await), а исполнитель
    // однопоточный кооперативный — одновременного доступа нет; пишут
    // экземпляры только в непересекающиеся разделы (NVS и Слоты).
    let ota_flash = FlashStorage::new(unsafe { FLASH.clone_unchecked() });
    let mut flash = FlashStorage::new(FLASH);
    // Подтверждение сразу после старта: не дошедшая до сюда Прошивка
    // считается неудачной, загрузчик с auto-rollback откатится при ребуте.
    ota::confirm(&mut flash);
    if WIFI_CREDENTIALS_SET {
        // Семя стека — из аппаратного ГСЧ (канон примера esp-hal).
        let rng = Rng::new();
        let seed = (rng.random() as u64) << 32 | rng.random() as u64;
        net::start(&spawner, WIFI, seed, env!("CARGO_PKG_VERSION"), ota_flash);
    } else if WIFI_SSID.is_empty() != WIFI_PASS.is_empty() {
        warn!("Wi-Fi: задан только один из WIFI_SSID/WIFI_PASS — сеть отключена, Гирлянда офлайн");
    } else {
        info!("Wi-Fi: WIFI_SSID/WIFI_PASS не заданы при сборке — сеть отключена, Гирлянда офлайн");
    }

    // Жизненный цикл Ленты — отдельная задача со статическим фьючерсом,
    // стек main не несёт ни драйвера, ни буферов кадра, ни флеша.
    spawner.spawn(
        strip_task(RMT, led_pin, button_pin, flash, mapping).expect("задача Ленты не поднялась"),
    );

    loop {
        Timer::after(Duration::from_secs(IDLE_TICK_SECS)).await;
    }
}

// Хост-сборка: пустая заглушка, чтобы гейты workspace шли без железа.
#[cfg(not(target_arch = "riscv32"))]
fn main() {}
