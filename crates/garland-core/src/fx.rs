//! Примитивы математики эффектов экосистемы FastLED: насыщающая
//! арифметика 8 бит, детерминированный ГПСЧ, синус, Перлин-шум,
//! палитры и rainbow-конверсия HSV.
//!
//! Точные целочисленные порты алгоритмов FastLED 3.6.0 (семантика
//! `FASTLED_SCALE8_FIXED`, `FASTLED_BLEND_FIXED`, `FASTLED_NOISE_FIXED`) —
//! см. `docs/research/fastled-primitives.md`. Всё чисто целочисленное,
//! без float: одинаково на хосте и устройстве.

use crate::color::{Hsv8, Rgb8};
use crate::frame::Frame;

// ============ Насыщающая арифметика 8 бит ============

/// `scale8` FastLED (`src/lib8tion/scale8.h`, семантика FIXED):
/// доля `scale`/256 от `i`, то есть `i·(scale+1)>>8`.
pub fn scale8(i: u8, scale: u8) -> u8 {
    ((u16::from(i) * (u16::from(scale) + 1)) >> 8) as u8
}

/// `scale8_video` FastLED: как [`scale8`], но ненулевое не гаснет в ноль —
/// к результату добавляется 1, если оба аргумента ненулевые.
pub fn scale8_video(i: u8, scale: u8) -> u8 {
    (((u16::from(i) * u16::from(scale)) >> 8) as u8) + u8::from(i != 0 && scale != 0)
}

/// `qadd8` FastLED: насыщающее сложение байтов.
pub fn qadd8(i: u8, j: u8) -> u8 {
    i.saturating_add(j)
}

/// `qsub8` FastLED: насыщающее вычитание байтов.
pub fn qsub8(i: u8, j: u8) -> u8 {
    i.saturating_sub(j)
}

/// `dim8_raw` FastLED: приближение гаммы 2.0, `scale8(x, x)`.
pub fn dim8_raw(x: u8) -> u8 {
    scale8(x, x)
}

/// `fadeToBlackBy` FastLED (`nscale8x3`): уменьшить яркость цвета на долю
/// `fade_by`/256 — ненулевой цвет может уйти в чёрный.
pub fn fade_to_black(c: Rgb8, fade_by: u8) -> Rgb8 {
    let scale = 255 - fade_by;
    Rgb8 {
        r: scale8(c.r, scale),
        g: scale8(c.g, scale),
        b: scale8(c.b, scale),
    }
}

/// `fadePixel` исходных прошивок (новый фейдер): чёрный не трогается; пока
/// какой-то канал дошёл до порога, цвет гаснет на [`fade_to_black`], а
/// ниже порога гаснет мгновенно — хвосты комет не «тлеют» вечно.
pub fn fade_pixel(c: Rgb8, step: u8) -> Rgb8 {
    if c == Rgb8::BLACK {
        return c;
    }
    /// Порог канала, ниже которого хвост гаснет сразу, а не по долям.
    const FADE_FLOOR: u8 = 30;
    if c.r >= FADE_FLOOR || c.g >= FADE_FLOOR || c.b >= FADE_FLOOR {
        fade_to_black(c, step)
    } else {
        Rgb8::BLACK
    }
}

/// `ease8InOutQuad` FastLED (`src/lib8tion.h`): квадратичное сглаживание
/// краёв — используется шумом Перлина для интерполяции внутри ячейки.
pub fn ease8_in_out_quad(i: u8) -> u8 {
    let mut j = i;
    if j & 0x80 != 0 {
        j = 255 - j;
    }
    let mut jj2 = scale8(j, j) << 1;
    if i & 0x80 != 0 {
        jj2 = 255 - jj2;
    }
    jj2
}

// ============ Детерминированный ГПСЧ ============

/// 16-битный ГПСЧ FastLED (`src/lib8tion/random8.h`): LCG
/// `X(n+1) = X(n)·2053 + 13849 (mod 2^16)`. Фиксированный сид даёт
/// одинаковые кадры на хосте и устройстве — детерминизм Эффекта.
pub struct Rng16 {
    seed: u16,
}

impl Rng16 {
    /// Множитель LCG.
    const MUL: u16 = 2053;
    /// Приращение LCG.
    const ADD: u16 = 13849;
    /// Дефолтный сид FastLED (`RAND16_SEED`).
    const DEFAULT_SEED: u16 = 1337;

    /// ГПСЧ с дефолтным сидом FastLED.
    pub fn new() -> Self {
        Self {
            seed: Self::DEFAULT_SEED,
        }
    }

    /// Очередное 16-битное число.
    pub fn random16(&mut self) -> u16 {
        self.seed = self.seed.wrapping_mul(Self::MUL).wrapping_add(Self::ADD);
        self.seed
    }

