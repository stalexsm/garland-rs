# Архитектура LED-эффектов в no_std Rust (исследование) — матрица на балконном окне

Дата: 2026-10-26 · Репозиторий: garland-rs (ESP32-C6)
Исходное требование владельца: эффекты — это «addons», их будут постоянно добавлять; каждый эффект планируется отдельным крейтом workspace с общим трейтом.
Уточнение владельца: лента смонтирована **матрицей** (ширина × высота, сетка из горизонтальных рядов) — трейт и слой рендера проектируются под 2D-фреймбуфер, маппинг (x, y) → индекс физического диода — отдельный слой прошивки.

Ссылки на WLED даны на ветку `main` (файл/строка, снято 2026-10-26), на Rust-крейты — docs.rs / crates.io (исходники сверены по тарболам crates.io).

---

## 1. Резюме

- В WLED эффект — глобальная C-функция `void(void)`, пишущая через глобальный контекст (`SEGMENT`/`SEGENV`); состояние эффекта живёт в mutable-полях сегмента (`step/call/aux0/aux1/data`), а не в объекте эффекта.
- WLED устроен матрично-первоклассно: эффекты адресуют пиксели по координатам (`setPixelColorXY(x, y, …)`), 2D-эффекты (Plasma Ball, Firenoise, Drift, …) — обычные режимы реестра с флагом `2` в метаданных; на 1D-сегменте тот же вызов вырождается в линейный `setPixelColor(x, …)`.
- Кроссфейд WLED: копия состояния, каждый кадр перехода выполняются **оба** эффекта, выводы смешиваются поканально с 16-битной альфой; при нехватке RAM — деградация до чистого fade.
- Маппинг (x, y) → физический диод — отдельный слой (в WLED — `customMappingTable`/ledmap; в FastLED — каноническая функция `XY()` с конфигурациями serpentine/vertical). Это классическая точка ошибок монтажа: она должна быть **конфигурацией прошивки, а не кодом эффектов**.
- Для проекта: трейт `Effect` с `render(&mut self, frame: &mut Frame<'_>, t_ms: u32, params)`; `Frame` — 2D-вид (W, H + срез `RGB8`), эффекты работают в координатах; 1D-эффекты пишутся через «path-вид» кадра (обход провода змейкой) — без дублирования кода. Реестр — статический срез фабрик `fn(Params) -> Box<dyn Effect>`; кроссфейд — два фреймбуфера (3 КБ на 500 ячеек, ~0,6% SRAM ESP32-C6).
- Крейты эффектов — `no_std` без esp-hal, математика тестируется `cargo test` на хосте; маппинг — чистая функция с юнит-тестами на известные раскладки.

---

## 2. WLED: референсная реализация (Aircoookie/WLED, main)

### 2.1 Реестр эффектов и метаданные

