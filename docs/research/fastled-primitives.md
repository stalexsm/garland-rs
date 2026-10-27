# Исследование: точные целочисленные алгоритмы примитивов FastLED (для порта в garland-rs)

**Цель:** перенести математику эффектов экосистемы FastLED (портируемые прошивки) в no_std Rust для ESP32-C6 — только целые числа, без float, без внешних зависимостей в ядре. Актуальность сверки: последний релиз FastLED на момент исследования — 3.10.8 (дата публикации по GitHub API: 2026-10-18).

**Обозначения достоверности:** формулы и таблицы ниже — прямое содержимое исходников FastLED (прочитаны построчно по ссылкам). Пометки *«вывод»* — интерпретация/рекомендация исследователя, не цитата.

---

## 0. Методика и базовые соглашения

- Основная версия для порта: **FastLED 3.6.0** (тег) — классическая раскладка `src/lib8tion/*.h`, `src/hsv2rgb.cpp`, `src/noise.cpp`, `src/colorutils.cpp`, `src/pixeltypes.h`, та же, против которой собирался исходной прошивки. Сверка с последним релизом **3.10.8**: в нём файлы переехали в `src/fl/math/*` (старые пути стали форвардингами), алгоритмы `random8` и beat-генераторов проверены байт-в-байт — идентичны (в beat-генераторах 3.10.8 добавлен только clamp `result > highest`). Остальные файлы 3.10.8 отдельно не сверялись (см. «Missing evidence»).
- На не-AVR платформах (ESP32-C6) FastLED компилирует **чистые C-ветки**: в `lib8tion.h` для «unspecified architecture» задаются `QADD8_C 1, SCALE8_C 1, BLEND8_C 1` и т.д. Все AVR-asm варианты (`*_AVRASM`, `*_LEAVING_R1_DIRTY`, `memmove8`) портировать не нужно. Источник: `src/lib8tion.h` (выбор C-реализаций).
- Дефолты конфига, влияющие на результаты (источник `src/fastled_config.h`): `FASTLED_SCALE8_FIXED 1`, `FASTLED_BLEND_FIXED 1`, `FASTLED_NOISE_FIXED 1`. Портировать нужно поведение именно с этими флагами — это то, что видно на LEDs в исходной прошивки.
- `fract8` = `u8` в смысле «n/256»; `fract16` = `u16` = «n/65536»; `accum88` = `u16` в формате Q8.8 (8 бит целых, 8 бит дробных).

---

## 1. Насыщающая арифметика 8 бит: scale8, qadd8, qsub8, qmul8, lerp8by8

**Семантика.** Вся яркостная математика FastLED — это умножения на дробь n/256 и сложения с насыщением по границам байта. `scale8` — базовый «диммер», на нём построены fade/blend/palette/noise. Критично: с включённым `FASTLED_SCALE8_FIXED` scale8 считает `i*(scale+1)>>8`, т.е. `scale8(255,255) == 255`, а не 254 (исправление ошибки старой версии, `fastled_config.h`).

**Алгоритмы** (C-ветки; источники: `src/lib8tion/scale8.h` — scale8/scale8_video/nscale8x3; `src/lib8tion/math8.h` — qadd8/qsub8/qmul8; `src/lib8tion.h` — lerp8by8):

```c
scale8(i, sc)          = (u16)(i) * (u16)(sc + 1) >> 8            // FIXED
scale8_video(i, sc)    = ((i16)i * (i16)sc >> 8) + (if i != 0 && sc != 0 { 1 } else { 0 })
nscale8x3(r,g,b, sc)   = ((u16)r * (sc+1) >> 8, ...)              // без video-правила
nscale8x3_video(...)   = r==0 ? 0 : (r*sc >> 8) + (sc != 0)       // ноль остаётся нулём,
                                                                  // ненулевой не гаснет в 0
qadd8(i, j)            = min(i + j, 255)
qsub8(i, j)            = max(i - j, 0)
qmul8(i, j)            = min(i * j, 255)
lerp8by8(a, b, frac)   = if b > a { a + scale8(b-a, frac) } else { a - scale8(a-b, frac) }
```

**Rust (integer-only):**

```rust
#[inline] pub fn scale8(i: u8, sc: u8) -> u8 {
    (((i as u16) * ((sc as u16) + 1)) >> 8) as u8          // FASTLED_SCALE8_FIXED = 1
}
#[inline] pub fn scale8_video(i: u8, sc: u8) -> u8 {
    (((i as i16 * sc as i16) >> 8) as u8) + u8::from(i != 0 && sc != 0)
}
#[inline] pub fn qadd8(i: u8, j: u8) -> u8 { i.saturating_add(j) }
#[inline] pub fn qsub8(i: u8, j: u8) -> u8 { i.saturating_sub(j) }
#[inline] pub fn qmul8(i: u8, j: u8) -> u8 {
    ((i as u16 * j as u16) as u16).min(255) as u8
}
#[inline] pub fn lerp8by8(a: u8, b: u8, frac: u8) -> u8 {
    if b > a { a + scale8(b - a, frac) } else { a - scale8(a - b, frac) }
}
```