    /// Очередной байт: сумма младшего и старшего байтов seeds — смешение
    /// FastLED («better mixing»), не старший байт.
    pub fn random8(&mut self) -> u8 {
        let s = self.random16();
        (s as u8).wrapping_add((s >> 8) as u8)
    }

    /// Байт `0..lim` (верхняя граница исключена), как `random8(lim)`.
    pub fn random8_below(&mut self, lim: u8) -> u8 {
        ((u16::from(self.random8()) * u16::from(lim)) >> 8) as u8
    }

    /// Байт `min..lim` (верхняя граница исключена), как `random8(min, lim)`.
    pub fn random8_range(&mut self, min: u8, lim: u8) -> u8 {
        self.random8_below(lim - min) + min
    }

    /// 16-битное `0..lim` (верхняя граница исключена), как `random16(lim)`.
    pub fn random16_below(&mut self, lim: u16) -> u16 {
        ((u32::from(self.random16()) * u32::from(lim)) >> 16) as u16
    }
}

impl Default for Rng16 {
    fn default() -> Self {
        Self::new()
    }
}

// ============ Синус ============

/// Наклоны кусочно-линейной аппроксимации синуса (`b_m16_interleave`,
/// `src/lib8tion/trig8.h`): пары «сдвиг, наклон» четырёх секций четверти.
const SIN_SLOPES: [u8; 8] = [0, 49, 49, 41, 90, 27, 117, 10];

/// `sin8` FastLED: синус угла `theta` (полный круг — 256 единиц),
/// переведённый в байт `0..255` (ноль синуса — 128).
pub fn sin8(theta: u8) -> u8 {
    let mut offset = theta;
    if theta & 0x40 != 0 {
        offset = 255 - offset;
    }
    offset &= 0x3F;
    let mut sec_offset = u16::from(offset & 0x0F);
    if theta & 0x40 != 0 {
        sec_offset += 1;
    }
    let section = usize::from(offset >> 4);
    let b = u16::from(SIN_SLOPES[section * 2]);
    let m16 = u16::from(SIN_SLOPES[section * 2 + 1]);
    let mx = (m16 * sec_offset) >> 4;
    let mut y = (mx + b) as i8;
    if theta & 0x80 != 0 {
        y = -y;
    }
    (i16::from(y) + 128) as u8
}

// ============ HSV → RGB (rainbow) ============

/// `hsv2rgb_rainbow` FastLED (`src/hsv2rgb.cpp`): основная конверсия
/// экосистемы — цветовой круг разбит на 8 секций по 32 единицы тона с
/// кусочно-линейными «полками» яркости (жёлтый намеренно ярче). Все
/// эффекты экосистемы окрашены именно через неё — порт бит-в-бит.
pub fn hsv2rgb_rainbow(c: Hsv8) -> Rgb8 {
    /// Опорные уровни градиента секций (`K255/K171/K170/K85`).
    const K171: u8 = 171;
    const K170: u8 = 170;
    const K85: u8 = 85;

    // Позиция внутри секции, растянутая в 8 раз (0..248): доля градиента.
    let offset8 = (u16::from(c.h & 0x1F)) << 3;
    let third = scale8(offset8 as u8, K85);
    let two_thirds = scale8(offset8 as u8, K171);

    let (mut r, mut g, mut b) = match c.h >> 5 {
        0 => (255 - third, third, 0),
        1 => (K171, K85 + third, 0),
        2 => (K171 - two_thirds, K170 + third, 0),
        3 => (0, 255 - third, third),
        4 => (0, K171 - two_thirds, K85 + two_thirds),
        5 => (third, 0, 255 - third),
        6 => (K85 + third, 0, K171 - third),
        _ => (K170 + third, 0, K85 - third),
    };

    if c.s != 255 {
        if c.s == 0 {
            // Нулевая насыщенность — серый полной яркости; яркость ниже
            // по-прежнему применяется.
            r = 255;
            g = 255;
            b = 255;
        } else {
            // Квадратичное приглушение насыщенности и подъём «пола».
            let desat = scale8_video(255 - c.s, 255 - c.s);
            let satscale = 255 - desat;
            r = scale8(r, satscale);
            g = scale8(g, satscale);
            b = scale8(b, satscale);
            r = r.saturating_add(desat);
            g = g.saturating_add(desat);
            b = b.saturating_add(desat);
        }
    }

    if c.v != 255 {
        let v = scale8_video(c.v, c.v);
        if v == 0 {
            return Rgb8::BLACK;
        }
        r = scale8(r, v);
        g = scale8(g, v);
        b = scale8(b, v);
    }

    Rgb8 { r, g, b }
}

// ============ Перлин-шум ============

