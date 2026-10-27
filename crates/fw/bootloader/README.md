# Загрузчик с автооткатом

`esp32c6-rollback.bin` — Загрузчик ESP-IDF для ESP32-C6, собранный с
`CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE=y`: если новая Прошивка не подтвердила
свой Слот (`ota::confirm`) до перезагрузки, Загрузчик помечает Слот невалидным
и запускает прежнюю Прошивку. Прекомпилированный Загрузчик espflash откат не
умеет, поэтому артефакт живёт в репо и ставится раннером при каждой заливке
(`--bootloader` в `.cargo/config.toml`).

## Как собран

По рецепту сборки прекомпилированных Загрузчиков espflash
(`espflash/resources/bootloaders/manifest.yaml`, вариант `esp32c6`) плюс одна
опция — откат:

- ESP-IDF `release/v6.1`, ref `9a97f6c54ec638111ce55cd36581b3c192f15207`
  (`v6.1-829-g9a97f6c54ec`); официальный docker-образ `espressif/idf:release-v6.1`
  стоит ровно на этом ref;
- минимальный проект как в xtask espflash: `CMakeLists.txt`
  (`include($ENV{IDF_PATH}/tools/cmake/project.cmake)`) и `main/main.c` с пустым
  `app_main`;
- `sdkconfig.defaults`:

  ```text
  # CONFIG_BOOTLOADER_COMPILE_TIME_DATE is not set
  CONFIG_ESPTOOLPY_FLASHSIZE_64MB=y
  CONFIG_ESPTOOLPY_FLASHSIZE="64MB"
  CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE=y
  ```

  Флеш «64 МБ» — как в манифесте espflash: esp-hal использует MMU-страницы
  64 КБ; реальные режим/размер/частоту флеша espflash вшивает в заголовок при
  каждой заливке, поэтому значение здесь ни на что не влияет;
- команды:

  ```text
  source /opt/esp/idf/export.sh
  idf.py -C <проект> -B <проект>/build -D SDKCONFIG_DEFAULTS=<проект>/sdkconfig.defaults set-target esp32c6
  idf.py -C <проект> -B <проект>/build bootloader
  ```

Артефакт — `build/bootloader/bootloader.bin`, отсюда он и копируется в репо.

## Как проверен

- Точная реплика без опции отката (тот же ref, тот же sdkconfig минус откат)
  совпала байт-в-байт с прекомпилированным `esp32c6-bootloader.bin` espflash
  (SHA-256 `caa4f498…`): рецепт и окружение воспроизводят официальный артефакт.
- В `sdkconfig` сборки: `CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE=y`;
  `CONFIG_BOOTLOADER_APP_ANTI_ROLLBACK` выключен (анти-откат блокирует Слоты
  через eFuse — Гирлянде не нужен).

SHA-256 артефакта: `821ea2cf3ebf867868e92eaa0b44ef8c3e358e9fc1ff26905d453c64839a30a9`.

## Как проверить на Стенде

1. `cargo run` из каталога `fw` — заливка с этим Загрузчиком; Прошивка
   подтверждает свой Слот при старте (`ota::confirm`).
2. Собрать образ Прошивки, зависающей до подтверждения: локально выключить
   подтверждение (временно не вызывать `ota::confirm`, правка не коммитится),
   затем `cargo build --release` и `espflash save-image --chip esp32c6
   target/riscv32imac-unknown-none-elf/release/fw hang.bin`.
3. Залить `hang.bin` через страницу Гирлянды: Прошивка запишет его в свободный
   Слот, активирует и перезагрузится — Стенд зависнет до подтверждения.
4. Перегрузить Стенд (reset или питание).
5. Ожидание: в мониторе видно, как Загрузчик помечает новый Слот невалидным и
   запускает прежний; на странице Гирлянды — прежняя версия Прошивки.
