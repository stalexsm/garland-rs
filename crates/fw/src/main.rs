//! Заготовка Прошивки Гирлянды: каркас на esp-hal/esp-rtos, собирается под
//! target `riscv32imac-unknown-none-elf` и печатает параметры Матрицы.
//!
//! Параметры Матрицы задаются переменными окружения при сборке (см. `build.rs`),
//! дефолты — Стенд 8×8:
//!
//! | Переменная     | Значения                                                              | Дефолт             |
//! |----------------|-----------------------------------------------------------------------|--------------------|
//! | `MATRIX_W`     | ширина Матрицы в Пикселях                                             | `8`                |
//! | `MATRIX_H`     | высота Матрицы в Пикселях                                             | `8`                |
//! | `LAYOUT`       | `serpentine-rows`, `plain-rows`, `serpentine-cols`, `plain-cols`      | `serpentine-rows`  |
//! | `START_CORNER` | `top-left`, `top-right`, `bottom-left`, `bottom-right`                | `top-left`         |
//!
//! Сборка и заливка — из каталога `fw` (target и runner прописаны в
//! `.cargo/config.toml`):
//!
//! ```text
//! cargo build                # Прошивка под Стенд 8×8
//! MATRIX_W=16 cargo build    # та же Прошивка под Матрицу 16×N
//! cargo run                  # сборка и заливка через espflash
//! ```
//!
//! На хост-таргетах крейт собирается пустой заглушкой — только чтобы гейты
//! workspace (`cargo fmt/clippy/test`) шли без железа; настоящий код прошивки
//! компилируется только под `riscv32`.
#![cfg_attr(target_arch = "riscv32", no_std)]
#![cfg_attr(target_arch = "riscv32", no_main)]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

// Размер кучи, байт: дефолт шаблона esp-generate; запас под Wi-Fi появится
// вместе с esp-radio.
#[cfg(target_arch = "riscv32")]
const HEAP_SIZE: usize = 65536;

// Период такта заготовки, секунд: пока нет ленты и эффектов, цикл просто спит.
#[cfg(target_arch = "riscv32")]
const STUB_TICK_SECS: u64 = 1;

#[cfg(target_arch = "riscv32")]
use embassy_executor::Spawner;
#[cfg(target_arch = "riscv32")]
use embassy_time::{Duration, Timer};
#[cfg(target_arch = "riscv32")]
use esp_backtrace as _;
#[cfg(target_arch = "riscv32")]
use esp_hal::clock::CpuClock;
#[cfg(target_arch = "riscv32")]
use esp_hal::timer::timg::TimerGroup;
#[cfg(target_arch = "riscv32")]
use garland_core::{Corner, Layout, Mapping};
#[cfg(target_arch = "riscv32")]
use log::info;

// Дескриптор приложения, обязательный для загрузчика esp-idf.
#[cfg(target_arch = "riscv32")]
esp_bootloader_esp_idf::esp_app_desc!();

// Настоящая Прошивка — только под target riscv32imac-unknown-none-elf.
#[cfg(target_arch = "riscv32")]
#[esp_rtos::main]
async fn main(_spawner: Spawner) -> ! {
    esp_println::logger::init_logger_from_env();

    let peripherals = esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::max()));

    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: HEAP_SIZE);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0, peripherals.FROM_CPU_INTR0);

    // Привязка координат — из параметров сборки; кривой конфиг падает на старте.
    let matrix_w: u16 = env!("MATRIX_W").parse().expect("MATRIX_W: ожидалось число");
    let matrix_h: u16 = env!("MATRIX_H").parse().expect("MATRIX_H: ожидалось число");
    let layout: Layout = env!("LAYOUT")
        .parse()
        .expect("LAYOUT: неизвестная Раскладка");
    let corner: Corner = env!("START_CORNER")
        .parse()
        .expect("START_CORNER: неизвестный стартовый угол");
    let mapping = Mapping::new(matrix_w, matrix_h, layout, corner);

    info!(
        "Матрица {matrix_w}×{matrix_h}, Раскладка {layout:?}, стартовый угол {corner:?}; Пикселей на Ленте: {}",
        mapping.pixel_count()
    );

    loop {
        Timer::after(Duration::from_secs(STUB_TICK_SECS)).await;
    }
}

// Хост-сборка: пустая заглушка, чтобы гейты workspace шли без железа.
#[cfg(not(target_arch = "riscv32"))]
fn main() {}