Побратимы и полезны при порте: `scale16(i, sc) = (u32(i) * (u32(sc)+1)) / 65536` (`scale8.h`), `scale16by8` (`(i*(sc+1))>>8`), `dim8_video(x) = scale8_video(x,x)` — приближение гаммы 2.0 (`scale8.h`, докблок «approximations of gamma correction with a gamma value of 2.0»), `map8(in, s, e) = scale8(in, e-s) + s` (`lib8tion.h`).

---

## 2. hsv2rgb_rainbow (именно rainbow, не spectrum)

**Семантика.** Основная конвертация FastLED (все `CRGB = CHSV`, `setHSV`, эффекты исходной прошивки идут через неё — `pixeltypes.h`: конструктор `CRGB(const CHSV&)` вызывает именно `hsv2rgb_rainbow`). Цветовой круг разбит на 8 секций по 32 градации hue; между опорными точками — кусочно-линейный градиент с фиксированными «полками» 255/171/170/85 (учитывает повышенную воспринимаемую яркость жёлтого). Это НЕ плавная спектральная конвертация: `hsv2rgb_spectrum` в `hsv2rgb.cpp` — отдельная функция (просто `hue = scale8(hue, 191)` + raw-конвертация), эффекты её не используют.

**Таблица градиента** (источник: `src/hsv2rgb.cpp`, функция `hsv2rgb_rainbow`; константы `K255=255, K171=171, K170=170, K85=85`; флаги `Y1=1, Y2=0, G2=0, Gscale=0` — берётся ветка Y1):

Обозначения: `offset = hue & 0x1F` (0..31); `offset8 = offset << 3`; `third = scale8(offset8, 85)` (макс 85); `twothirds = scale8(offset8, 171)` (макс 170).

| Секция | hue | Переход | R | G | B |
|---|---|---|---|---|---|
| 0 | 0–31 | красный→оранжевый | `255 − third` | `third` | 0 |
| 1 | 32–63 | оранжевый→жёлтый | `171` | `85 + third` | 0 |
| 2 | 64–95 | жёлтый→зелёный | `171 − twothirds` | `170 + third` | 0 |
| 3 | 96–127 | зелёный→аква | 0 | `255 − third` | `third` |
| 4 | 128–159 | аква→синий | 0 | `171 − twothirds` | `85 + twothirds` |
| 5 | 160–191 | синий→пурпурный | `third` | 0 | `255 − third` |
| 6 | 192–223 | пурпурный→розовый | `85 + third` | 0 | `171 − third` |
| 7 | 224–255 | розовый→красный | `170 + third` | 0 | `85 − third` |

*(вывод: замыкание круга неидеально — при hue=255 получается ≈(253,0,2) вместо (255,0,0); это известная особенность аппроксимации, повторять 1-в-1.)*

**Сатурация и яркость** (после выбора секции; источник: тот же `hsv2rgb.cpp`, хвост `hsv2rgb_rainbow`, ветки `FASTLED_SCALE8_FIXED==1`):

```c
if sat != 255:
    if sat == 0:  r = g = b = 255                      // нулевая сатурация = белый
    else:
        desat     = scale8_video(255 - sat, 255 - sat) // квадратичное приглушение
        satscale  = 255 - desat
        r,g,b     = scale8(r, satscale), ...           // FIXED: (ch*(satscale+1))>>8
        r,g,b     = r + desat, g + desat, b + desat    // brightness_floor
if val != 255:
    val = scale8_video(val, val)                       // «видео»-приглушение V
    if val == 0: r = g = b = 0
    else:        r,g,b = scale8(r, val), ...           // FIXED-умножение
```

**Про apply_gamma_video.** Внутри `hsv2rgb_rainbow` гамма **не применяется**. `applyGamma_video` — отдельная пост-обработка в `src/colorutils.cpp`, и она **float** (`pow(orig, gamma) * 255.0`), с защитой «не гасить ненулевое в ноль». Для no_std-порта она не годится. Эквивалентные целочисленные замены (вывод): 256-байтная LUT гаммы, посчитанная офлайн, либо вообще отказ — в исходной прошивки гамму не вызывают; «видео»-приглушение внутри rainbow-конвертации уже встроено.

**Rust-скетч (ядро):**

```rust
pub fn hsv2rgb_rainbow(h: u8, s: u8, v: u8) -> (u8, u8, u8) {
    let offset8 = ((h & 0x1F) as u16) << 3;              // 0..248
    let third = scale8(offset8 as u8, 85);
    let twothirds = scale8(offset8 as u8, 171);
    let (mut r, mut g, mut b) = match h >> 5 {
        0 => (255 - third, third, 0),
        1 => (171, 85u8.saturating_add(third), 0),
        2 => (171 - twothirds, 170u8.saturating_add(third), 0),
        3 => (0, 255 - third, third),
        4 => (0, 171 - twothirds, 85u8.saturating_add(twothirds)),
        5 => (third, 0, 255 - third),
        6 => (85u8.saturating_add(third), 0, 171 - third),
        _ => (170u8.saturating_add(third), 0, 85u8.saturating_sub(third)),
    };
    if s != 255 {
        if s == 0 { return (255, 255, 255); }
        let desat = scale8_video(255 - s, 255 - s);
        let sc = 255 - desat;
        r = scale8(r, sc); g = scale8(g, sc); b = scale8(b, sc);
        r += desat; g += desat; b += desat;
    }
    if v != 255 {
        let vv = scale8_video(v, v);
        if vv == 0 { return (0, 0, 0); }
        r = scale8(r, vv); g = scale8(g, vv); b = scale8(b, vv);
    }
    (r, g, b)
}
```

