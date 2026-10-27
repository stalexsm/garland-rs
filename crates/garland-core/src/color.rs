//! Цветовая математика: RGB8/HSV8, гамма-коррекция и 16-битное
//! альфа-смешение (канон кроссфейда WLED). Конверсия HSV → RGB —
//! rainbow-вариант FastLED в модуле [`crate::fx`].
//!
//! Все функции чистые и целочисленные — без float, детерминированны и
//! одинаковы на хосте и на устройстве.

/// Цвет Пикселя в RGB888.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rgb8 {
    /// Красный канал.
    pub r: u8,
    /// Зелёный канал.
    pub g: u8,
    /// Синий канал.
    pub b: u8,
}

impl Rgb8 {
    /// Чёрный (выключенный) Пиксель.
    pub const BLACK: Rgb8 = Rgb8 { r: 0, g: 0, b: 0 };
}

/// Цвет в HSV: тон `h` — полный круг 0..255 (шаг ≈ 1,41°), насыщенность `s`
/// и яркость `v` — 0..255. Порядок тонов — красный → зелёный → синий.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Hsv8 {
    /// Тон.
    pub h: u8,
    /// Насыщенность.
    pub s: u8,
    /// Яркость.
    pub v: u8,
}

/// Полная непрозрачность второго цвета в 16-битной альфе смешения.
pub const MAX_ALPHA: u16 = u16::MAX;

static GAMMA_TABLE: [u8; 256] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // 0
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, // 16
    1, 1, 1, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 2, 2, 2, // 32
    2, 3, 3, 3, 3, 3, 3, 3, 4, 4, 4, 4, 4, 5, 5, 5, // 48
    5, 6, 6, 6, 6, 7, 7, 7, 7, 8, 8, 8, 9, 9, 9, 10, // 64
    10, 10, 11, 11, 11, 12, 12, 13, 13, 13, 14, 14, 15, 15, 16, 16, // 80
    17, 17, 18, 18, 19, 19, 20, 20, 21, 21, 22, 22, 23, 24, 24, 25, // 96
    25, 26, 27, 27, 28, 29, 29, 30, 31, 32, 32, 33, 34, 35, 35, 36, // 112
    37, 38, 39, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 50, // 128
    51, 52, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 64, 66, 67, 68, // 144
    69, 70, 72, 73, 74, 75, 77, 78, 79, 81, 82, 83, 85, 86, 87, 89, // 160
    90, 92, 93, 95, 96, 98, 99, 101, 102, 104, 105, 107, 109, 110, 112, 114, // 176
    115, 117, 119, 120, 122, 124, 126, 127, 129, 131, 133, 135, 137, 138, 140, 142, // 192
    144, 146, 148, 150, 152, 154, 156, 158, 160, 162, 164, 167, 169, 171, 173, 175, // 208
    177, 180, 182, 184, 186, 189, 191, 193, 196, 198, 200, 203, 205, 208, 210, 213, // 224
    215, 218, 220, 223, 225, 228, 231, 233, 236, 239, 241, 244, 247, 249, 252, 255, // 240
];

/// Гамма-коррекция одного канала: канон WS2812 с γ = 2,8 — таблица
/// `round(255·(i/255)^2,8)`, как в smart-leds/WLED.
pub fn gamma8(value: u8) -> u8 {
    GAMMA_TABLE[value as usize]
}

/// Гамма-коррекция цвета, поканально.
pub fn gamma(c: Rgb8) -> Rgb8 {
    Rgb8 {
        r: gamma8(c.r),
        g: gamma8(c.g),
        b: gamma8(c.b),
    }
}