/// Таблица перестановок Перлина (`p[]` из `src/noise.cpp`): 256 значений
/// плюс дубликат первого — соседний узел `P(i+1)` читается без заворота.
const PERM: [u8; 257] = [
    151, 160, 137, 91, 90, 15, 131, 13, 201, 95, 96, 53, 194, 233, 7, 225, //
    140, 36, 103, 30, 69, 142, 8, 99, 37, 240, 21, 10, 23, 190, 6, 148, //
    247, 120, 234, 75, 0, 26, 197, 62, 94, 252, 219, 203, 117, 35, 11, 32, //
    57, 177, 33, 88, 237, 149, 56, 87, 174, 20, 125, 136, 171, 168, 68, 175, //
    74, 165, 71, 134, 139, 48, 27, 166, 77, 146, 158, 231, 83, 111, 229, 122, //
    60, 211, 133, 230, 220, 105, 92, 41, 55, 46, 245, 40, 244, 102, 143, 54, //
    65, 25, 63, 161, 1, 216, 80, 73, 209, 76, 132, 187, 208, 89, 18, 169, //
    200, 196, 135, 130, 116, 188, 159, 86, 164, 100, 109, 198, 173, 186, 3, 64, //
    52, 217, 226, 250, 124, 123, 5, 202, 38, 147, 118, 126, 255, 82, 85, 212, //
    207, 206, 59, 227, 47, 16, 58, 17, 182, 189, 28, 42, 223, 183, 170, 213, //
    119, 248, 152, 2, 44, 154, 163, 70, 221, 153, 101, 155, 167, 43, 172, 9, //
    129, 22, 39, 253, 19, 98, 108, 110, 79, 113, 224, 232, 178, 185, 112, 104, //
    218, 246, 97, 228, 251, 34, 242, 193, 238, 210, 144, 12, 191, 179, 162, 241, //
    81, 51, 145, 235, 249, 14, 239, 107, 49, 192, 214, 31, 181, 199, 106, 157, //
    184, 84, 204, 176, 115, 121, 50, 45, 127, 4, 150, 254, 138, 236, 205, 93, //
    222, 114, 67, 29, 24, 72, 243, 141, 128, 195, 78, 66, 215, 61, 156, 180, //
    151,
];

/// Хэш угла ячейки: `P(i)`; сосед читается с индексом+1 — таблица 257.
fn perm(i: u8) -> u8 {
    PERM[usize::from(i)]
}

/// Хэш соседнего угла: `P(i+1)` в арифметике C (без заворота байта).
fn perm_next(i: u8) -> u8 {
    PERM[usize::from(i) + 1]
}

/// `avg7` FastLED (`src/lib8tion/math8.h`): среднее байтов со знаком
/// с заворачивающей арифметикой — градиент вклада угла ячейки.
fn avg7(i: i8, j: i8) -> i8 {
    (i >> 1).wrapping_add(j >> 1).wrapping_add(i & 1)
}

/// `grad8` FastLED (`src/noise.cpp`): вклад градиента угла по хэшу.
fn grad8(hash: u8, x: i8, y: i8, z: i8) -> i8 {
    let hash = hash & 15;
    let mut u = if hash < 8 { x } else { y };
    let mut v = if hash < 4 {
        y
    } else if hash == 12 || hash == 14 {
        x
    } else {
        z
    };
    if hash & 1 != 0 {
        u = u.wrapping_neg();
    }
    if hash & 2 != 0 {
        v = v.wrapping_neg();
    }
    avg7(u, v)
}

/// `lerp7by8` FastLED: линейная интерполяция байтов со знаком. Разность
/// считается в 16 битах: крайние значения градиента дают до 255.
fn lerp7by8(a: i8, b: i8, frac: u8) -> i8 {
    if b > a {
        let delta = (i16::from(b) - i16::from(a)) as u8;
        (i16::from(a) + i16::from(scale8(delta, frac))) as i8
    } else {
        let delta = (i16::from(a) - i16::from(b)) as u8;
        (i16::from(a) - i16::from(scale8(delta, frac))) as i8
    }
}