---

## 3. blend / nblend 8-битные, fadeToBlackBy (scale8 vs scale8_video)

**Семантика.** `blend8(a, b, amountOfB)` — линейная интерполяция байта a к b на долю amountOfB/256. `nblend(CRGB&, CRGB&, fract8)` — попиксельный blend трёх каналов; `blend(...)` — то же без мутации. `fadeToBlackBy` — уменьшение яркости на fadeBy долю (fadeBy/255) за вызов.

**Алгоритм blend8** (источник: `src/lib8tion/math8.h`, ветка `FASTLED_BLEND_FIXED && FASTLED_SCALE8_FIXED`, C-версия):

```c
partial = (a << 8) | b;        // a*256 + b
partial += b * amountOfB;
partial -= a * amountOfB;      // итог: (a*256 + b + (b-a)*amountOfB) / 256
result  = partial >> 8;
```

Точная формула: `blend8 = (a*256 + b + (b-a)*k) >> 8`, k = amountOfB. Даёт точные границы: при k=0 → a, при k=255 → b (в отличие от старого `scale8(a,255-k)+scale8(b,k)`, который вносил ошибку ±1 — старый вариант оставлен в `colorutils.cpp` в `#if 0` с комментарием «rounding errors»).

**nblend / blend** (источник: `src/colorutils.cpp`): при amountOfOverlay==0 вернуть без изменений, при ==255 — заменить целиком, иначе три вызова `blend8` по каналам. Есть отдельный `nblend(CHSV&, ...)` с выбором направления по кругу оттенков (SHORTEST_HUES и т.п., там hue крутится по модулю 256 через `scale8(delta, k)`).

**fadeToBlackBy — важное уточнение к постановке задачи:** это **не** scale8_video. Два семейства (источники: `src/colorutils.cpp` и `src/pixeltypes.h`):

| Функция | Масштабирование | Правило |
|---|---|---|
| `fadeToBlackBy(leds, n, fadeBy)`, `fade_raw` | `nscale8(255 - fadeBy)` → `nscale8x3` | FIXED: `(ch*(scale+1))>>8`, может уйти в 0 |
| `fadeLightBy`, `fade_video`, `CRGB::nscale8_video` | `nscale8_video(255 - fadeBy)` | video: ненулевое не гаснет (`(ch*sc>>8)+1`) |

Метод `CRGB::fadeToBlackBy(fadefactor)` (`pixeltypes.h`) = `nscale8x3(r,g,b, 255-fadefactor)`. исходной прошивки-«fader» эквивалентен `fadeToBlackBy`.

**Rust:**

```rust
#[inline] pub fn blend8(a: u8, b: u8, k: u8) -> u8 {
    let partial = ((a as u16) << 8) | b as u16;          // a*256 + b
    let partial = partial + b as u16 * k as u16;
    let partial = partial - a as u16 * k as u16;         // не уходит в минус: k<=255
    (partial >> 8) as u8
}
pub fn nblend(dst: &mut (u8, u8, u8), ov: (u8, u8, u8), k: u8) {
    if k == 0 { return; }
    if k == 255 { *dst = ov; return; }
    *dst = (blend8(dst.0, ov.0, k), blend8(dst.1, ov.1, k), blend8(dst.2, ov.2, k));
}
pub fn fade_to_black_by(px: &mut (u8, u8, u8), fade_by: u8) {
    let sc = 255 - fade_by;                              // nscale8x3, FIXED
    px.0 = scale8(px.0, sc); px.1 = scale8(px.1, sc); px.2 = scale8(px.2, sc);
}
```

---

## 4. blur1d / blur2d

**Семантика.** Одномерный бокс-блюр «распространяет» часть яркости пикселя на соседей слева/справа (2D-версия — на 8 соседей, как два прохода по строкам и столбцам). Свет полностью **не** сохраняется: повторные вызовы медленно гасят картинку (задумано как способ плавно очистить матрицу). Док-комментарий `colorutils.cpp`: 0 = нет растекания, 64 = умеренное, 172 = максимум гладкости, 173..255 = шире, но с мерцанием.

**Алгоритм** (источник: `src/colorutils.cpp`, `blur1d`, `blur2d`, `blurRows`, `blurColumns`). Окно — `[i-1, i, i+1]`; целочисленные веса:

```c
keep = 255 - blur_amount;         // сколько остаётся самому, доля n/256
seep = blur_amount >> 1;          // сколько уходит КАЖДОМУ соседу (половина blur)
carryover = Black;
for i in 0..n:
    part = scale8(cur, seep)      // nscale8, FIXED: (ch*(seep+1))>>8
    cur   = scale8(cur, keep)
    cur  += carryover             // qadd8 по каналам (CRGB::operator+=)
    if i > 0: leds[i-1] += part
    leds[i] = cur
    carryover = part
```

`blur2d(w, h, amt) = blurRows(...) затем blurColumns(...)` — оба прохода это тот же одномерный фильтр, в 3.6.0 каждый ряд/столбец обрабатывается через `XY(x,y)`-доступ (независимо от физической раскладки матрицы).

**Rust:**

