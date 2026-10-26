//! Прошивка Гирлянды: первый свет на Стенде. Ядро — esp-hal/esp-rtos (async),
//! Лента — WS2812B через esp-hal-smartled (RMT-TX, порядок GRB).
//!
//! Жизненный цикл:
//!
//! 1. Тестовая сетка Раскладки (`TEST_GRID_SECS` секунд): белый бегунок идёт
//!    по Ленте и показывает порядок змейки, красный Пиксель 0 держит
//!    стартовый угол.
//! 2. Дальше — статичный цвет (`STATIC_COLOR`) с потолком яркости
//!    `MAX_BRIGHTNESS`; либо, если при сборке задан `DRIVER_GATE=1`,
//!    непрерывный blink/breathe — железный гейт драйвера на залипания и
//!    порчу цветов при частых обновлениях.
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
//! | `MAX_BRIGHTNESS`| потолок яркости канала, 0..=255                                   | `64`              |
//! | `TEST_GRID_SECS`| длительность тестовой сетки, секунд (0 — выключить)               | `8`               |
//! | `DRIVER_GATE`   | `1` — после сетки непрерывный blink/breathe вместо статичного     | `0`               |
//!
//! Сборка и заливка — из каталога `fw` (target и runner прописаны в
//! `.cargo/config.toml`):
//!
//! ```text
//! cargo build                          # Прошивка под Стенд 8×8
//! MATRIX_W=16 cargo build              # та же Прошивка под Матрицу 16×N
//! DRIVER_GATE=1 cargo run              # гейт драйвера: blink/breathe
//! cargo run                            # сборка и заливка через espflash
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
mod diag;

// Размер кучи, байт: дефолт шаблона esp-generate; запас под Wi-Fi появится
// вместе с esp-radio.
#[cfg(target_arch = "riscv32")]
const HEAP_SIZE: usize = 65536;

// Параметры из окружения сборки; мусор в конфиге ломает сборку, а не старт.
#[cfg(target_arch = "riscv32")]
const MATRIX_W: u16 = env_u16(env!("MATRIX_W"));
#[cfg(target_arch = "riscv32")]
const MATRIX_H: u16 = env_u16(env!("MATRIX_H"));
#[cfg(target_arch = "riscv32")]
const LED_PIN: u16 = env_u16(env!("LED_PIN"));
#[cfg(target_arch = "riscv32")]
const MAX_BRIGHTNESS: u8 = env_u8(env!("MAX_BRIGHTNESS"));
#[cfg(target_arch = "riscv32")]
const TEST_GRID_SECS: u16 = env_u16(env!("TEST_GRID_SECS"));
#[cfg(target_arch = "riscv32")]
const DRIVER_GATE: bool = env_bool(env!("DRIVER_GATE"));

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