/// `inoise8` FastLED (`src/noise.cpp`): 8-битный градиентный шум Перлина,
/// координаты — 16-битные (старший байт — номер ячейки, младший — позиция
/// внутри), выход — байт 0..255.
pub fn inoise8(x: u16, y: u16, z: u16) -> u8 {
    // Номера ячеек и хэши углов: вся арифметика — байты с заворотом,
    // соседние углы читаются через [`perm_next`] (без заворота индекса).
    let (xb, yb, zb) = ((x >> 8) as u8, (y >> 8) as u8, (z >> 8) as u8);
    let a = perm(xb).wrapping_add(yb);
    let aa = perm(a).wrapping_add(zb);
    let ab = perm_next(a).wrapping_add(zb);
    let b = perm_next(xb).wrapping_add(yb);
    let ba = perm(b).wrapping_add(zb);
    let bb = perm_next(b).wrapping_add(zb);

    // Дробные позиции: сглаженные доли и знаковые координаты градиента
    // (сдвиг и маска снимают знак, N = -128 — шаг к соседнему узлу).
    let ease = |t: u16| ease8_in_out_quad(t as u8);
    let (u, v, w) = (ease(x), ease(y), ease(z));
    let (xx, yy, zz) = (
        ((x as u8) >> 1 & 0x7F) as i8,
        ((y as u8) >> 1 & 0x7F) as i8,
        ((z as u8) >> 1 & 0x7F) as i8,
    );
    let n: i8 = -0x80;

    let x1 = lerp7by8(
        grad8(perm(aa), xx, yy, zz),
        grad8(perm(ba), xx.wrapping_sub(n), yy, zz),
        u,
    );
    let x2 = lerp7by8(
        grad8(perm(ab), xx, yy.wrapping_sub(n), zz),
        grad8(perm(bb), xx.wrapping_sub(n), yy.wrapping_sub(n), zz),
        u,
    );
    let x3 = lerp7by8(
        grad8(perm_next(aa), xx, yy, zz.wrapping_sub(n)),
        grad8(perm_next(ba), xx.wrapping_sub(n), yy, zz.wrapping_sub(n)),
        u,
    );
    let x4 = lerp7by8(
        grad8(perm_next(ab), xx, yy.wrapping_sub(n), zz.wrapping_sub(n)),
        grad8(
            perm_next(bb),
            xx.wrapping_sub(n),
            yy.wrapping_sub(n),
            zz.wrapping_sub(n),
        ),
        u,
    );
    let y1 = lerp7by8(x1, x2, v);
    let y2 = lerp7by8(x3, x4, v);
    // Свёртка даёт −64..+64; сдвиг в ноль и удвоение с насыщением — 0..255.
    let ans = lerp7by8(y1, y2, w);
    let n = ans.wrapping_add(64) as u8;
    qadd8(n, n)
}

// ============ Палитры ============

/// `CRGBPalette16` FastLED: 16 опорных цветов; цвет по индексу 0..255 —
/// кусочно-линейный градиент между соседними опорами (LINEARBLEND).
#[derive(Clone, Copy, Debug)]
pub struct Palette16 {
    entries: [Rgb8; 16],
}

impl Palette16 {
    /// Палитра из 16 опорных цветов.
    pub const fn new(entries: [Rgb8; 16]) -> Self {
        Self { entries }
    }

    /// `ColorFromPalette` FastLED (`src/colorutils.cpp`, LINEARBLEND):
    /// старший полубайт индекса выбирает пару опор, младший — долю
    /// смешения; `brightness` гасит ненулевые каналы (0 — чёрный).
    pub fn color(&self, index: u8, brightness: u8) -> Rgb8 {
        let hi = usize::from(index >> 4);
        let lo = index & 0x0F;
        let c1 = self.entries[hi];
        let c = if lo == 0 {
            c1
        } else {
            let c2 = self.entries[(hi + 1) & 0x0F];
            // Доли смешения: f2 — доля второй опоры, f1 — первой (в сумме 255).
            let f2 = u16::from(lo) << 4;
            let f1 = 255 - f2;
            let mix = |a: u8, b: u8| scale8(a, f1 as u8).saturating_add(scale8(b, f2 as u8));
            Rgb8 {
                r: mix(c1.r, c2.r),
                g: mix(c1.g, c2.g),
                b: mix(c1.b, c2.b),
            }
        };
        if brightness == 255 {
            return c;
        }
        if brightness == 0 {
            return Rgb8::BLACK;
        }
        // Поправка округления FastLED: яркость +1 перед масштабированием.
        let b = brightness + 1;
        Rgb8 {
            r: scale8(c.r, b),
            g: scale8(c.g, b),
            b: scale8(c.b, b),
        }
    }
}

/// Красный байт из литерала 0xRRGGBB — палитры заданы байтами исходников.
const fn rgb24(value: u32) -> Rgb8 {
    Rgb8 {
        r: (value >> 16) as u8,
        g: (value >> 8) as u8,
        b: value as u8,
    }
}

/// `RainbowColors_p` FastLED — «Радужные переливы».
pub const RAINBOW_PALETTE: Palette16 = Palette16::new([
    rgb24(0xFF0000),
    rgb24(0xD52A00),
    rgb24(0xAB5500),
    rgb24(0xAB7F00),
    rgb24(0xABAB00),
    rgb24(0x56D500),
    rgb24(0x00FF00),
    rgb24(0x00D52A),
    rgb24(0x00AB55),
    rgb24(0x0056AA),
    rgb24(0x0000FF),
    rgb24(0x2A00D5),
    rgb24(0x5500AB),
    rgb24(0x7F0081),
    rgb24(0xAB0055),
    rgb24(0xD5002B),
]);