```rust
pub fn blur1d(leds: &mut [(u8, u8, u8)], blur: u8) {
    let keep = 255 - blur;
    let seep = blur >> 1;
    let mut carry = (0u8, 0u8, 0u8);
    for i in 0..leds.len() {
        let cur = leds[i];
        let part = (scale8(cur.0, seep), scale8(cur.1, seep), scale8(cur.2, seep));
        let mut c = (scale8(cur.0, keep), scale8(cur.1, keep), scale8(cur.2, keep));
        c.0 = c.0.saturating_add(carry.0);
        c.1 = c.1.saturating_add(carry.1);
        c.2 = c.2.saturating_add(carry.2);
        if i > 0 {
            let p = &mut leds[i - 1];
            p.0 = p.0.saturating_add(part.0);
            p.1 = p.1.saturating_add(part.1);
            p.2 = p.2.saturating_add(part.2);
        }
        leds[i] = c;
        carry = part;
    }
}
// blur2d: blur1d по каждой строке, затем по каждому столбцу (два прохода).
```

---

## 5. random8 / random16

**Семантика.** Единый ГПСЧ на всю прошивку — **16-битный LCG** (не xorshift): `X(n+1) = X(n)*2053 + 13849 (mod 65536)`. Дефолтный сид **1337** (источник: `src/lib8tion.cpp`, `#define RAND16_SEED 1337; uint16_t rand16seed = RAND16_SEED;`). Формула и константы: `src/lib8tion/random8.h` (`FASTLED_RAND16_2053`, `FASTLED_RAND16_13849`, докблок `X(n+1) = (2053 * X(n)) + 13849`).

**Алгоритмы** (источник: `src/lib8tion/random8.h`):

```c
random8()        : seed = seed*2053 + 13849;
                   return (u8)(seed & 0xFF) + (u8)(seed >> 8);   // СУММА байтов, не старший байт!
random16()       : seed = seed*2053 + 13849; return seed;
random8(lim)     : (random8() * lim) >> 8                       // 0..lim-1, слегка смещённое
random8(min,lim) : random8(lim - min) + min
random16(lim)    : ((u32)random16() * lim) >> 16
random16(min,lim): random16(lim - min) + min
random16_set_seed(s); random16_get_seed(); random16_add_entropy(e)  // seed += e
```

Подводный камень для порта: `random8()` возвращает **сумму младшего и старшего байтов** по модулю 256 («better mixing», комментарий в исходнике), а не `seed >> 8`.

**Rust:**

```rust
pub struct Rng16 { seed: u16 }
impl Rng16 {
    pub fn new() -> Self { Self { seed: 1337 } }                 // RAND16_SEED
    #[inline] pub fn random16(&mut self) -> u16 {
        self.seed = self.seed.wrapping_mul(2053).wrapping_add(13849);
        self.seed
    }
    #[inline] pub fn random8(&mut self) -> u8 {
        let s = self.random16();
        (s as u8).wrapping_add((s >> 8) as u8)                   // сумма байтов
    }
    #[inline] pub fn random8_lim(&mut self, lim: u8) -> u8 {
        ((self.random8() as u16 * lim as u16) >> 8) as u8
    }
    #[inline] pub fn random16_lim(&mut self, lim: u16) -> u16 {
        ((self.random16() as u32 * lim as u32) >> 16) as u16
    }
}
```

---

## 6. sin8 / cos8 / sin16

**Семантика и главный ответ на вопрос ТЗ:** sin8 в FastLED — **не таблица на 256 байт**. Это кусочно-линейная аппроксимация по 4 секциям по 16 значений с 8-байтовой таблицей наклонов `b_m16_interleave`. Дамп 256-байтной LUT **не нужен**; для побайтовой идентичности с FastLED достаточно воспроизвести алгоритм ниже (заявленная точность — отклонение ≤2% от float `(sin(x)*128)+128`, докблок `trig8.h`; для LED-эффектов идентичность бит-в-бит обеспечивается самим алгоритмом).

**Алгоритм** (источник: `src/lib8tion/trig8.h`, `sin8_C` + `const uint8_t b_m16_interleave[] = { 0, 49, 49, 41, 90, 27, 117, 10 }`):

```c
offset = theta;
if (theta & 0x40) offset = 255 - offset;   // зеркалим правую половину четверти
offset &= 0x3F;                            // 0..63
secoffset = offset & 0x0F;                 // 0..15
if (theta & 0x40) secoffset++;             // 1..16 во второй половине
section = offset >> 4;                     // 0..3
b   = b_m16_interleave[section*2];
m16 = b_m16_interleave[section*2 + 1];
mx  = (m16 * secoffset) >> 4;
y   = (i8)(mx + b);
if (theta & 0x80) y = -y;
y += 128;
return y;                                  // 0..255
cos8(t) = sin8(t + 64)                     // сдвиг на четверть периода
```

**Rust:**

```rust
const B_M16: [u8; 8] = [0, 49, 49, 41, 90, 27, 117, 10];
pub fn sin8(theta: u8) -> u8 {
    let mut offset = theta;
    if theta & 0x40 != 0 { offset = 255 - offset; }
    offset &= 0x3F;
    let mut secoffset = (offset & 0x0F) as u16;
    if theta & 0x40 != 0 { secoffset += 1; }
    let section = (offset >> 4) as usize;
    let b = B_M16[section * 2] as i16;
    let m16 = B_M16[section * 2 + 1] as u16;
    let mx = ((m16 * secoffset) >> 4) as i16;
    let mut y = (mx + b) as i8;
    if theta & 0x80 != 0 { y = -y; }
    (y as i16 + 128) as u8
}
#[inline] pub fn cos8(theta: u8) -> u8 { sin8(theta.wrapping_add(64)) }
```

