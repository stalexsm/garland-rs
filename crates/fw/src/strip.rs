//! Путь Кадра на Ленту: физический порядок по Привязке координат и
//! приведение цвета (гамма-коррекция, потолок яркости).
//!
//! Чистые функции без железа: время и Привязка приходят снаружи, поэтому
//! всё проверяется тестами на хосте.

use garland_core::{Frame, Mapping, Rgb8, gamma};

/// Максимальное значение цветового канала (из канона гаммы garland-core);
/// pub(crate): нужен диагностическому breathe для шкалы яркости.
pub(crate) const MAX_CHANNEL: u8 = 255;

/// Приведение цвета к Ленте: гамма-коррекция, затем сжатие до
/// `max_brightness` — ни один канал результата не превысит потолок.
pub fn encode(c: Rgb8, max_brightness: u8) -> Rgb8 {
    scale(gamma(c), max_brightness)
}

/// Перекладывает Кадр в буфер Ленты в физическом порядке: Пиксель (x, y)
/// встаёт в слот [`Mapping::strip_index`]. Длина `strip` обязана совпадать
/// с числом Пикселей `mapping`, размеры `frame` — с Матрицей `mapping`.
pub fn to_strip(frame: &Frame, mapping: &Mapping, strip: &mut [Rgb8]) {
    debug_assert_eq!(strip.len(), mapping.pixel_count());
    debug_assert_eq!(frame.len(), mapping.pixel_count());
    for (slot, pixel) in strip.iter_mut().enumerate() {
        let (x, y) = mapping.coord(slot);
        *pixel = frame.get(x, y).expect("размер Фреймбуфера = Матрице");
    }
}

/// Пропорциональное сжатие каналов цвета до уровня 0..=255.
pub(crate) fn scale(c: Rgb8, level: u8) -> Rgb8 {
    let channel = |v: u8| (u16::from(v) * u16::from(level) / u16::from(MAX_CHANNEL)) as u8;
    Rgb8 {
        r: channel(c.r),
        g: channel(c.g),
        b: channel(c.b),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use garland_core::{Corner, Layout};

    /// Цвет-метка в тестах порядка Ленты: белый виден на всех трёх каналах.
    const MARKER: Rgb8 = Rgb8 {
        r: 255,
        g: 255,
        b: 255,
    };

    /// Статичный цвет Прошивки для тестов гаммы: тёплый белый (канон diag).
    const WARM_WHITE: Rgb8 = Rgb8 {
        r: 255,
        g: 180,
        b: 120,
    };

    /// Приведение к Ленте: гамма, затем сжатие до потолка яркости.
    #[test]
    fn encode_applies_gamma_then_caps_brightness() {
        // Полный потолок — чистая гамма.
        assert_eq!(encode(WARM_WHITE, MAX_CHANNEL), gamma(WARM_WHITE));
        // Ни один канал не превышает потолок.
        for max in [1, 8, 64, 200] {
            let c = encode(WARM_WHITE, max);
            assert!(c.r <= max && c.g <= max && c.b <= max);
        }
        // Чёрный остаётся чёрным.
        assert_eq!(encode(Rgb8::BLACK, 64), Rgb8::BLACK);
    }

    /// Порядок Ленты физический: на змейке логический ряд 1 лежит на Ленте
    /// навстречу, Пиксель (0, 1) стоит под номером 7 (матрица 4×3).
    #[test]
    fn to_strip_applies_serpentine_order() {
        let mapping = Mapping::new(4, 3, Layout::RowSerpentine, Corner::TopLeft);
        let mut buffer = [Rgb8::BLACK; 12];
        let mut frame = Frame::new(4, 3, &mut buffer).unwrap();
        frame.set(0, 1, MARKER);

        let mut strip = [Rgb8::BLACK; 12];
        to_strip(&frame, &mapping, &mut strip);

        for (slot, pixel) in strip.iter().enumerate() {
            let expected = if slot == 7 { MARKER } else { Rgb8::BLACK };
            assert_eq!(*pixel, expected, "слот Ленты {slot}");
        }
    }

    /// Стартовый угол тоже применяется: при старте справа снизу логический
    /// (0, 0) лежит в конце Ленты.
    #[test]
    fn to_strip_applies_start_corner() {
        let mapping = Mapping::new(4, 3, Layout::RowPlain, Corner::BottomRight);
        let mut buffer = [Rgb8::BLACK; 12];
        let mut frame = Frame::new(4, 3, &mut buffer).unwrap();
        frame.set(0, 0, MARKER);

        let mut strip = [Rgb8::BLACK; 12];
        to_strip(&frame, &mapping, &mut strip);

        for (slot, pixel) in strip.iter().enumerate() {
            let expected = if slot == 11 { MARKER } else { Rgb8::BLACK };
            assert_eq!(*pixel, expected, "слот Ленты {slot}");
        }
    }
}
