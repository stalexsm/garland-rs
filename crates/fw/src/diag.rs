//! Диагностические кадры Прошивки: тестовая сетка Раскладки на старте,
//! статичный цвет и циклы blink/breathe для гейта драйвера.
//!
//! Чистые функции без железа: геометрия — через [`Frame`] и [`Mapping`],
//! время приходит снаружи, поэтому всё проверяется тестами на хосте.

use garland_core::{Frame, Mapping, Rgb8, gamma};

/// Цвет Пикселя 0: горит всю фазу сетки и отмечает стартовый угол.
const GRID_ORIGIN: Rgb8 = Rgb8 { r: 255, g: 0, b: 0 };

/// Белый цвет бегунка: видны все три канала Ленты.
const GRID_RUNNER: Rgb8 = Rgb8 {
    r: 255,
    g: 255,
    b: 255,
};

/// Сколько миллисекунд бегунок горит на одной позиции Ленты: полный проход
/// Стенда 8×8 занимает около 4 с.
const GRID_STEP_MS: u32 = 60;

/// Статичный цвет Прошивки: тёплый белый — цвет тестовой сетки Раскладки.
pub const STATIC_COLOR: Rgb8 = Rgb8 {
    r: 255,
    g: 180,
    b: 120,
};

/// Полупериод blink (вкл/выкл), мс: 2,5 вспышки в секунду.
pub const BLINK_HALF_MS: u32 = 200;

/// Период дыхания (спад + подъём), мс.
pub const BREATHE_PERIOD_MS: u32 = 2000;

/// Цвет blink в момент `elapsed_ms`: полный цвет в первой половине периода,
/// чёрный во второй, затем цикл повторяется.
pub fn blink(elapsed_ms: u32) -> Rgb8 {
    if elapsed_ms % (2 * BLINK_HALF_MS) < BLINK_HALF_MS {
        STATIC_COLOR
    } else {
        Rgb8::BLACK
    }
}

/// Цвет breathe в момент `elapsed_ms`: линейный треугольник яркости — от
/// чёрного к полному цвету к чёрному за период.
pub fn breathe(elapsed_ms: u32) -> Rgb8 {
    let phase = elapsed_ms % BREATHE_PERIOD_MS;
    let half = BREATHE_PERIOD_MS / 2;
    let distance = if phase <= half {
        phase
    } else {
        BREATHE_PERIOD_MS - phase
    };
    let level = (distance * u32::from(MAX_CHANNEL) / half) as u8;
    scale(STATIC_COLOR, level)
}

/// Максимальное значение цветового канала (из канона гаммы garland-core).
const MAX_CHANNEL: u8 = 255;

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
fn scale(c: Rgb8, level: u8) -> Rgb8 {
    let channel = |v: u8| (u16::from(v) * u16::from(level) / u16::from(MAX_CHANNEL)) as u8;
    Rgb8 {
        r: channel(c.r),
        g: channel(c.g),
        b: channel(c.b),
    }
}

/// Кадр тестовой сетки Раскладки в момент `elapsed_ms` от старта фазы:
/// белый бегунок идёт по Ленте и показывает порядок змейки, красный Пиксель 0
/// весь цикл держит стартовый угол.
pub fn test_grid(frame: &mut Frame, mapping: &Mapping, elapsed_ms: u32) {
    frame.fill(Rgb8::BLACK);
    let (x, y) = mapping.coord(0);
    frame.set(x, y, GRID_ORIGIN);

    let count = mapping.pixel_count() as u32;
    let position = elapsed_ms / GRID_STEP_MS % count;
    let (x, y) = mapping.coord(position as usize);
    frame.set(x, y, GRID_RUNNER);
}

#[cfg(test)]
mod tests {
    use super::*;
    use garland_core::{Corner, Layout};

    /// Матрица 4×3, змейка по рядам, старт слева сверху.
    fn mapping() -> Mapping {
        Mapping::new(4, 3, Layout::RowSerpentine, Corner::TopLeft)
    }

    #[test]
    fn grid_marks_origin_pixel_red() {
        let mapping = mapping();
        let mut buffer = [Rgb8::BLACK; 12];
        let mut frame = Frame::new(4, 3, &mut buffer).unwrap();

        // Пиксель 0 при этой Раскладке — (0, 0); бегунок уже ушёл на позицию 1,
        // и красная отметка стартового угла видна.
        test_grid(&mut frame, &mapping, GRID_STEP_MS);

        assert_eq!(frame.get(0, 0), Some(Rgb8 { r: 255, g: 0, b: 0 }));
    }