- Тип функции эффекта: `typedef void (*mode_ptr)();` — эффект не принимает аргументов, всё через глобальный контекст ([wled00/FX.h, L851](https://github.com/Aircoookie/WLED/blob/main/wled00/FX.h#L851)).
- Запись реестра — `ModeData { _id, _fcn, _data }`, где `_data` — строка метаданных в PROGMEM: имя, слайдеры, цветовые слоты, палитра, флаги UI ([FX.h, L852–858](https://github.com/Aircoookie/WLED/blob/main/wled00/FX.h#L852)).
- Хранилище — `std::vector<mode_ptr> _mode` и `std::vector<const char*> _modeData` ([FX.h, L1076–1077](https://github.com/Aircoookie/WLED/blob/main/wled00/FX.h#L1076)); наполнение в `setupEffectData()`: «Solid» первым, затем `addEffect(id, fn, name)` ([FX.cpp, L11056–11068](https://github.com/Aircoookie/WLED/blob/main/wled00/FX.cpp#L11056)). Всего `MODE_COUNT = 221` ([FX.h, L379](https://github.com/Aircoookie/WLED/blob/main/wled00/FX.h#L379)).
- Пример метаданных: `"Blink@!,Duty cycle;!,!;!;01"` ([FX.cpp, L227](https://github.com/Aircoookie/WLED/blob/main/wled00/FX.cpp#L227)); для 2D-эффектов в флагах ставится `2`, например `"Plasma Ball@Speed,,Fade,Blur;;!;2"` ([FX.cpp, L5944](https://github.com/Aircoookie/WLED/blob/main/wled00/FX.cpp#L5944)), регистрация обычная: `addEffect(FX_MODE_2DPLASMABALL, &mode_2DPlasmaball, …)` ([FX.cpp, L11262](https://github.com/Aircoookie/WLED/blob/main/wled00/FX.cpp#L11262)).

Вывод: WLED решает «эффекты как addons» списком указателей функций + строкой метаданных; 2D-эффекты при этом — не отдельная подсистема, а обычные записи реестра.

### 2.2 Модель эффекта: глобальный контекст и состояние

- Контекст подставляется макросами: `#define SEGMENT (*strip._currentSegment)`, `#define SEGENV (*strip._currentSegment)` ([FX.h, L108–109](https://github.com/Aircoookie/WLED/blob/main/wled00/FX.h#L108)); указатель выставляется в service-цикле ([FX_fcn.cpp, L1474](https://github.com/Aircoookie/WLED/blob/main/wled00/FX_fcn.cpp#L1474)).
- Состояние эффекта — mutable-поля сегмента: `step` (uint32), `call` (счётчик кадров), `aux0`/`aux1` (uint16) и `data` — указатель на heap-скретч ([FX.h, L488–492](https://github.com/Aircoookie/WLED/blob/main/wled00/FX.h#L488)). Пример: blink хранит номер итерации в `SEGENV.step` ([FX.cpp, L200–214](https://github.com/Aircoookie/WLED/blob/main/wled00/FX.cpp#L200)).
- Heap-скретч: `Segment::allocateData()` с глобальным лимитом `MAX_SEGMENT_DATA` при отсутствии PSRAM ([FX_fcn.cpp, L159–177](https://github.com/Aircoookie/WLED/blob/main/wled00/FX_fcn.cpp#L159)).
- Каждый вызов эффекта обязан перерисовать **весь** сегмент/матрицу (blink проходит весь `SEGLEN`, [FX.cpp, L212–215](https://github.com/Aircoookie/WLED/blob/main/wled00/FX.cpp#L212)).

### 2.3 Параметры и тайминги

- Параметры — фиксированный набор полей сегмента: `speed`, `intensity`, `custom1`, `custom2`, `custom3` (5 бит), `check1..3` ([FX.h, L475–482](https://github.com/Aircoookie/WLED/blob/main/wled00/FX.h#L475)).
- Длительность перехода по умолчанию — 750 мс (`_transitionDur(750)`, [FX.h, L879](https://github.com/Aircoookie/WLED/blob/main/wled00/FX.h#L879)); общая тайм-база — `now = nowUp + timebase` ([FX_fcn.cpp, L1450](https://github.com/Aircoookie/WLED/blob/main/wled00/FX_fcn.cpp#L1450)); кадр ограничен `_frametime` и `MIN_FRAME_DELAY` ([FX_fcn.cpp, L1446–1450](https://github.com/Aircoookie/WLED/blob/main/wled00/FX_fcn.cpp#L1446)).

### 2.4 Полезная цветовая математика

- `color_blend(c1, c2, blend)` — поканальное смешение без ветвлений («poorman's SIMD», два канала за операцию) ([colors.cpp, L20–34](https://github.com/Aircoookie/WLED/blob/main/wled00/colors.cpp#L20)).
- Гамма — таблица 256×u8, вычисляется на старте для произвольной гаммы ([colors.cpp, L653–675](https://github.com/Aircoookie/WLED/blob/main/wled00/colors.cpp#L653)); применяется **на выходе**, при покраске шины ([FX_fcn.cpp, L1898–1908](https://github.com/Aircoookie/WLED/blob/main/wled00/FX_fcn.cpp#L1898)).
- Быстрые синусы без float: `sin8_t`/`sin16_t` (16-битная фаза) ([wled_math.cpp, L65–84](https://github.com/Aircoookie/WLED/blob/main/wled00/wled_math.cpp#L65)).

### 2.5 Матрица как первоклассный случай: (x, y)-доступ и размеры

- Сегмент хранит пиксели **линейно**, 2D-доступ — арифметика над этим массивом: `setPixelColorXYRaw(x, y) = pixels[x + y * vWidth()]` ([FX.h, L569–570](https://github.com/Aircoookie/WLED/blob/main/wled00/FX.h#L569)).
- Эффекты пишут через API координат: `Segment::setPixelColorXY(int x, int y, c)` ([FX.h, L774–777](https://github.com/Aircoookie/WLED/blob/main/wled00/FX.h#L774)); на **1D-сегменте** тот же вызов вырождается в линейный `setPixelColor(x, c)` — y игнорируется ([FX.h, L811–814](https://github.com/Aircoookie/WLED/blob/main/wled00/FX.h#L811)). Один и тот же интерфейс закрывает 1D и 2D.
- Размеры на время вызова эффекта: статические `_vWidth/_vHeight`, выставляются `setDrawDimensions()` перед каждым прогоном ([FX.h, L691](https://github.com/Aircoookie/WLED/blob/main/wled00/FX.h#L691), [L513](https://github.com/Aircoookie/WLED/blob/main/wled00/FX.h#L513)).
- 2D-эффекты — полноценные граждане: тело эффекта пишет по координатам, например Plasma Ball: цикл по `rows/cols`, кэш шума на строку, `SEGMENT.addPixelColorXY(i, j, …)` ([FX.cpp, функция `mode_2DPlasmaball`](https://github.com/Aircoookie/WLED/blob/main/wled00/FX.cpp)); если запуск не на матрице — `FX_FALLBACK_STATIC` (эффект сам деградирует).
- Физическая проводка поверх логики — отдельный слой: `getMappedPixelIndex()` прогоняет логический индекс через `customMappingTable` (ledmap), включая 2D ([FX.h, L1019–1021](https://github.com/Aircoookie/WLED/blob/main/wled00/FX.h#L1019)).

### 2.6 Канонический маппинг матрицы: FastLED XYMatrix

Канонический пример FastLED целиком посвящён именно этой проблеме: [examples/XYMatrix/XYMatrix.ino, L100–134](https://github.com/FastLED/FastLED/blob/master/examples/XYMatrix/XYMatrix.ino#L100) — функция `XY(x, y)` с двумя флагами конфигурации:

- `kMatrixSerpentineLayout == false`, вертикаль — обычный порядок: `i = y * kMatrixWidth + x`; горизонтальная проводка по столбцам — `i = kMatrixHeight * (kMatrixWidth - (x+1)) + y` (L104–110);
- serpentine по **рядам**: нечётный ряд разворачивается — `reverseX = (kMatrixWidth - 1) - x`, `i = y * kMatrixWidth + reverseX` (L115–119);
- serpentine по **столбцам** (`kMatrixVertical`): нечётный столбец разворачивается по y (L120–129).

Файл прямо предупреждает: сначала добейся корректной раскладки, потом рисуй («Once you've gotten the basics working (AND NOT UNTIL THEN!)», [XYMatrix.ino, L135+](https://github.com/FastLED/FastLED/blob/master/examples/XYMatrix/XYMatrix.ino#L135)) — ошибки змейки визуально ломают любую картинку.

---

## 3. Переходы между эффектами (crossfade) в WLED

Основной источник — [FX_fcn.cpp, L321–359](https://github.com/Aircoookie/WLED/blob/main/wled00/FX_fcn.cpp#L321):

1. У перехода два канала: **fade** (кроссфейд цветов/палитры/CCT/opacity, копия не нужна) и **spatial** (wipe/push — требует копии). Для смены эффекта по умолчанию (`TRANSITION_FADE`, [FX.h, L382](https://github.com/Aircoookie/WLED/blob/main/wled00/FX.h#L382)) копия не нужна.
2. Для spatial-переходов создаётся полная копия сегмента `new Segment(*this)` — с пиксельным буфером ([FX_fcn.cpp, L352](https://github.com/Aircoookie/WLED/blob/main/wled00/FX_fcn.cpp#L352)); буфер сегмента — `uint32_t *pixels`, 4 байта на ячейку ([FX.h, L497](https://github.com/Aircoookie/WLED/blob/main/wled00/FX.h#L497)).
3. Прогресс — 16-битный: `diff * 0xFFFF / dur`, пересчитывается каждый кадр ([FX_fcn.cpp, L468–477](https://github.com/Aircoookie/WLED/blob/main/wled00/FX_fcn.cpp#L468)).
4. Во время перехода выполняются **оба** эффекта — новый и старый в копию ([FX_fcn.cpp, L1471–1488](https://github.com/Aircoookie/WLED/blob/main/wled00/FX_fcn.cpp#L1471)); `show()` сводит сегменты в фреймбуфер через `blendSegment` ([FX_fcn.cpp, L1863–1886](https://github.com/Aircoookie/WLED/blob/main/wled00/FX_fcn.cpp#L1863)).
5. Смешение — `color_blend` с прогрессом как альфой; при включённой гамме opacity проходит `gamma8inv` ([FX_fcn.cpp, L1587–1588](https://github.com/Aircoookie/WLED/blob/main/wled00/FX_fcn.cpp#L1587)).
6. При нехватке RAM под копию — деградация до чистого fade ([FX_fcn.cpp, L375–378](https://github.com/Aircoookie/WLED/blob/main/wled00/FX_fcn.cpp#L375)).

### Цена памяти (матрица 100–500 ячеек)

| Ячеек (пример) | 1 буфер RGB8 (3 Б/яч.) | 2 буфера RGB8 (кроссфейд) | 2 буфера uint32 (как WLED) |
|---|---:|---:|---:|
| 100 (10×10)    | 300 Б  | 600 Б  | 800 Б |
| 256 (16×16)    | 768 Б  | 1,5 КБ | 2 КБ  |
| 500 (25×20)    | 1,5 КБ | 3,0 КБ | 4,0 КБ |

ESP32-C6 имеет 512 КБ SRAM ([Espressif, продуктная страница](https://www.espressif.com/en/products/socs/esp32-c6) — «a 512KB SRAM»). Два полных буфера RGB8 на 500 ячеек — 3 КБ ≈ 0,6%; два uint32 — 4 КБ ≈ 0,8%. Память кроссфейда не ограничение.

---

## 4. Rust-экосистема LED-эффектов

### 4.1 smart-leds / smart-leds-trait (транспорт кадра, линейный)

- `SmartLedsWrite::write<T, I>(&mut self, iterator: T) where T: IntoIterator<Item = I>, I: Into<Self::Color>` — весь API: один итератор пикселей за кадр ([docs.rs, smart-leds-trait 0.3.2](https://docs.rs/smart-leds-trait/0.3.2/smart_leds_trait/trait.SmartLedsWrite.html), src/lib.rs:43–54; async-вариант — L56–75).
- Пиксель — `RGB8` из крейта `rgb`, реэкспорт ([src/lib.rs, L15](https://docs.rs/smart-leds-trait/0.3.2/smart_leds_trait/index.html); [docs.rs rgb::RGB8](https://docs.rs/rgb/latest/rgb/struct.RGB8.html)).
- Утилиты: `hsv2rgb(Hsv)` ([docs.rs](https://docs.rs/smart-leds/0.4.0/smart_leds/hsv/fn.hsv2rgb.html), src/hsv.rs:23), `gamma()` — таблица `GAMMA8: [u8; 256]` ([docs.rs](https://docs.rs/smart-leds/0.4.0/smart_leds/fn.gamma.html), src/lib.rs:60–101), `brightness()` ([docs.rs](https://docs.rs/smart-leds/0.4.0/smart_leds/fn.brightness.html), src/lib.rs:17–48).
- Понятия «эффект» и «матрица» в smart-leds нет: это транспорт линейной цепочки (порядок = порядок проводки). Значит **маппинг матрицы — наша ответственность в прошивке**, до вызова `write`. Пример цикла: итератор → `ws2812.write(pixels)` → `sleep` ([examples/m5atom_smart_leds.rs](https://github.com/cat-in-136/ws2812-esp32-rmt-driver/blob/main/examples/m5atom_smart_leds.rs)).

### 4.2 Библиотеки эффектов

- **smart_led_effects** — трейт `EffectIterator { fn name(&self) -> &'static str; fn next(&mut self) -> Option<Vec<Srgb<u8>>> }` ([src/strip/effects_trait.rs, L3–6](https://docs.rs/smart_led_effects/latest/smart_led_effects/strip/effects_trait/trait.EffectIterator.html)): аллокация `Vec` на кадр, std-ориентированный, только 1D. Непригоден как модель, полезен как антипример.
- **smart-leds-fx** — портированные эффекты WLED, `#![no_std]` (src/lib.rs:1). Сигнатура: `step<P: Pixel>(self, pixels: &mut [P], state: &mut EffectState, params: &Params)` ([src/effect.rs, L394–430](https://docs.rs/smart-leds-fx/latest/smart_leds_fx/effect/enum.Effect.html)); состояние — `EffectState { counter: u32, aux: u32 }` ([src/segment.rs, L5–8](https://docs.rs/smart-leds-fx/latest/smart_leds_fx/segment/struct.EffectState.html)) — WLED-паттерн в Rust. Минус: эффект — **закрытый** `#[non_exhaustive] enum` («New effects may be added in minor releases», [src/effect.rs, L20–24](https://docs.rs/smart-leds-fx/latest/smart_leds_fx/effect/enum.Effect.html)) — addon-крейты не могут добавить вариант; 2D-эффектов нет.
- **photonic** — декларативный фреймворк (атрибуты/узлы/boxing, [docs.rs/photonic](https://docs.rs/photonic/latest/photonic/boxed/index.html)): избыточен для минимального интерфейса.
- **pennant** — статусные LED, трейт `StatusLed` ([src/lib.rs, L120](https://docs.rs/pennant/latest/pennant/trait.StatusLed.html)); не про сцены.

Вывод: готового no_std-движка «открытый набор эффектов + 2D-кадр» нет; проектируем сами, ближайший референс сигнатуры — smart-leds-fx, референс 2D-модели — WLED.

---

## 5. Дизайн для матрицы: кадр, маппинг, 1D-поверх-2D

### 5.1 Frame: 2D-фреймбуфер

Кадр — сплошной срез `RGB8` длиной `W*H` плюс размеры:

```rust
pub struct Frame<'a> {
    pub w: u16,
    pub h: u16,
    pixels: &'a mut [RGB8],   // строка-мажор, БЕЗ змейки (логический порядок)
}
impl Frame<'_> {
    pub fn set(&mut self, x: u16, y: u16, c: RGB8);   // границы проверяет
    pub fn get(&self, x: u16, y: u16) -> RGB8;
    pub fn fill(&mut self, c: RGB8);
    pub fn row(&mut self, y: u16) -> &mut [RGB8];     // горизонтальная полоса
    pub fn col(&mut self, x: u16) -> impl Iterator;   // вертикальная полоса (по строкам)
    pub fn path(&self) -> PathView<'_>;               // обход «по проводу», см. 5.3
}
```

Это модель WLED, перенесённая честно: пиксели линейны, координаты — арифметика над срезом (`FX.h, L569`), размеры — часть контекста кадра (`FX.h, L691`). Отличие: у нас `w/h` — поля кадра, а не глобальные статики, поэтому 2D-вид тестируем на хосте.

### 5.2 Слой маппинга (x, y) → физический индекс — в прошивке, не в эффектах

Классическая точка ошибок монтажа (змейка/направление/угол старта). Правила:

1. Эффекты видят **чистую логическую сетку** (x, y), без знания проводки — так же, как в WLED эффекты не знают про ledmap (`customMappingTable` применяется при выводе, [FX.h, L1019–1021](https://github.com/Aircoookie/WLED/blob/main/wled00/FX.h#L1019)).
2. Маппинг — чистая функция в крейте прошивки с конфигурацией: `Mapping { w, h, serpentine, vertical, start_corner }` — ровно те четыре случая, что канонизированы в FastLED `XY()` ([XYMatrix.ino, L100–134](https://github.com/FastLED/FastLED/blob/master/examples/XYMatrix/XYMatrix.ino#L100)); змейка по рядам — самая частая проводка для окна (ряды горизонтальны).
3. Выход движка: логический кадр `w*h` → применить маппинг → линейный буфер `write()` в smart-leds. Гамма и яркость — тоже здесь, на выходе (§6.4).

Маппинг — чистая функция ⇒ юнит-тесты на хосте: для каждой конфигурации проверяем обход пути (0,1,2,… по проводу), углы, паритет рядов. Это ровно тест, который ловит «классическую ошибку змейки» до монтажа.

### 5.3 1D-эффекты на матрице без дублирования кода

WLED-подход: единый API `setPixelColorXY`, на 1D вырождающийся в линейный ([FX.h, L811](https://github.com/Aircoookie/WLED/blob/main/wled00/FX.h#L811)); 1D-эффекты не знают про матрицу. У нас два представления в одном `Frame`:

- **Path-вид** (`path()`): линейный обход «по проводу» — последовательность (x, y), порождённая тем же `Mapping` (змейка). Эффект «бегущая радуга» пишет в `path()[i]` и физически бежит по проводу — код 1D-эффекта нулём отличается от версии для ленты; дублирования нет, потому что path — свойство кадра, а не эффект.
- **Виды row/col** для полосовых эффектов (развернуть радугу вдоль оси окна с повторением на каждый ряд — « Axis-режим»): `row(y)`/`col(x)` дают срезы, эффект выбирает ось параметром.

Минимальный контракт: эффект получает `Frame` с `w/h`; 1D-эффекты пользуются `path()` (физический порядок), 2D — координатами; полосовые — `row/col`. Никаких отдельных «1D-крейтов».

### 5.4 Трейт Effect: сигнатура

```rust
pub trait Effect {
    fn name(&self) -> &'static str;
    /// Отрисовать один кадр матрицы. `t_ms` — монотонное время (мс, wrapping u32).
    /// Эффект обязан заполнить кадр целиком (контракт WLED, FX.cpp L212–215).
    fn render(&mut self, frame: &mut Frame<'_>, t_ms: u32, params: Params);
}
```

- Время — параметр, а не глобаль (в WLED общая тайм-база `strip.now`, [FX_fcn.cpp, L1450](https://github.com/Aircoookie/WLED/blob/main/wled00/FX_fcn.cpp#L1450)); smart-leds-fx поступает так же (`Params::now_ms`, [src/effect.rs, L404–412](https://docs.rs/smart-leds-fx/latest/smart_leds_fx/effect/enum.Effect.html)).
- Состояние — в `self` (обоснование против `aux0/aux1`: §4.2 — в Rust с трейтом состояние естественно живёт с объектом).
- Пиксель — `RGB8` из крейта `rgb`: кадр уходит в `SmartLedsWrite::write` без конвертации.
- 2D-эффект на «не-матрице» — не проблема проекта (матрица всегда), но контракт кадра с `w/h` позволяет эффекте деградировать самому, как `FX_FALLBACK_STATIC` в WLED.

### 5.5 Аллокация

- `Box<dyn Effect>` через глобальный аллокатор esp-alloc (`HEAP: EspHeap`, [docs.rs/esp-alloc](https://docs.rs/esp-alloc/latest/esp_alloc/index.html), src/lib.rs:170; макрос `heap_allocator!`, src/macros.rs:14; реализация `Allocator`/`GlobalAlloc` — src/allocators.rs:25–40).
- Аллокация только при переключении эффекта (одна — смена, две — при кроссфейде), не per-frame. Статические буферы кадра — в прошивке: `[RGB8; W*H]` (два на кроссфейд).
- Heap-скретч самого эффекта — его собственные `Box`/`Vec` поля (кэш шума, как в Plasma Ball — `noisebuf` на строку), ограничение по вкусу автора эффекта; лимит как в WLED (`MAX_SEGMENT_DATA`, [FX_fcn.cpp, L159–177](https://github.com/Aircoookie/WLED/blob/main/wled00/FX_fcn.cpp#L159)) здесь не нужен при 512 КБ SRAM.

### 5.6 Реестр

```rust
pub type Factory = fn(Params) -> Box<dyn Effect>;
pub static REGISTRY: &[(&str, Factory)] = &[
    ("rain",   effect_rain::create),
    ("plasma", effect_plasma::create),
];
```

- Аналог `ModeData`/`addEffect` WLED ([FX.h, L852–858](https://github.com/Aircoookie/WLED/blob/main/wled00/FX.h#L852), [FX.cpp, L11056+](https://github.com/Aircoookie/WLED/blob/main/wled00/FX.cpp#L11056)); добавление эффекта-крейта = зависимость + одна строка. Функции-указатели — ноль рантайм-инициализации, реестр в `.rodata`.
- Метаданные параметров (аналог строки `"Plasma Ball@Speed,,Fade,Blur;;!;2"`) не нужны, пока нет UI настроек — см. открытые вопросы.

### 5.7 Параметры и персистентность (набросок, не реализовывать)

- База — `speed: u8`, `intensity: u8` (как в WLED, [FX.h, L475–476](https://github.com/Aircoookie/WLED/blob/main/wled00/FX.h#L475)).
- Персистентность: `{magic, version, effect_id, speed, intensity, crc}` во flash через `esp-storage` ([docs.rs](https://docs.rs/esp-storage/latest/esp_storage/), «Access ESP32 flash») поверх трейтов `embedded-storage` ([docs.rs](https://docs.rs/embedded-storage/latest/embedded_storage/)); чтение при старте, запись с дебаунсом, чередование двух секторов. Отдельная задача.

---

## 6. Тестируемость на хосте

Правило: **крейты эффектов и маппинг не знают про esp-hal**.

- Крейт эффекта: `#![no_std]` + `extern crate alloc`; зависимости — ядро (Frame/trait/blend), `rgb`/`smart-leds`, при нужде `libm` (no_std-математика, [docs.rs/libm](https://docs.rs/libm/latest/libm/); прецедент хост-тестов в no_std-крейте — `#[cfg(test)]`-модули libm, src/math/atan.rs:133).
- Цветовая математика (HSV→RGB, гамма-таблица, blend) — чистые функции ядра: референс `hsv2rgb`/`GAMMA8` в smart-leds; юнит-тесты краёв 0/255, монотонности.
- **Маппинг — обязательный юнит-тест**: для всех 4 конфигураций FastLED-набора (row/col × serpentine/plain, ± угол старта) проверяется полный обход пути и возврат инверсии `path_index(x,y)` ↔ `(x,y)(i)`.
- Детерминизм: генератор шума передаётся/хранится seeded — кадр воспроизводим в тесте.
- Гамма — на выходе движка, не в эффектах (как WLED `show()`, [FX_fcn.cpp, L1898–1908](https://github.com/Aircoookie/WLED/blob/main/wled00/FX_fcn.cpp#L1898)); яркость кроссфейда компенсировать `gamma8inv`-эквивалентом ([FX_fcn.cpp, L1587–1588](https://github.com/Aircoookie/WLED/blob/main/wled00/FX_fcn.cpp#L1587)).

Критерий: `cargo test --workspace` на макбуке проверяет эффекты, маппинг, кроссфейд; на железе гоняется только прошивка.

---

## 7. Черновик интерфейса (минимальный)

```rust
// крейт garland-effect-core: no_std, без esp-hal
extern crate alloc;
use alloc::boxed::Box;
use smart_leds::RGB8; // тип пикселя из крейта rgb

/// Общие параметры эффекта (аналог speed/intensity WLED, FX.h:475–476).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Params { pub speed: u8, pub intensity: u8 }

/// Логический 2D-кадр: строка-мажор, БЕЗ змейки (змейка — слой прошивки).
pub struct Frame<'a> {
    pub w: u16,
    pub h: u16,
    pixels: &'a mut [RGB8],
}
impl<'a> Frame<'a> {
    pub fn set(&mut self, x: u16, y: u16, c: RGB8);      // с проверкой границ
    pub fn get(&self, x: u16, y: u16) -> RGB8;
    pub fn fill(&mut self, c: RGB8);
    pub fn row(&mut self, y: u16) -> &mut [RGB8];
    pub fn path(&mut self) -> PathView<'_>;              // обход «по проводу» для 1D
}
/// Path-вид: (index) -> (x, y), порождён маппингом прошивки.
pub struct PathView<'a> { /* ... */ }
impl PathView<'_> { pub fn set(&mut self, i: usize, c: RGB8); pub fn len(&self) -> usize; }

/// Эффект: owns своё состояние; за вызов обязан заполнить кадр целиком.
pub trait Effect {
    fn name(&self) -> &'static str;
    /// `t_ms` — монотонное время (мс, wrapping u32), единая тайм-база прошивки.
    fn render(&mut self, frame: &mut Frame<'_>, t_ms: u32, params: Params);
}

/// Фабрика для реестра.
pub type Factory = fn(Params) -> Box<dyn Effect>;

/// Кроссфейд двух кадров с 16-битной альфой (аналог color_blend16).
pub fn blend_frames(dst: &mut [RGB8], a: &[RGB8], b: &[RGB8], alpha: u16);
```

```rust
// прошивка: маппинг — чистая функция, конфигурация монтажа
pub struct Mapping { pub w: u16, pub h: u16, pub serpentine: bool, pub vertical: bool }
impl Mapping {
    /// Полный набор конфигураций — как канонический XY() FastLED (XYMatrix.ino:100–134).
    pub fn index(&self, x: u16, y: u16) -> usize { /* ... */ }
    pub fn path(&self) -> impl Iterator<Item = (u16, u16)> { /* ... */ } // для PathView
}

pub static REGISTRY: &[(&str, Factory)] = &[
    ("rain",   effect_rain::create),
    ("plasma", effect_plasma::create),
];
```

```rust
// крейт эффекта (crates/effect-plasma): 2D — первоклассный случай
pub struct Plasma { time_base: u32, noise: SmallRng }

pub fn create(params: Params) -> Box<dyn Effect> { Box::new(Plasma::new(params)) }

impl Effect for Plasma {
    fn name(&self) -> &'static str { "plasma" }
    fn render(&mut self, frame: &mut Frame<'_>, t_ms: u32, params: Params) {
        // цикл по y, по x: hsv2rgb(...); frame.set(x, y, c);
        // как mode_2DPlasmaball: кэш шума на строку (FX.cpp)
    }
}
```

```rust
// крейт эффекта (crates/effect-rainbow): 1D-эффект без дублирования
impl Effect for Rainbow {
    fn render(&mut self, frame: &mut Frame<'_>, t_ms: u32, params: Params) {
        let n = frame.path().len();
        for i in 0..n {
            let c = hsv2rgb(Hsv { hue: (i * 255 / n).wrapping_add(t as u8), .. });
            frame.path().set(i, c); // физически бежит по проводу змейкой
        }
    }
}
```

```rust
// прошивка: движок кадров с кроссфейдом (см. раздел 3)
// cur: Box<dyn Effect>, prev: Option<(Box<dyn Effect>, u16 /*alpha*/)>
// fb_a, fb_b: статические [RGB8; W*H]
// кадр: cur.render(Frame::new(fb_a), t, params); при кроссфейде аналогично fb_b,
// blend_frames(&mut out, &fb_b, &fb_a, alpha);
// затем: apply mapping -> linear buffer -> strip.write(out.iter()) // SmartLedsWrite
```

---

## 8. Выводы для проекта

1. **Матрица — первоклассная модель кадра**: `Frame { w, h, pixels: &[RGB8] }`, эффекты пишут в (x, y) — ровно как WLED (`setPixelColorXYRaw` = `x + y*vWidth`, [FX.h, L569](https://github.com/Aircoookie/WLED/blob/main/wled00/FX.h#L569)), но с размерами в кадре, а не в глобальных статиках.
2. **Маппинг (x, y) → диод — отдельный чистый слой прошивки** с конфигурацией serpentine/vertical/угла (канон — FastLED `XY()`, [XYMatrix.ino, L100–134](https://github.com/FastLED/FastLED/blob/master/examples/XYMatrix/XYMatrix.ino#L100)); эффекты его не видят; юнит-тесты маппинга — обязательный гейт до монтажа.
3. **1D без дублирования**: 1D-эффекты работают через path-вид кадра (обход по проводу, порождён тем же маппингом) — аналог вырождения `setPixelColorXY` в линейный вызов у WLED ([FX.h, L811](https://github.com/Aircoookie/WLED/blob/main/wled00/FX.h#L811)); полосовые — через `row/col`.
4. **Трейт с состоянием в `self` + фабрики `fn(Params) -> Box<dyn Effect>`** — носитель «addon-ности»; реестр — статический срез в прошивке (аналог `ModeData`+`addEffect`). Закрытый enum (smart-leds-fx) для addon-крейтов тупиковый.
5. **Кроссфейд — упрощённая схема WLED**: два фреймбуфера + 16-битная альфа + `color_blend`-аналог; оба эффекта рендерятся каждый кадр перехода. 3 КБ на 500 ячеек (~0,6% SRAM ESP32-C6) — не ограничение.
6. **Гамма — на выходе движка** (таблица 256×u8), эффекты о ней не думают; alpha кроссфейда — с гамма-компенсацией, как в `blendSegment`.
7. **Крейты эффектов — чистые no_std** без esp-hal: вся математика (эффекты, маппинг, кроссфейд, HSV/гамма) проверяется `cargo test --workspace` на хосте.

## 9. Открытые вопросы

1. **Физическая конфигурация матрицы**: сколько рядов/колонок (W×H), змейка по рядам или по столбцам, горизонтальная или вертикальная проводка, угол старта провода — всё это **конфиг** `Mapping`; измерить при монтаже (FastLED прямо требует зафиксировать раскладку до отрисовки).
2. **Семантика 1D на матрице по умолчанию**: path-вид (по проводу) или ось окна (ось с повторением на ряды)? Предложен path как физически честный; окончательное решение — при первом 1D-эффекте (можно параметром оси).
3. **RGBW**: нужен ли белый канал — меняет тип пикселя кадра и цену буферов ×4/3. Решение за владельцем по целевому железу.
4. **Альфа кроссфейда: гамма-пространство или линейное?** WLED смешивает в гамма-пространстве с `gamma8inv` по opacity ([FX_fcn.cpp, L1587–1588](https://github.com/Aircoookie/WLED/blob/main/wled00/FX_fcn.cpp#L1587)); выбрать и зафиксировать (влияет на восприятие середины перехода).
5. **Автосборка реестра**: ручная таблица (предложено) или `linkme` distributed slice — магия компоновки против одной строки регистрации вручную.
6. **Контракт времени**: достаточно ли `t_ms` (рендер строго раз в FRAME_TIME_MS), или нужен явный `dt`? WLED-эффекты фактически полагаются на счётчик кадров `SEGENV.call` ([FX.h, L489](https://github.com/Aircoookie/WLED/blob/main/wled00/FX.h#L489)) — нужен один договор.
7. **Случайность**: seeded-генератор в фабрике (детерминизм/тесты) + seed с esp-hal RNG на железе — оформить при первом шумовом эффекте (как Plasma Ball, кэширующий perlin-шум).
8. **MAX-размер матрицы**: константа для статических буферов — фиксировать при монтаже (оценка владельца: 100–500 ячеек).
9. **Метаданные параметров для UI** (аналог `"Plasma Ball@Speed,,Fade,Blur;;!;2"`): понадобится, только если появится UI настроек; до тех пор — `name()` достаточно.