**sin16 кратко** (источник: `src/lib8tion/trig8.h`, `sin16_C`; точность ≤0.69% от `sin(x)*32767`):

```c
base  = [0, 6393, 12539, 18204, 23170, 27245, 30273, 32137]  // u16, 8 значений
slope = [49, 48, 44, 38, 31, 23, 14, 4]                      // u8, 8 значений
offset = (theta & 0x3FFF) >> 3;              // 0..2047
if (theta & 0x4000) offset = 2047 - offset;  // зеркальная половина
section = offset >> 8;                       // 0..7
secoffset8 = (u8)offset >> 1;                // 0..127
y = base[section] + slope[section] * secoffset8;   // i16
if (theta & 0x8000) y = -y;
cos16(t) = sin16(t + 16384)
```

Родственные генераторы (`src/lib8tion.h`): `triwave8` (треугольник), `quadwave8 = ease8InOutQuad(triwave8(x))`, `cubicwave8` — иногда дешевле sin8.

---

## 7. inoise8 — 8-битный Перлин-подобный шум

**Семантика.** `inoise8(x, y=0, z=0)` — градиентный шум Перлина на 8-битной сетке: вход — координаты `u16` (старший байт = номер ячейки 0..255, младший — позиция внутри ячейки), выход — `u8` 0..255 (распределение сгущено к среднему, плато около 127/128). Реализация — «fixed»-версия (по умолчанию `FASTLED_NOISE_FIXED 1`): сглаживание между узлами через `ease8InOutQuad`, без старых «разрывов». Источник: `src/noise.cpp`.

**Структура алгоритма** (все имена и формулы — из `src/noise.cpp`):

1. Таблица перестановок `p[]` — 256 значений Кена Перлина + дубликат первого (257-й элемент `p[256]=151` для доступа `P(X+1)` без заворота). Полный дамп обязателен для порта:

```text
151,160,137, 91, 90, 15,131, 13,201, 95, 96, 53,194,233,  7,225,
140, 36,103, 30, 69,142,  8, 99, 37,240, 21, 10, 23,190,  6,148,
247,120,234, 75,  0, 26,197, 62, 94,252,219,203,117, 35, 11, 32,
 57,177, 33, 88,237,149, 56, 87,174, 20,125,136,171,168, 68,175,
 74,165, 71,134,139, 48, 27,166, 77,146,158,231, 83,111,229,122,
 60,211,133,230,220,105, 92, 41, 55, 46,245, 40,244,102,143, 54,
 65, 25, 63,161,  1,216, 80, 73,209, 76,132,187,208, 89, 18,169,
200,196,135,130,116,188,159, 86,164,100,109,198,173,186,  3, 64,
 52,217,226,250,124,123,  5,202, 38,147,118,126,255, 82, 85,212,
207,206, 59,227, 47, 16, 58, 17,182,189, 28, 42,223,183,170,213,
119,248,152,  2, 44,154,163, 70,221,153,101,155,167, 43,172,  9,
129, 22, 39,253, 19, 98,108,110, 79,113,224,232,178,185,112,104,
218,246, 97,228,251, 34,242,193,238,210,144, 12,191,179,162,241,
 81, 51,145,235,249, 14,239,107, 49,192,214, 31,181,199,106,157,
184, 84,204,176,115,121, 50, 45,127,  4,150,254,138,236,205, 93,
222,114, 67, 29, 24, 72,243,141,128,195, 78, 66,215, 61,156,180,
151
```

2. Хэши углов ячейки (вся арифметика — `u8` с заворотом):
`X=x>>8, Y=y>>8, Z=z>>8; A = P(X)+Y; AA = P(A)+Z; AB = P(A+1)+Z; B = P(X+1)+Y; BA = P(B)+Z; BB = P(B+1)+Z`.

3. Дробные части `u=x, v=y, w=z` (младшие байты) сглаживаются: `u = ease8InOutQuad(u)` и т.д.; знаковые координаты для градиента: `xx = (u8)(x)>>1 & 0x7F` (и N = 0x80 для соседних узлов).

4. `grad8(hash, x, y, z)` (источник: `noise.cpp`): `hash &= 0xF`; `u = (hash&8) ? y : x`; `v = hash<4 ? y : (hash==12||hash==14) ? x : z`; при `hash&1` инверт u, при `hash&2` — v; результат `avg7(u,v)`, где `avg7(i,j) = (i>>1) + (j>>1) + (i & 1)` (`src/lib8tion/math8.h`). 2D-вариант `grad8(hash,x,y)`: `hash&4 ? (u,v)=(y,x) : (u,v)=(x,y)`, те же инверсии и `avg7`.

5. Трилинейная интерполяция `lerp7by8(a,b,frac)` (signed, источник: `noise.cpp`): `b>a ? a + scale8(b-a, frac) : a - scale8(a-b, frac)` — то же, что `lerp8by8`, но для i8.