    /// Белый бегунок движется по Ленте: на первой позиции он ещё на Пикселе 0
    /// (перекрывает красный), через шаг — на соседнем Пикселе 1.
    #[test]
    fn grid_runner_walks_the_strip() {
        let mapping = mapping();
        let mut buffer = [Rgb8::BLACK; 12];
        let mut frame = Frame::new(4, 3, &mut buffer).unwrap();

        test_grid(&mut frame, &mapping, 0);
        assert_eq!(
            frame.get(0, 0),
            Some(Rgb8 {
                r: 255,
                g: 255,
                b: 255
            })
        );

        test_grid(&mut frame, &mapping, GRID_STEP_MS);
        assert_eq!(
            frame.get(1, 0),
            Some(Rgb8 {
                r: 255,
                g: 255,
                b: 255
            })
        );
        assert_eq!(frame.get(0, 0), Some(GRID_ORIGIN));
    }

    /// На развёрнутом ряду змейки бегунок идёт навстречу: Пиксель 5 матрицы
    /// 4×3 стоит в (2, 1) — второй ряд проходится справа налево.
    #[test]
    fn grid_runner_follows_serpentine_reversal() {
        let mapping = mapping();
        let mut buffer = [Rgb8::BLACK; 12];
        let mut frame = Frame::new(4, 3, &mut buffer).unwrap();

        test_grid(&mut frame, &mapping, 5 * GRID_STEP_MS);
        assert_eq!(
            frame.get(2, 1),
            Some(Rgb8 {
                r: 255,
                g: 255,
                b: 255
            })
        );
    }

    /// Пройдя Ленту до конца, бегунок начинается с Пикселя 0 заново.
    #[test]
    fn grid_runner_wraps_around_the_strip() {
        let mapping = mapping();
        let mut buffer = [Rgb8::BLACK; 12];
        let mut frame = Frame::new(4, 3, &mut buffer).unwrap();

        test_grid(&mut frame, &mapping, 12 * GRID_STEP_MS);
        assert_eq!(
            frame.get(0, 0),
            Some(Rgb8 {
                r: 255,
                g: 255,
                b: 255
            })
        );
    }

    /// Blink: первая половина периода — полный цвет, вторая — чёрный,
    /// далее цикл повторяется.
    #[test]
    fn blink_alternates_on_and_off() {
        assert_eq!(blink(0), STATIC_COLOR);
        assert_eq!(blink(BLINK_HALF_MS - 1), STATIC_COLOR);
        assert_eq!(blink(BLINK_HALF_MS), Rgb8::BLACK);
        assert_eq!(blink(2 * BLINK_HALF_MS - 1), Rgb8::BLACK);
        assert_eq!(blink(2 * BLINK_HALF_MS), STATIC_COLOR);
    }

    /// Сумма каналов — грубая мера яркости цвета для сравнений в тестах.
    fn brightness_of(c: Rgb8) -> u32 {
        u32::from(c.r) + u32::from(c.g) + u32::from(c.b)
    }

    /// Breathe: от чёрного к полному цвету к чёрному за период;
    /// подъём и спад симметричны, яркость меняется монотонно.
    #[test]
    fn breathe_rises_and_falls() {
        assert_eq!(breathe(0), Rgb8::BLACK);
        assert_eq!(breathe(BREATHE_PERIOD_MS / 2), STATIC_COLOR);
        assert_eq!(breathe(BREATHE_PERIOD_MS), Rgb8::BLACK);

        assert_eq!(breathe(250), breathe(BREATHE_PERIOD_MS - 250));
        assert!(brightness_of(breathe(250)) < brightness_of(breathe(500)));
        assert!(brightness_of(breathe(500)) < brightness_of(breathe(750)));
    }

    /// Приведение к Ленте: гамма, затем сжатие до потолка яркости.
    #[test]
    fn encode_applies_gamma_then_caps_brightness() {
        // Полный потолок — чистая гамма.
        assert_eq!(encode(STATIC_COLOR, MAX_CHANNEL), gamma(STATIC_COLOR));
        // Ни один канал не превышает потолок.
        for max in [1, 8, 64, 200] {
            let c = encode(STATIC_COLOR, max);
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
        frame.set(0, 1, GRID_RUNNER);

        let mut strip = [Rgb8::BLACK; 12];
        to_strip(&frame, &mapping, &mut strip);

        for (slot, pixel) in strip.iter().enumerate() {
            let expected = if slot == 7 { GRID_RUNNER } else { Rgb8::BLACK };
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
        frame.set(0, 0, GRID_RUNNER);

        let mut strip = [Rgb8::BLACK; 12];
        to_strip(&frame, &mapping, &mut strip);

        for (slot, pixel) in strip.iter().enumerate() {
            let expected = if slot == 11 { GRID_RUNNER } else { Rgb8::BLACK };
            assert_eq!(*pixel, expected, "слот Ленты {slot}");
        }
    }
}
