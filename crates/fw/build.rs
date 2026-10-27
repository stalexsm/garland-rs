fn main() {
    // Правка самого скрипта (например, дефолтов) перезапускает сборку —
    // без этого прошивка соберётся со старыми значениями до cargo clean.
    println!("cargo:rerun-if-changed=build.rs");
    build_config();
    // Линкерные скрипты esp-hal — только под target прошивки; на хосте они
    // ломают сборку обычным cc.
    if is_firmware_target() {
        linker_be_nice();
        // linkall.x должен быть последним линкер-скриптом (иначе проблемы с flip-link)
        println!("cargo:rustc-link-arg=-Tlinkall.x");
    }
}

fn is_firmware_target() -> bool {
    std::env::var("TARGET")
        .map(|target| target.contains("riscv32"))
        .unwrap_or(false)
}

// Параметры фиксируются на этапе сборки переменными окружения;
// дефолты — Стенд 8×8, змейка по горизонтальным рядам, старт в левом верхнем углу.
const DEFAULT_MATRIX_W: &str = "8";
const DEFAULT_MATRIX_H: &str = "8";
const DEFAULT_LAYOUT: &str = "serpentine-rows";
const DEFAULT_START_CORNER: &str = "top-left";
const DEFAULT_LED_PIN: &str = "10";
const DEFAULT_BUTTON_PIN: &str = "9";
const DEFAULT_MAX_BRIGHTNESS: &str = "64";
const DEFAULT_TEST_GRID_SECS: &str = "8";
const DEFAULT_DRIVER_GATE: &str = "0";

// Учётные данные Wi-Fi — только при сборке; пустые значения (дефолт)
// означают офлайн-режим: прошивка собирается и работает без сети.
const DEFAULT_WIFI_SSID: &str = "";
const DEFAULT_WIFI_PASS: &str = "";

fn build_config() {
    for (name, default) in [
        ("MATRIX_W", DEFAULT_MATRIX_W),
        ("MATRIX_H", DEFAULT_MATRIX_H),
        ("LAYOUT", DEFAULT_LAYOUT),
        ("START_CORNER", DEFAULT_START_CORNER),
        ("LED_PIN", DEFAULT_LED_PIN),
        ("BUTTON_PIN", DEFAULT_BUTTON_PIN),
        ("MAX_BRIGHTNESS", DEFAULT_MAX_BRIGHTNESS),
        ("TEST_GRID_SECS", DEFAULT_TEST_GRID_SECS),
        ("DRIVER_GATE", DEFAULT_DRIVER_GATE),
        ("WIFI_SSID", DEFAULT_WIFI_SSID),
        ("WIFI_PASS", DEFAULT_WIFI_PASS),
    ] {
        let value = std::env::var(name).unwrap_or_else(|_| default.to_string());
        println!("cargo:rustc-env={name}={value}");
        println!("cargo:rerun-if-env-changed={name}");
    }
}

fn linker_be_nice() {
    let args: Vec<String> = std::env::args().collect();
    // Скрипту нужны и вид ошибки, и символ: вызов с меньшим числом аргументов
    // обработке не подлежит.
    if args.len() > 2 {
        let kind = &args[1];
        let what = &args[2];

        match kind.as_str() {
            "undefined-symbol" => match what.as_str() {
                what if what.starts_with("_defmt_") => {
                    eprintln!();
                    eprintln!(
                        "💡 `defmt` not found - make sure `defmt.x` is added as a linker script and you have included `use defmt_rtt as _;`"
                    );
                    eprintln!();
                }
                "_stack_start" => {
                    eprintln!();
                    eprintln!("💡 Is the linker script `linkall.x` missing?");
                    eprintln!();
                }
                what if what.starts_with("esp_rtos_") => {
                    eprintln!();
                    eprintln!(
                        "💡 `esp-radio` has no scheduler enabled. Make sure you have initialized `esp-rtos` or provided an external scheduler."
                    );
                    eprintln!();
                }
                "embedded_test_linker_file_not_added_to_rustflags" => {
                    eprintln!();
                    eprintln!(
                        "💡 `embedded-test` not found - make sure `embedded-test.x` is added as a linker script for tests"
                    );
                    eprintln!();
                }
                "free"
                | "malloc"
                | "calloc"
                | "get_free_internal_heap_size"
                | "malloc_internal"
                | "realloc_internal"
                | "calloc_internal"
                | "free_internal" => {
                    eprintln!();
                    eprintln!(
                        "💡 Did you forget the `esp-alloc` dependency or didn't enable the `compat` feature on it?"
                    );
                    eprintln!();
                }
                _ => (),
            },
            // we don't have anything helpful for "missing-lib" yet
            _ => {
                std::process::exit(1);
            }
        }

        std::process::exit(0);
    }

    println!(
        "cargo:rustc-link-arg=--error-handling-script={}",
        std::env::current_exe().unwrap().display()
    );
}