/// `CloudColors_p` FastLED — «Облака» (HTML-имена цветов из исходника).
pub const CLOUD_PALETTE: Palette16 = Palette16::new([
    rgb24(0x0000FF), // Blue
    rgb24(0x00008B), // DarkBlue
    rgb24(0x00008B),
    rgb24(0x00008B),
    rgb24(0x00008B),
    rgb24(0x00008B),
    rgb24(0x00008B),
    rgb24(0x00008B),
    rgb24(0x0000FF), // Blue
    rgb24(0x00008B), // DarkBlue
    rgb24(0x87CEEB), // SkyBlue
    rgb24(0x87CEEB), // SkyBlue
    rgb24(0xADD8E6), // LightBlue
    rgb24(0xFFFFFF), // White
    rgb24(0xADD8E6), // LightBlue
    rgb24(0x87CEEB), // SkyBlue
]);

/// `LavaColors_p` FastLED — «Лава».
pub const LAVA_PALETTE: Palette16 = Palette16::new([
    rgb24(0x000000), // Black
    rgb24(0x800000), // Maroon
    rgb24(0x000000), // Black
    rgb24(0x800000), // Maroon
    rgb24(0x8B0000), // DarkRed
    rgb24(0x8B0000), // DarkRed
    rgb24(0x800000), // Maroon
    rgb24(0x8B0000), // DarkRed
    rgb24(0x8B0000), // DarkRed
    rgb24(0x8B0000), // DarkRed
    rgb24(0xFF0000), // Red
    rgb24(0xFFA500), // Orange
    rgb24(0xFFFFFF), // White
    rgb24(0xFFA500), // Orange
    rgb24(0xFF0000), // Red
    rgb24(0x8B0000), // DarkRed
]);

/// `PartyColors_p` FastLED — «Плазма».
pub const PARTY_PALETTE: Palette16 = Palette16::new([
    rgb24(0x5500AB),
    rgb24(0x84007C),
    rgb24(0xB5004B),
    rgb24(0xE5001B),
    rgb24(0xE81700),
    rgb24(0xB84700),
    rgb24(0xAB7700),
    rgb24(0xABAB00),
    rgb24(0xAB5500),
    rgb24(0xDD2200),
    rgb24(0xF2000E),
    rgb24(0xC2003E),
    rgb24(0x8F0071),
    rgb24(0x5F00A1),
    rgb24(0x2F00D0),
    rgb24(0x0007F9),
]);

// ============ Шумовое поле ============

/// Максимальная сторона Матрицы для шумовых эффектов: поле квадратное
/// `side × side` байт, при 64 — 4 КиБ на экземпляр эффекта (жив всегда
/// ровно один экземпляр Текущего эффекта).
pub const NOISE_MAX_SIDE: u16 = 64;

/// Движок шумовых эффектов (`fillNoiseLED`, Noise + Palette):
/// квадратное поле 8-битного шума Перлина с дрейфом координат, сглаживанием
/// во времени и окраской через палитру. Эффект владеет полем и каждый Кадр
/// делает `tick` (шаг физики) и `render` (окраска в Фреймбуфер).
pub struct NoiseField {
    /// Сторона квадратного поля: max(ширина, высота) Матрицы.
    side: u16,
    /// Поле: `data[row * side + col]`.
    data: [u8; (NOISE_MAX_SIDE * NOISE_MAX_SIDE) as usize],
    /// Координаты шума: дрейфуют каждый такт.
    x: u16,
    y: u16,
    z: u16,
    /// Смещение тона палитры (colorLoop).
    hue: u8,
}

impl NoiseField {
    /// Поле для Матрицы `w`×`h`; `None`, если сторона больше
    /// [`NOISE_MAX_SIDE`] — такая Матрица не поддерживается шумом.
    pub fn new(w: u16, h: u16) -> Option<Self> {
        let side = w.max(h);
        if side > NOISE_MAX_SIDE {
            return None;
        }
        Some(Self {
            side,
            data: [0; (NOISE_MAX_SIDE * NOISE_MAX_SIDE) as usize],
            x: 0,
            y: 0,
            z: 0,
            hue: 0,
        })
    }