/// GPIO по номеру из `LED_PIN`: типы периферии различаются по именам, поэтому
/// номер разворачивается в поле периферии, а пин стирается до [`AnyPin`].
#[cfg(target_arch = "riscv32")]
macro_rules! gpio_peripheral {
    ($p:ident, $number:expr) => {
        match $number {
            0 => $p.GPIO0.degrade(),
            1 => $p.GPIO1.degrade(),
            2 => $p.GPIO2.degrade(),
            3 => $p.GPIO3.degrade(),
            4 => $p.GPIO4.degrade(),
            5 => $p.GPIO5.degrade(),
            6 => $p.GPIO6.degrade(),
            7 => $p.GPIO7.degrade(),
            8 => $p.GPIO8.degrade(),
            9 => $p.GPIO9.degrade(),
            10 => $p.GPIO10.degrade(),
            11 => $p.GPIO11.degrade(),
            12 => $p.GPIO12.degrade(),
            13 => $p.GPIO13.degrade(),
            14 => $p.GPIO14.degrade(),
            15 => $p.GPIO15.degrade(),
            16 => $p.GPIO16.degrade(),
            17 => $p.GPIO17.degrade(),
            18 => $p.GPIO18.degrade(),
            19 => $p.GPIO19.degrade(),
            20 => $p.GPIO20.degrade(),
            21 => $p.GPIO21.degrade(),
            22 => $p.GPIO22.degrade(),
            23 => $p.GPIO23.degrade(),
            24 => $p.GPIO24.degrade(),
            25 => $p.GPIO25.degrade(),
            26 => $p.GPIO26.degrade(),
            27 => $p.GPIO27.degrade(),
            28 => $p.GPIO28.degrade(),
            29 => $p.GPIO29.degrade(),
            30 => $p.GPIO30.degrade(),
            _ => panic!("LED_PIN: на ESP32-C6 поддерживаются GPIO0..=30"),
        }
    };
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
use esp_hal::gpio::{AnyPin, Pin as _};
#[cfg(target_arch = "riscv32")]
use esp_hal::peripherals::RMT;
#[cfg(target_arch = "riscv32")]
use esp_hal::rmt::Rmt;
#[cfg(target_arch = "riscv32")]
use esp_hal::time::Rate;
#[cfg(target_arch = "riscv32")]
use esp_hal::timer::timg::TimerGroup;
#[cfg(target_arch = "riscv32")]
use esp_hal_smartled::{AdapterError, RmtSmartLeds, WS2812B_TIMING, buffer_size, color_order};
#[cfg(target_arch = "riscv32")]
extern crate alloc;
#[cfg(target_arch = "riscv32")]
use alloc::boxed::Box;
#[cfg(target_arch = "riscv32")]
use garland_core::{Corner, Frame, Layout, Mapping};
#[cfg(target_arch = "riscv32")]
use log::info;
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
    led.write(strip.iter().copied())
        .await
        .expect("запись Ленты не удалась");
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
/// статичный цвет. Отдельная embassy-задача: её фьючерс статический (bss),
/// стек `main` не несёт ни драйвера, ни буферов кадра.
// large_stack_frames меряет фьючерс задачи, а embassy выделяет его статически
// (bss), стек при работе не расходуется; ожидание (expect) этот поздний линт
// не связывает, поэтому прицельное разрешение (allow).
#[cfg(target_arch = "riscv32")]
#[allow(
    clippy::large_stack_frames,
    reason = "фьючерс embassy-задачи выделяется статически (bss), стек не расходуется"
)]
#[embassy_executor::task]
async fn strip_task(rmt_peripheral: RMT<'static>, led_pin: AnyPin<'static>, mapping: Mapping) {
    let mut led = init_strip_driver(rmt_peripheral, led_pin);
    let mut frame_buffer = [garland_core::Rgb8::BLACK; PIXELS];
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

    // Фаза 2: гейт драйвера или статичный цвет.
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

    info!("Статичный цвет");
    {
        let mut frame = next_frame!(frame_buffer);
        frame.fill(diag::STATIC_COLOR);
        show(led.as_mut(), &mut strip_buffer, &frame, &mapping).await;
    }
    loop {
        Timer::after(Duration::from_secs(IDLE_TICK_SECS)).await;
    }
}

/// Настоящая Прошивка — только под target riscv32imac-unknown-none-elf.
#[cfg(target_arch = "riscv32")]
#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    esp_println::logger::init_logger_from_env();

    let peripherals = esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::max()));

    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: HEAP_SIZE);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0, peripherals.FROM_CPU_INTR0);

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

    // Жизненный цикл Ленты — отдельная задача со статическим фьючерсом,
    // стек main не несёт ни драйвера, ни буферов кадра.
    spawner.spawn(
        strip_task(
            peripherals.RMT,
            gpio_peripheral!(peripherals, LED_PIN),
            mapping,
        )
        .expect("задача Ленты не поднялась"),
    );

    loop {
        Timer::after(Duration::from_secs(IDLE_TICK_SECS)).await;
    }
}

// Хост-сборка: пустая заглушка, чтобы гейты workspace шли без железа.
#[cfg(not(target_arch = "riscv32"))]
fn main() {}
