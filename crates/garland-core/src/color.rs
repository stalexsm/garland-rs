//! Цветовая математика: RGB8/HSV8, перевод HSV → RGB, гамма-коррекция
//! и 16-битное альфа-смешение (канон кроссфейда WLED).
//!
//! Все функции чистые и целочисленные — без float, детерминированны и
//! одинаковы на хосте и на устройстве.

/// Максимальное значение цветового канала.
const MAX_CHANNEL: u16 = 255;

/// Шесть секторов цветового круга: тон делится на красный → зелёный → синий.
const HUE_STEPS: u16 = 6;

/// Длина сектора в единицах расширенного тона h·6.
const HUE_SECTOR: u16 = 256;

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

/// Перевод HSV → RGB (целочисленный, «спектровый» круг: шесть секторов по
/// 256 единиц расширенного тона h·6; круг не замкнут на 6 единиц — тон 255
/// почти красный). При `s = 0` — оттенок серого `v`, при `v = 0` — чёрный.
pub fn hsv_to_rgb(c: Hsv8) -> Rgb8 {
    // Расширенный тон h·6 ∈ 0..1530 делится на шесть секторов по 256 единиц:
    // сектор — какой из шести участков круга, f — позиция внутри сектора.
    let v = u16::from(c.v);
    let s = u16::from(c.s);
    let hue6 = u16::from(c.h) * HUE_STEPS;
    let sector = hue6 / HUE_SECTOR;
    let f = hue6 % HUE_SECTOR;

    // Доля `part` (0..255) от яркости v; промежуточные произведения ≤ 65025 —
    // помещаются в u16. p — минимум, q — ниже максимума на восходе сектора,
    // t — выше минимума.
    let part_of_v = |part: u16| v * part / MAX_CHANNEL;
    let p = v - part_of_v(s);
    let q = v - part_of_v(f * s / MAX_CHANNEL);
    let t = v - part_of_v((MAX_CHANNEL - f) * s / MAX_CHANNEL);

    let (r, g, b) = match sector {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    Rgb8 {
        r: r as u8,
        g: g as u8,
        b: b as u8,
    }
}

/// Таблица гаммы: значения round(255·(i/255)^2,8) для i = 0..255.
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
    fn hsv_primaries() {
        // Чистые тона: красный (0), зелёный (~120°), синий (~240°).
        assert_eq!(
            hsv_to_rgb(Hsv8 {
                h: 0,
                s: 255,
                v: 255
            }),
            Rgb8 { r: 255, g: 0, b: 0 }
        );
        assert_eq!(
            hsv_to_rgb(Hsv8 {
                h: 85,
                s: 255,
                v: 255
            }),
            Rgb8 { r: 1, g: 255, b: 0 }
        );
        assert_eq!(
            hsv_to_rgb(Hsv8 {
                h: 170,
                s: 255,
                v: 255
            }),
            Rgb8 { r: 0, g: 3, b: 255 }
        );
        // Круг замыкается: тон 255 — почти красный.
        assert_eq!(
            hsv_to_rgb(Hsv8 {
                h: 255,
                s: 255,
                v: 255
            }),
            Rgb8 { r: 255, g: 0, b: 5 }
        );
    }

    #[test]
    fn hsv_sector_boundaries() {
        // Границы секторов: переход красный → зелёный → синий → красный.
        assert_eq!(
            hsv_to_rgb(Hsv8 {
                h: 42,
                s: 255,
                v: 255
            }),
            Rgb8 {
                r: 255,
                g: 252,
                b: 0
            }
        );
        assert_eq!(
            hsv_to_rgb(Hsv8 {
                h: 43,
                s: 255,
                v: 255
            }),
            Rgb8 {
                r: 253,
                g: 255,
                b: 0
            }
        );
        assert_eq!(
            hsv_to_rgb(Hsv8 {
                h: 127,
                s: 255,
                v: 255
            }),
            Rgb8 {
                r: 0,
                g: 255,
                b: 250
            }
        );
        assert_eq!(
            hsv_to_rgb(Hsv8 {
                h: 128,
                s: 255,
                v: 255
            }),
            Rgb8 {
                r: 0,
                g: 255,
                b: 255
            }
        );
        assert_eq!(
            hsv_to_rgb(Hsv8 {
                h: 213,
                s: 255,
                v: 255
            }),
            Rgb8 {
                r: 254,
                g: 0,
                b: 255
            }
        );
        assert_eq!(
            hsv_to_rgb(Hsv8 {
                h: 214,
                s: 255,
                v: 255
            }),
            Rgb8 {
                r: 255,
                g: 0,
                b: 251
            }
        );
    }

    #[test]
    fn hsv_degenerate_saturation_and_value() {
        // s = 0 — оттенок серого независимо от тона.
        assert_eq!(
            hsv_to_rgb(Hsv8 {
                h: 90,
                s: 0,
                v: 200
            }),
            Rgb8 {
                r: 200,
                g: 200,
                b: 200
            }
        );
        // v = 0 — чёрный независимо от тона и насыщенности.
        assert_eq!(
            hsv_to_rgb(Hsv8 {
                h: 200,
                s: 255,
                v: 0
            }),
            Rgb8::BLACK
        );
    }

    /// Радуга без скачков: соседние тона отличаются по каждому каналу
    /// не больше чем на шаг сектора (6 единиц тона).
    #[test]
    fn hsv_rainbow_is_smooth() {
        let max_channel_step = 6;
        let mut prev = hsv_to_rgb(Hsv8 {
            h: 0,
            s: 255,
            v: 255,
        });
        for h in 1..=255u8 {
            let next = hsv_to_rgb(Hsv8 { h, s: 255, v: 255 });
            assert!(
                prev.r.abs_diff(next.r) <= max_channel_step,
                "r-скачок на тоне {h}"
            );
            assert!(
                prev.g.abs_diff(next.g) <= max_channel_step,
                "g-скачок на тоне {h}"
            );
            assert!(
                prev.b.abs_diff(next.b) <= max_channel_step,
                "b-скачок на тоне {h}"
            );
            prev = next;
        }
        // Склейка круга 255→0: тон 255 почти красный — канал b меняется на 5,
        // в пределах того же порога.
        let wrapped = hsv_to_rgb(Hsv8 {
            h: 0,
            s: 255,
            v: 255,
        });
        assert!(
            prev.r.abs_diff(wrapped.r) <= max_channel_step
                && prev.g.abs_diff(wrapped.g) <= max_channel_step
                && prev.b.abs_diff(wrapped.b) <= max_channel_step,
            "склейка круга 255→0: шаг в пределах порога"
        );
    }

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
