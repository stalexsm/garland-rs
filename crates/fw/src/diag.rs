//! Кадры Прошивки: улыбающийся смайл на старте, статичный цвет и циклы
//! blink/breathe для гейта драйвера.
//!
//! Чистые функции без железа: геометрия — через [`Frame`] и [`Mapping`],
//! время приходит снаружи, поэтому всё проверяется тестами на хосте.

use garland_core::{Frame, Mapping, Rgb8, gamma};

/// Статичный цвет Прошивки: тёплый белый.
pub const STATIC_COLOR: Rgb8 = Rgb8 {
    r: 255,
    g: 180,
    b: 120,
};

/// Полупериод blink (вкл/выкл), мс: 2,5 вспышки в секунду.
pub const BLINK_HALF_MS: u32 = 200;

/// Период дыхания (спад + подъём), мс.
pub const BREATHE_PERIOD_MS: u32 = 2000;

/// Цвет лица смайла: жёлтый.
const SMILE_FACE: Rgb8 = Rgb8 {
    r: 255,
    g: 255,
    b: 0,
};

/// Сторона спрайта смайла, Пикселей.
const SMILE_SIDE: u16 = 8;

/// Спрайт смайла [`SMILE_SIDE`]×[`SMILE_SIDE`], строки сверху вниз: `true` —
/// жёлтый Пиксель лица, `false` — тёмный (контур, глаза, улыбка).
const SMILE_SPRITE: [[bool; SMILE_SIDE as usize]; SMILE_SIDE as usize] = [
    [false, false, true, true, true, true, false, false],
    [false, true, true, true, true, true, true, false],
    [true, true, false, true, true, false, true, true],
    [true, true, true, true, true, true, true, true],
    [true, false, true, true, true, true, false, true],
    [true, true, false, false, false, false, true, true],
    [false, true, true, true, true, true, true, false],
    [false, false, true, true, true, true, false, false],
];

/// Рисует спрайт смайла левым верхним углом в (x0, y0); Пиксели за
/// пределами Фреймбуфера срезаются.
fn blit_smile(frame: &mut Frame, x0: u16, y0: u16) {
    for (row, line) in SMILE_SPRITE.iter().enumerate() {
        for (col, lit) in line.iter().enumerate() {
            if *lit {
                frame.set(x0 + col as u16, y0 + row as u16, SMILE_FACE);
            }
        }
    }
}

/// Координаты глаз в спрайте (столбец, строка): тёмные Пиксели лица,
/// которые при моргании вспыхивают белым.
const SMILE_EYES: [(u16, u16); 2] = [(2, 2), (5, 2)];

/// Цвет глаз при моргании: белый.
const SMILE_BLINK_COLOR: Rgb8 = Rgb8 {
    r: 255,
    g: 255,
    b: 255,
};

/// Перерыв между морганиями, мс: смайл моргает раз в три секунды.
const SMILE_BLINK_PERIOD_MS: u32 = 3000;

/// Сколько миллисекунд глаза остаются закрытыми.
const SMILE_BLINK_MS: u32 = 200;