6. Схема свёртки: `X1..X4 = lerp(grad8(...AA/BA/AB/BB...), u)`, `Y1 = lerp(X1,X2,v)`, `Y2 = lerp(X3,X4,v)`, `ans = lerp(Y1,Y2,w)` → диапазон **−64..+64**.

7. Финал: `inoise8 = qadd8(n + 64, n + 64)` (0..255; комментарий в исходнике «-64..+64 → 0..128 → 0..255»).

8. `ease8InOutQuad` (источник: `src/lib8tion.h`, C-ветка EASE8_C):

```c
j = i; if (j & 0x80) j = 255 - j;
jj = scale8(j, j);  jj2 = jj << 1;
if (i & 0x80) jj2 = 255 - jj2;
return jj2;
```

**Rust-скетч (2D, целочисленный):**

```rust
const P: [u8; 257] = [ /* дамп выше */ ];
#[inline] fn grad8_2d(hash: u8, x: i8, y: i8) -> i8 {
    let h = hash;
    let (mut u, mut v) = if h & 4 != 0 { (y, x) } else { (x, y) };
    if h & 1 != 0 { u = -u; }
    if h & 2 != 0 { v = -v; }
    (u >> 1).wrapping_add(v >> 1).wrapping_add(u & 1)     // avg7
}
#[inline] fn lerp7by8(a: i8, b: i8, frac: u8) -> i8 {
    if b > a { a + scale8((b - a) as u8, frac) as i8 }
    else     { a - scale8((a - b) as u8, frac) as i8 }
}
pub fn inoise8(x: u16, y: u16) -> i8 {
    let (xh, yh) = ((x >> 8) as u8, (y >> 8) as u8);
    let a  = P[xh as usize].wrapping_add(yh);
    let aa = P[a as usize];
    let ab = P[a as usize + 1];                 // a+1 <= 256 -> таблица 257
    let b  = P[xh.wrapping_add(1) as usize].wrapping_add(yh);
    let ba = P[b as usize];
    let bb = P[b as usize + 1];
    let ease = |t: u16| ease8_in_out_quad(t as u8);
    let (u, v) = (ease(x), ease(y));
    let xx = (((x as u8) >> 1) & 0x7F) as i8;
    let yy = (((y as u8) >> 1) & 0x7F) as i8;
    let nn = 0x80u8 as i8;                       // -128
    let x1 = lerp7by8(grad8_2d(aa, xx, yy), grad8_2d(ba, xx.wrapping_sub(nn), yy), u);
    let x2 = lerp7by8(grad8_2d(ab, xx, yy.wrapping_sub(nn)), grad8_2d(bb, xx.wrapping_sub(nn), yy.wrapping_sub(nn)), u);
    let ans = lerp7by8(x1, x2, v);
    ans                                          // -64..+64
}
pub fn inoise8_u8(x: u16, y: u16) -> u8 {
    let n = inoise8(x, y).wrapping_add(64);
    n.saturating_add(n)                          // qadd8(n, n)
}
```

**Как эффекты анимируют шум.** Паттерн — канонический порт примера FastLED «Noise + Palette»:

- координаты `u16 x, y, z`; за кадр: **`z += speed`; `x += speed/8`; `y -= speed/16`** — вот эти инкременты и есть «движение» шума;
- каждому пикселю (i,j): `inoise8(x + scale*i, y + scale*j, z)`;
- масштабы на эффект (константы `*_SCALE`): Madness 100, Cloud 30, Lava 50, Plasma 30, Rainbow 30, RainbowStripe 20, Zebra 30, Forest 120, Ocean 90;
- сглаживание во времени при `speed < 50`: `sm = 200 - speed*4`; `data = scale8(old, sm) + scale8(new, 256-sm)`;
- постобработка: `data = qsub8(data, 16); data = qadd8(data, scale8(data, 39))`;
- рендер: `ColorFromPalette(pal, index, bri)`; при `colorLoop` — `index += ihue++`; яркость `bri = 255 if bri>127 else dim8_raw(bri*2)`;
- `speed` по умолчанию 20, задаётся слайдером в приложении; «скорость шума» ≈ инкремент z за кадр (типично 10..60 при 30–60 fps).

**Про INA/INC:** идентификаторов `INA`/`INC` в прошивках исходной прошивки v1.12 (BT) и v1.13 (WiFi) **не найдено** (файлы `noise_effects.ino`, `effects.ino` просмотрены). Если имелась в виду анимация координат — это инкременты из списка выше (`z += speed` и т.д.). Вопрос остаётся открытым — см. «Missing evidence».

---

## 8. beat88 / beat16 / beat8, beatsin88 / beatsin16 / beatsin8

**Семантика.** Фазовые генераторы от миллисекундного таймера: `beat*` — пилообразный фазовый аккумулятор (0..65535 с заворотом), `beatsin*` — синус с той же фазой, отмасштабированный в диапазон [lowest, highest]. BPM можно задавать простым числом (1..255) или в Q8.8 (`bpm*256`). Источник: `src/lib8tion.h`, группа BeatGenerators (в 3.10.8 — `src/fl/math/beat.h`, формулы те же).

**Точные формулы:**