    /// Шаг физики: пересчитать поле (`fillnoise8` + сглаживание
    /// `fillNoiseLED`) и продрейфовать координаты (`z += speed`,
    /// `x += speed/8`, `y -= speed/16`), сдвинуть тон палитры.
    pub fn tick(&mut self, scale: u16, speed: u16) {
        // Сглаживание во времени: при медленном движении поле тянется к
        // новым значениям лишь частично (доля 256−sm от нового значения).
        let smoothing: u16 = if speed < 50 { 200 - speed * 4 } else { 0 };
        let side = usize::from(self.side);
        for row in 0..side {
            let x_offset = scale.wrapping_mul(row as u16);
            for col in 0..side {
                let y_offset = scale.wrapping_mul(col as u16);
                let mut value = inoise8(
                    self.x.wrapping_add(x_offset),
                    self.y.wrapping_add(y_offset),
                    self.z,
                );
                value = qsub8(value, 16);
                value = qadd8(value, scale8(value, 39));
                if smoothing != 0 {
                    let old = self.data[row * side + col];
                    let inv = (256 - smoothing) as u8;
                    value = scale8(old, smoothing as u8).saturating_add(scale8(value, inv));
                }
                self.data[row * side + col] = value;
            }
        }
        self.z = self.z.wrapping_add(speed);
        self.x = self.x.wrapping_add(speed / 8);
        self.y = self.y.wrapping_sub(speed / 16);
        self.hue = self.hue.wrapping_add(1);
    }

    /// Окрасить Кадр (`fillNoiseLED`): тон Пикселя — поле, прочитанное
    /// транспонированно (радужная рябь), яркость — то же поле напрямую;
    /// тёмная половина гасится квадратично, светлая горит полной.
    pub fn render(&self, palette: &Palette16, color_loop: bool, frame: &mut Frame<'_>) {
        let side = usize::from(self.side);
        let (w, h) = (usize::from(frame.width()), usize::from(frame.height()));
        for ix in 0..w {
            for iy in 0..h {
                let index = self.data[iy * side + ix];
                let bri_raw = self.data[ix * side + iy];
                let bri = if bri_raw > 127 {
                    255
                } else {
                    dim8_raw(bri_raw.wrapping_mul(2))
                };
                let index = if color_loop {
                    index.wrapping_add(self.hue)
                } else {
                    index
                };
                frame.set(ix as u16, iy as u16, palette.color(index, bri));
            }
        }
    }
}

// ============ Такты эффектов ============

/// Потолок догоняющих тактов: нормальный рендер опаздывает на такт-два;
/// пауза в кадрах дольше [`CATCHUP_LIMIT`] тактов считается сбросом шкалы
/// (долгий Покой, пауза планировщика) — эффект продолжит с текущего кадра,
/// не копя многочасовую задолженность.
pub const CATCHUP_LIMIT: u32 = 64;

/// Число тактов эффекта между кадрами `prev_ms` и `now_ms` при интервале
/// `tick_ms`: пошаговые эффекты шагают на все пропущенные такты разом;
/// забег больше [`CATCHUP_LIMIT`] тактов даёт ноль — шкала сброшена.
/// `tick_ms` — не ноль.
pub fn steps_since(prev_ms: u32, now_ms: u32, tick_ms: u32) -> u32 {
    let steps = now_ms.wrapping_sub(prev_ms) / tick_ms;
    if steps <= CATCHUP_LIMIT { steps } else { 0 }
}