/// Кадр приветствия на старте в момент `elapsed_ms`: жёлтый улыбающийся
/// смайл по центру Матрицы, глаза и улыбка — тёмные Пиксели лица. Раз в
/// [`SMILE_BLINK_PERIOD_MS`] глаза на [`SMILE_BLINK_MS`] вспыхивают белым.
/// Прошивка держит смайл `SMILE_SECS` секунд, затем включает Гирлянду.
pub fn smile(frame: &mut Frame, mapping: &Mapping, elapsed_ms: u32) {
    frame.fill(Rgb8::BLACK);
    let x0 = mapping.width().saturating_sub(SMILE_SIDE) / 2;
    let y0 = mapping.height().saturating_sub(SMILE_SIDE) / 2;
    blit_smile(frame, x0, y0);
    let eyes_closed = elapsed_ms % SMILE_BLINK_PERIOD_MS < SMILE_BLINK_MS;
    if eyes_closed {
        // Глаза — дырки в спрайте: при моргании вспыхивают белым.
        for (ex, ey) in SMILE_EYES {
            frame.set(x0 + ex, y0 + ey, SMILE_BLINK_COLOR);
        }
    }
}

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

    /// Смайл на Стенде 8×8 занимает Матрицу целиком: жёлтое лицо, глаза и
    /// улыбка — тёмные Пиксели (спрайт от руки — независимая проверка).
    #[test]
    fn smile_fills_the_stand() {
        let mapping = Mapping::new(8, 8, Layout::RowSerpentine, Corner::TopLeft);
        let mut buffer = [Rgb8::BLACK; 64];
        let mut frame = Frame::new(8, 8, &mut buffer).unwrap();

        // Момент с открытыми глазами — окно совпадает со спрайтом целиком.
        smile(&mut frame, &mapping, SMILE_BLINK_MS);

        let expected = [
            "..YYYY..", ".YYYYYY.", "YY.YY.YY", "YYYYYYYY", "Y.YYYY.Y", "YY....YY", ".YYYYYY.",
            "..YYYY..",
        ];
        let face = Rgb8 {
            r: 255,
            g: 255,
            b: 0,
        };
        for y in 0..8u16 {
            for x in 0..8u16 {
                let want = if expected[y as usize].as_bytes()[x as usize] == b'Y' {
                    face
                } else {
                    Rgb8::BLACK
                };
                assert_eq!(frame.get(x, y), Some(want), "Пиксель ({x}, {y})");
            }
        }
    }

    /// На Матрице крупнее Стенда смайл встаёт по центру: поле в один Пиксель
    /// со всех сторон.
    #[test]
    fn smile_centers_on_a_bigger_matrix() {
        let mapping = Mapping::new(10, 10, Layout::RowSerpentine, Corner::TopLeft);
        let mut buffer = [Rgb8::BLACK; 100];
        let mut frame = Frame::new(10, 10, &mut buffer).unwrap();

        // Момент с открытыми глазами.
        smile(&mut frame, &mapping, SMILE_BLINK_MS);

        let face = Rgb8 {
            r: 255,
            g: 255,
            b: 0,
        };
        // Угол Матрицы тёмный, верх лба спрайта (строка 0, столбцы 2..5) —
        // со сдвигом на поле.
        assert_eq!(frame.get(0, 0), Some(Rgb8::BLACK));
        assert_eq!(frame.get(3, 1), Some(face));
        assert_eq!(frame.get(2, 1), Some(Rgb8::BLACK));
        // Широкая улыбка (строка 5 спрайта, столбцы 2..5) — тёмная.
        assert_eq!(frame.get(4, 6), Some(Rgb8::BLACK));
        // Правый нижний угол тоже остаётся полем.
        assert_eq!(frame.get(9, 9), Some(Rgb8::BLACK));
    }

    /// Смайл моргает: в начале цикла глаза горят белым, улыбка на месте;
    /// потом глаза открываются (тёмные Пиксели); с новым циклом вспыхивают
    /// снова.
    #[test]
    fn smile_blinks_eyes() {
        let mapping = Mapping::new(8, 8, Layout::RowSerpentine, Corner::TopLeft);
        let mut buffer = [Rgb8::BLACK; 64];
        let mut frame = Frame::new(8, 8, &mut buffer).unwrap();

        // Моргание: глаза горят белым, улыбка не тронута.
        let white = Rgb8 {
            r: 255,
            g: 255,
            b: 255,
        };
        smile(&mut frame, &mapping, 0);
        assert_eq!(frame.get(2, 2), Some(white));
        assert_eq!(frame.get(5, 2), Some(white));
        assert_eq!(frame.get(3, 5), Some(Rgb8::BLACK));

        // Открытые глаза: те же Пиксели тёмные.
        smile(&mut frame, &mapping, SMILE_BLINK_MS);
        assert_eq!(frame.get(2, 2), Some(Rgb8::BLACK));
        assert_eq!(frame.get(5, 2), Some(Rgb8::BLACK));

        // Новый цикл — глаза снова вспыхивают белым.
        smile(&mut frame, &mapping, SMILE_BLINK_PERIOD_MS);
        assert_eq!(frame.get(2, 2), Some(white));
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