```c
beat88(bpm88, t0) = (u16)((((millis() - t0) * bpm88 * 280)) >> 16)
   // 65536/60000 ≈ 280/256; погрешность ~0.05% (120 BPM ≈ 119.93)
beat16(bpm, t0)   = beat88(bpm < 256 ? bpm << 8 : bpm, t0)      // u16 пила
beat8(bpm, t0)    = beat16(bpm, t0) >> 8                         // u8 пила
beatsin88(bpm88, lo, hi, t0, ph) = lo + scale16(sin16(beat88(bpm88,t0) + ph) + 32768, hi - lo)
beatsin16(...)  аналогично через sin16/scale16
beatsin8(bpm, lo, hi, t0, ph)    = lo + scale8 (sin8 (beat8 (bpm,t0) + ph), hi - lo)
```

В 3.10.8 добавлен clamp `if result > highest { result = highest }` во все beatsin-варианты (в 3.6.0 его нет — при lo=0/hi=255 переполнения не бывает, при экзотических диапазонах может).

**Критично для Rust:** в C произведение `(millis - t0) * bpm88 * 280` считается в `u32` с **молчаливым переполнением** — именно это даёт периодический заворот пилы. В Rust нужен `wrapping_mul`; потеря старших битов — часть спецификации, не баг:

```rust
pub fn beat88(bpm88: u16, t0: u32, ms: u32) -> u16 {
    let t = ms.wrapping_sub(t0)
        .wrapping_mul(bpm88 as u32)
        .wrapping_mul(280);
    (t >> 16) as u16
}
pub fn beat16(bpm: u16, t0: u32, ms: u32) -> u16 {
    beat88(if bpm < 256 { bpm << 8 } else { bpm }, t0, ms)
}
pub fn beat8(bpm: u16, t0: u32, ms: u32) -> u8 { (beat16(bpm, t0, ms) >> 8) as u8 }
pub fn beatsin8(bpm: u16, lo: u8, hi: u8, t0: u32, ph: u8, ms: u32) -> u8 {
    let beat = beat8(bpm, t0, ms);
    let s = sin8(beat.wrapping_add(ph));
    lo + scale8(s, hi - lo)
}
```

---

## 9. Вывод: нужна ли палитровая система

**Ответ: да, портировать минимальную палитровую систему; вместе с HSV.** Обоснование:

- 8 из 9 полноэкранных noise-эффектов исходной прошивки рендерят через `ColorFromPalette(currentPalette, index, bri)` со стандартными палитрами (`RainbowColors_p`, `CloudColors_p`, `LavaColors_p`, `ForestColors_p`, `OceanColors_p`, `PartyColors_p`, `RainbowStripeColors_p`) — источник: `noise_effects.ino`. Без палитр эти эффекты не переносятся без переписывания.
- При этом `madnessNoise` и масса мелких эффектов работают напрямую с `CHSV`, так что HSV-конвертация нужна в любом случае. Это не «или-или».

**Что именно переносить** (источник: `src/colorutils.cpp`, `ColorFromPalette(const CRGBPalette16&, ...)`):

- `CRGBPalette16` = `[CRGB; 16]` (48 байт) — этого достаточно (варианты 32/256 не нужны).
- Алгоритм (LINEARBLEND): `hi4 = index >> 4`, `lo4 = index & 0x0F`; `f2 = lo4 << 4` (0..240), `f1 = 255 - f2`; результат `scale8(c1.ch, f1) + scale8(c2.ch, f2)` по каналам; сосед для hi4==15 — запись 0 (круговой заворот); при `lo4 == 0` — вернуть запись как есть.
- `NOBLEND`: всегда `pal[index >> 4]`. `LINEARBLEND_NOWRAP`: предварительно `index = map8(index, 0, 239)` — использовать только если нужны разрывные палитры; для портируемых эффектов хватает LINEARBLEND/NOBLEND (*вывод*).
- Яркостный параметр: если `brightness != 255` — `brightness += 1` и `scale8` (FIXED) по ненулевым каналам; если 0 — чёрный.
- Встроенные палитры — статические массивы 16×RGB из `src/colorpalettes.cpp` (там же их байты), копируются как константы.

```rust
pub fn color_from_palette(pal: &[(u8, u8, u8); 16], index: u8, bri: u8, blend: bool) -> (u8, u8, u8) {
    let (hi, lo) = ((index >> 4) as usize, index & 0x0F);
    let c1 = pal[hi];
    let mut c = if !blend || lo == 0 { c1 } else {
        let c2 = pal[(hi + 1) & 0x0F];
        let f2 = (lo << 4) as u16;               // 0..240
        let f1 = 255u16 - f2;
        (scale8(c1.0, f1 as u8) + scale8(c2.0, f2 as u8),
         scale8(c1.1, f1 as u8) + scale8(c2.1, f2 as u8),
         scale8(c1.2, f1 as u8) + scale8(c2.2, f2 as u8))
    };
    if bri != 255 {
        if bri == 0 { return (0, 0, 0); }
        let b = bri + 1;                          // adjust for rounding (FIXED)
        c.0 = scale8(c.0, b); c.1 = scale8(c.1, b); c.2 = scale8(c.2, b);
    }
    c
}
```

Сложность — около 60 строк плюс таблицы палитр; проще, чем изобретать замену, и даёт бит-в-бит одинаковую картинку с исходной прошивки (*вывод*).

---

## 10. Что НЕ портировать и подводные камни