/// 16-битное альфа-смешение: `alpha = 0` — первый цвет, [`MAX_ALPHA`] —
/// второй, середина — их полусумма; каналы смешиваются независимо
/// с округлением к ближайшему (канон `color_blend` WLED).
pub fn blend(c1: Rgb8, c2: Rgb8, alpha: u16) -> Rgb8 {
    // Взвешенная полусумма каналов: (a·(65535−α) + b·α + 32768) / 65535 —
    // округление к ближайшему; u32 вмещает 255·65535 с запасом.
    let mix = |a: u8, b: u8| {
        let weighted =
            u32::from(a) * u32::from(MAX_ALPHA - alpha) + u32::from(b) * u32::from(alpha);
        ((weighted + u32::from(MAX_ALPHA) / 2) / u32::from(MAX_ALPHA)) as u8
    };
    Rgb8 {
        r: mix(c1.r, c2.r),
        g: mix(c1.g, c2.g),
        b: mix(c1.b, c2.b),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gamma_endpoints_and_spots() {
        // Края таблицы обязательны: чёрное остаётся чёрным, полное — полным.
        assert_eq!(gamma8(0), 0);
        assert_eq!(gamma8(255), 255);
        // Контрольные точки таблицы round(255·(i/255)^2,8).
        assert_eq!(gamma8(64), 5);
        assert_eq!(gamma8(128), 37);
        assert_eq!(gamma8(192), 115);
    }

    /// Вся таблица сверяется с формулой round(255·(i/255)^2,8): ручные правки
    /// константы (транскрипция, потерянные значения) ломают этот тест целиком,
    /// а не только контрольные точки.
    #[test]
    fn gamma_table_matches_formula() {
        for i in 0..=255u16 {
            let expected = ((f64::from(i) / 255.0).powf(2.8) * 255.0).round() as u8;
            assert_eq!(gamma8(i as u8), expected, "gamma8({i})");
        }
    }

    #[test]
    fn gamma_is_monotonic() {
        let mut prev = gamma8(0);
        for i in 1..=255u8 {
            let current = gamma8(i);
            assert!(current >= prev, "гамма убывает на {i}");
            prev = current;
        }
    }

    #[test]
    fn gamma_applies_per_channel() {
        let color = Rgb8 {
            r: 128,
            g: 255,
            b: 0,
        };
        let expected = Rgb8 {
            r: gamma8(128),
            g: gamma8(255),
            b: gamma8(0),
        };
        assert_eq!(gamma(color), expected);
    }

    #[test]
    fn blend_endpoints() {
        let red = Rgb8 { r: 255, g: 0, b: 0 };
        let blue = Rgb8 { r: 0, g: 0, b: 255 };
        // Полная прозрачность — первый цвет, полная непрозрачность — второй.
        assert_eq!(blend(red, blue, 0), red);
        assert_eq!(blend(red, blue, MAX_ALPHA), blue);
    }

    #[test]
    fn blend_middle() {
        // Середина чёрного и белого — половинный серый.
        let mid = blend(
            Rgb8::BLACK,
            Rgb8 {
                r: 255,
                g: 255,
                b: 255,
            },
            32768,
        );
        assert_eq!(
            mid,
            Rgb8 {
                r: 128,
                g: 128,
                b: 128
            }
        );
        // Середина красного и синего: каждый канал — округлённый к ближайшему
        // вес своей доли, отсюда лёгкая асимметрия (127 против 128).
        let mid_rb = blend(
            Rgb8 { r: 255, g: 0, b: 0 },
            Rgb8 { r: 0, g: 0, b: 255 },
            32768,
        );
        assert_eq!(
            mid_rb,
            Rgb8 {
                r: 127,
                g: 0,
                b: 128
            }
        );
    }

    #[test]
    fn blend_same_color_is_stable() {
        let color = Rgb8 {
            r: 13,
            g: 200,
            b: 77,
        };
        for alpha in [0, 1, 12345, MAX_ALPHA] {
            assert_eq!(blend(color, color, alpha), color);
        }
    }

    /// Смешение монотонно по альфе: каждый канал движется от c1 к c2 без
    /// возвратов (канал, убывающий от c1 к c2, монотонно убывает).
    #[test]
    fn blend_is_monotonic_in_alpha() {
        let c1 = Rgb8 {
            r: 10,
            g: 250,
            b: 0,
        };
        let c2 = Rgb8 {
            r: 240,
            g: 0,
            b: 130,
        };
        let mut prev = blend(c1, c2, 0);
        for alpha in 1..=MAX_ALPHA {
            let current = blend(c1, c2, alpha);
            assert!(current.r >= prev.r && current.g <= prev.g && current.b >= prev.b);
            prev = current;
        }
    }
}
