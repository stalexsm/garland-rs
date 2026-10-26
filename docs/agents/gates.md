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

Не ослабляй проверки, чтобы пройти гейт: без явного согласования не добавляй `#[allow(...)]`, `#[expect(...)]`, исключения в конфиги fmt/clippy.