- AVR-asm ветки, `*_LEAVING_R1_DIRTY`, `cleanup_R1`, `memmove8/memcpy8/memset8` — платформенно-специфичны (`lib8tion.cpp`, `scale8.h`).
- `applyGamma_video` — float (`colorutils.cpp`); для порта — LUT или отказ (см. §2).
- `scale8` только в FIXED-семантике `(+1)`; иначе `scale8(255,255)=254` и все fade/blend «завышают» затухание на 1.
- `random8()` — сумма байтов, не `>>8` (§5).
- beat-формулы — с обязательным wrapping-переполнением `u32` (§8).
- `inoise8`: индекс `A+1` может быть 256 — таблица перестановок 257 элементов (§7).
- Смешение `fadeToBlackBy` (plain) и `fadeLightBy` (video) даёт разный визуальный «шлейф» — в портируемых эффектах встречаются оба варианта поведения, выбирать по эффекту.

---

## Источники (все — первичные, raw.githubusercontent.com, тег 3.6.0 если не указано иное)

- scale8/scale8_video/nscale8x3(×3_video)/scale16/dim8: <https://raw.githubusercontent.com/FastLED/FastLED/3.6.0/src/lib8tion/scale8.h>
- qadd8/qsub8/qmul8/avg7/blend8: <https://raw.githubusercontent.com/FastLED/FastLED/3.6.0/src/lib8tion/math8.h>
- sin8/cos8/sin16/b_m16_interleave: <https://raw.githubusercontent.com/FastLED/FastLED/3.6.0/src/lib8tion/trig8.h>
- random8/random16/LCG/seed-механика: <https://raw.githubusercontent.com/FastLED/FastLED/3.6.0/src/lib8tion/random8.h>
- lerp8by8/map8/ease8InOutQuad/beat88/beat16/beat8/beatsin88/16/8/GET_MILLIS: <https://raw.githubusercontent.com/FastLED/FastLED/3.6.0/src/lib8tion.h>
- RAND16_SEED = 1337: <https://raw.githubusercontent.com/FastLED/FastLED/3.6.0/src/lib8tion.cpp>
- hsv2rgb_rainbow/hsv2rgb_spectrum/rgb2hsv_approximate: <https://raw.githubusercontent.com/FastLED/FastLED/3.6.0/src/hsv2rgb.cpp>
- inoise8/inoise16/таблица p[]/grad8/lerp7by8/fill_noise: <https://raw.githubusercontent.com/FastLED/FastLED/3.6.0/src/noise.cpp>
- blend/nblend/fadeToBlackBy/fade_video/blur1d/blur2d/ColorFromPalette/applyGamma_video/HeatColor: <https://raw.githubusercontent.com/FastLED/FastLED/3.6.0/src/colorutils.cpp>
- CRGB/CHSV, nscale8/nscale8_video/fadeToBlackBy-методы: <https://raw.githubusercontent.com/FastLED/FastLED/3.6.0/src/pixeltypes.h>
- Дефолты FASTLED_SCALE8_FIXED/FASTLED_BLEND_FIXED/FASTLED_NOISE_FIXED: <https://raw.githubusercontent.com/FastLED/FastLED/3.6.0/src/fastled_config.h>
- Сверка с 3.10.8: <https://raw.githubusercontent.com/FastLED/FastLED/3.10.8/src/fl/math/random8.h>, <https://raw.githubusercontent.com/FastLED/FastLED/3.10.8/src/fl/math/beat.h> (идентичны 3.6.0; в beat добавлен clamp), факт переезда файлов: <https://raw.githubusercontent.com/FastLED/FastLED/master/src/lib8tion/scale8.h> (форвардинг в `fl/math/`)
- Версия/дата релиза 3.10.8: GitHub API `repos/FastLED/FastLED/releases/latest`

**Отклонённые/деприоритизированные источники:** статьи и форки-«пересказы» алгоритмов FastLED (вторичны); `colorutils.h` 3.6.0 (типы палитр — взяты по использованию в `colorutils.cpp`, отдельного чтения не потребовалось).

## Contradictions
- Постановка задачи предполагала «sin8 — LUT 256 байт» и «fadeToBlackBy = scale8_video». По первичным исходникам верно обратное: sin8 — кусочно-линейное вычисление с 8-байтовой таблицей (`trig8.h`), fadeToBlackBy — plain `nscale8x3`, а video-вариант — это `fadeLightBy`/`fade_video` (`colorutils.cpp`, `pixeltypes.h`). Оба расхождения зафиксированы в §3 и §6.
- «INA/INC» не обнаружены в коде исходной прошивки — см. Missing evidence.

## Missing evidence
- Значение «INA/INC» из постановки задачи не верифицировано: таких идентификаторов нет в `noise_effects.ino`/`effects.ino` исходной прошивки v1.12/v1.13. Возможно, это константы из другого форка/статьи; типовые скорости даны в §7.
- Файлы hsv2rgb/noise/colorutils/scale8/trig8 в новом слое 3.10.8 (`src/fl/math/*`) не сверялись попозиционно — сверены только `random8.h` и `beat.h`. Исторически lib8tion-алгоритмы стабильны, но при желании можно допроверить.

## Next steps
- Допроверить `src/fl/math/trig8.h`, `src/fl/math/hsv2rgb*`, `src/fl/math/noise/*` в 3.10.8, если целевой ориентир — новейший FastLED, а не эпоха исходной прошивки.
- Уточнить у постановщика, что имелось в виду под INA/INC (какой репозиторий/файл).
