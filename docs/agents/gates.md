# Gates

Обязательные проверки качества. Работа с кодом считается завершённой только когда все три прошли без ошибок.

```bash
cargo fmt --check
cargo clippy --all --all-targets -- -D warnings
cargo test --workspace
```

- `cargo fmt --check` — проверка форматирования (не правит код; чтобы исправить — `cargo fmt`).
- `cargo clippy --all --all-targets -- -D warnings` — линтер по всем таргетам, включая тесты; любое предупреждение считается ошибкой.
- `cargo test --workspace` — тесты всех крейтов workspace.

## Гейт под target прошивки

Под `riscv32imac-unknown-none-elf` флаг `--all-targets` неприменим: крейта `test` под таргетом нет. Линтер целевой сборки прошивки:

```bash
cd crates/fw && cargo clippy --bins -- -D warnings
```

`--bins` собирает и линтует бинарь прошивки; тесты прошивки — хост-тесты, их покрывает гейт выше.

Не ослабляй проверки, чтобы пройти гейт: без явного согласования не добавляй `#[allow(...)]`, `#[expect(...)]`, исключения в конфиги fmt/clippy.