/// Новый якорь тактов после кадра: граница последнего догнанного такта —
/// сетка тактов не плывёт от неровных кадров; при сбросе шкалы (забег
/// больше [`CATCHUP_LIMIT`]) — сам `now_ms`. Результат — следующий
/// `prev_ms` для [`steps_since`].
pub fn advance_anchor(prev_ms: u32, now_ms: u32, tick_ms: u32) -> u32 {
    let steps = now_ms.wrapping_sub(prev_ms) / tick_ms;
    if steps <= CATCHUP_LIMIT {
        prev_ms.wrapping_add(steps.wrapping_mul(tick_ms))
    } else {
        now_ms
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Hsv8;

    /// `scale8` в семантике FIXED: полная шкала не съедает единицу.
    #[test]
    fn scale8_landmarks() {
        assert_eq!(scale8(255, 255), 255);
        assert_eq!(scale8(255, 0), 0);
        assert_eq!(scale8(0, 255), 0);
        // (255·129)>>8 = 128
        assert_eq!(scale8(255, 128), 128);
        // (128·129)>>8 = 64
        assert_eq!(scale8(128, 128), 64);
    }

    /// `scale8_video` — это тождество при полной шкале и ноль при нуле.
    #[test]
    fn scale8_video_keeps_nonzero() {
        assert_eq!(scale8_video(200, 255), 200);
        assert_eq!(scale8_video(0, 255), 0);
        assert_eq!(scale8_video(123, 0), 0);
    }

    #[test]
    fn saturating_add_sub() {
        assert_eq!(qadd8(250, 10), 255);
        assert_eq!(qadd8(10, 20), 30);
        assert_eq!(qsub8(5, 10), 0);
        assert_eq!(qsub8(10, 4), 6);
    }

    /// `fadeToBlackBy`: 200 на 70/256 → 145, 100 → 72, 50 → 36 (ручные
    /// выкладки (канал·186)>>8).
    #[test]
    fn fade_to_black_scales_channels() {
        let faded = fade_to_black(
            Rgb8 {
                r: 200,
                g: 100,
                b: 50,
            },
            70,
        );
        assert_eq!(
            faded,
            Rgb8 {
                r: 145,
                g: 72,
                b: 36
            }
        );
    }

    /// Фейдер хвостов: чёрный не трогается, тусклые хвосты гаснут сразу,
    /// яркие — по долям.
    #[test]
    fn fade_pixel_floors_dim_tails() {
        assert_eq!(fade_pixel(Rgb8::BLACK, 70), Rgb8::BLACK);
        assert_eq!(
            fade_pixel(
                Rgb8 {
                    r: 10,
                    g: 10,
                    b: 10
                },
                70
            ),
            Rgb8::BLACK
        );
        assert_ne!(fade_pixel(Rgb8 { r: 200, g: 5, b: 5 }, 70), Rgb8::BLACK);
    }

    /// `ease8InOutQuad`: края тождественны, середина — выпуклость.
    #[test]
    fn ease8_landmarks() {
        assert_eq!(ease8_in_out_quad(0), 0);
        assert_eq!(ease8_in_out_quad(255), 255);
        assert_eq!(ease8_in_out_quad(128), 129);
    }

    /// `sin8`: ноль синуса — 128, пик в четверти — 255, спад — 1.
    #[test]
    fn sin8_landmarks() {
        assert_eq!(sin8(0), 128);
        assert_eq!(sin8(64), 255);
        assert_eq!(sin8(128), 128);
        assert_eq!(sin8(192), 1);
    }

    /// LCG FastLED детерминирован: первый вывод сидa 1337 — 6198, байт —
    /// сумма байтов слова (22+24).
    #[test]
    fn rng16_is_deterministic() {
        let mut rng = Rng16::new();
        assert_eq!(rng.random16(), 6198);
        let mut rng = Rng16::new();
        assert_eq!(rng.random8(), 78);
        // Одинаковые сиды — одинаковые последовательности.
        let (mut a, mut b) = (Rng16::new(), Rng16::new());
        for _ in 0..16 {
            assert_eq!(a.random16(), b.random16());
        }
    }

    /// Диапазонные варианты ГПСЧ не выходят за границу (исключающую).
    #[test]
    fn rng16_respects_limits() {
        let mut rng = Rng16::new();
        for _ in 0..256 {
            assert!(rng.random8_below(10) < 10);
            assert!((3..7).contains(&rng.random8_range(3, 7)));
            assert!(rng.random16_below(1000) < 1000);
        }
    }

    /// `hsv2rgb_rainbow`: опорные точки секций и «полки» яркости — цвета
    /// совпадают с FastLED, включая нечистые 171/85 на границах секций.
    #[test]
    fn hsv2rgb_rainbow_landmarks() {
        let full = |h: u8| hsv2rgb_rainbow(Hsv8 { h, s: 255, v: 255 });
        assert_eq!(full(0), Rgb8 { r: 255, g: 0, b: 0 });
        assert_eq!(
            full(64),
            Rgb8 {
                r: 171,
                g: 170,
                b: 0
            }
        );
        assert_eq!(full(96), Rgb8 { r: 0, g: 255, b: 0 });
        // Аква в rainbow-круге FastLED — (0,171,85), не бирюзовый.
        assert_eq!(
            full(128),
            Rgb8 {
                r: 0,
                g: 171,
                b: 85
            }
        );
        assert_eq!(full(255), Rgb8 { r: 253, g: 0, b: 2 });
    }

    /// Нулевая насыщенность — серый, яркость применима; нулевая яркость —
    /// чёрный в любой тональности.
    #[test]
    fn hsv2rgb_rainbow_sat_and_val() {
        let white = hsv2rgb_rainbow(Hsv8 {
            h: 42,
            s: 0,
            v: 255,
        });
        assert_eq!(
            white,
            Rgb8 {
                r: 255,
                g: 255,
                b: 255
            }
        );
        // v=128 → scale8_video(128,128)=65; канал 255 → (255·66)>>8 = 65.
        let gray = hsv2rgb_rainbow(Hsv8 {
            h: 42,
            s: 0,
            v: 128,
        });
        assert_eq!(
            gray,
            Rgb8 {
                r: 65,
                g: 65,
                b: 65
            }
        );
        assert_eq!(
            hsv2rgb_rainbow(Hsv8 {
                h: 200,
                s: 255,
                v: 0
            }),
            Rgb8::BLACK
        );
    }

    /// `inoise8` в начале координат — середина шкалы (выкладка по таблице
    /// перестановок), вывод детерминирован и повторяется.
    #[test]
    fn inoise8_is_deterministic() {
        assert_eq!(inoise8(0, 0, 0), 128);
        assert_eq!(inoise8(1000, 2000, 3000), inoise8(1000, 2000, 3000));
    }

    /// Палитра: целые индексы дают опоры, промежуточные — смесь; яркость
    /// 255 тождественна, 0 — чёрный, 254 после поправки округления — полная.
    #[test]
    fn palette16_color_landmarks() {
        assert_eq!(RAINBOW_PALETTE.color(0, 255), Rgb8 { r: 255, g: 0, b: 0 });
        // Индекс 128 — опора 8 (0x00AB55).
        assert_eq!(
            RAINBOW_PALETTE.color(128, 255),
            Rgb8 {
                r: 0,
                g: 171,
                b: 85
            }
        );
        // Индекс 8: половина между 0x FF0000 и 0xD52A00 → (234, 21, 0).
        assert_eq!(
            RAINBOW_PALETTE.color(8, 255),
            Rgb8 {
                r: 234,
                g: 21,
                b: 0
            }
        );
        assert_eq!(RAINBOW_PALETTE.color(0, 0), Rgb8::BLACK);
        assert_eq!(RAINBOW_PALETTE.color(0, 254), Rgb8 { r: 255, g: 0, b: 0 });
        assert_eq!(RAINBOW_PALETTE.color(0, 128), Rgb8 { r: 129, g: 0, b: 0 });
    }

    /// Поле шумов: рендер заполняет Кадр, шаг физики меняет картинку,
    /// одинаковые поля рисуют бит-в-бит одинаковые Кадры.
    #[test]
    fn noise_field_renders_and_drifts() {
        let mut frame_a = [Rgb8::BLACK; 16];
        let mut frame_b = [Rgb8::BLACK; 16];
        let mut field = NoiseField::new(4, 4).unwrap();
        field.tick(30, 20);
        field.render(
            &RAINBOW_PALETTE,
            true,
            &mut Frame::new(4, 4, &mut frame_a).unwrap(),
        );
        // Кадр заполнен не мусором: все Пиксели перезаписаны рендером.
        assert!(frame_a.iter().any(|c| *c != Rgb8::BLACK));
        // После дрейфа картинка изменилась.
        field.tick(30, 20);
        field.render(
            &RAINBOW_PALETTE,
            true,
            &mut Frame::new(4, 4, &mut frame_b).unwrap(),
        );
        assert_ne!(frame_a, frame_b);
        // Детерминизм: новое поле теми же шагами даёт тот же Кадр.
        let mut twin = NoiseField::new(4, 4).unwrap();
        twin.tick(30, 20);
        twin.tick(30, 20);
        let mut frame_c = [Rgb8::BLACK; 16];
        twin.render(
            &RAINBOW_PALETTE,
            true,
            &mut Frame::new(4, 4, &mut frame_c).unwrap(),
        );
        assert_eq!(frame_b, frame_c);
    }

    /// Поле крупнее потолка не собирается — конструктор отказывает.
    #[test]
    fn noise_field_rejects_oversize() {
        assert!(NoiseField::new(64, 8).is_some());
        assert!(NoiseField::new(65, 8).is_none());
    }

    /// Такты: нормальный темп считается точно, до лимита включительно;
    /// прыжок назад (заворот u32) или долгая пауза — сброс шкалы (ноль).
    #[test]
    fn steps_since_counts_ticks() {
        assert_eq!(steps_since(0, 80, 80), 1);
        assert_eq!(steps_since(0, 240, 80), 3);
        assert_eq!(steps_since(80, 80, 80), 0);
        assert_eq!(
            steps_since(0, CATCHUP_LIMIT * 80, 80),
            CATCHUP_LIMIT,
            "до лимита включительно такты догоняются"
        );
        assert_eq!(
            steps_since(0, (CATCHUP_LIMIT + 1) * 80, 80),
            0,
            "забег сверх лимита — сброс шкалы"
        );
        assert_eq!(steps_since(1000, 5, 80), 0, "прыжок назад — сброс");
    }

    /// Якорь тактов: на границе догнанного такта (сетка не плывёт), сквозь
    /// заворот u32; при сбросе шкалы — на сам момент кадра.
    #[test]
    fn anchor_sticks_to_tick_grid_and_resets() {
        assert_eq!(advance_anchor(0, 100, 80), 80);
        assert_eq!(advance_anchor(0, 160, 80), 160);
        assert_eq!(advance_anchor(80, 240, 80), 240);
        assert_eq!(advance_anchor(u32::MAX - 80, 40, 80), u32::MAX);
        assert_eq!(
            advance_anchor(0, (CATCHUP_LIMIT + 1) * 80, 80),
            (CATCHUP_LIMIT + 1) * 80,
            "сброс шкалы — якорь на момент кадра"
        );
    }
}
