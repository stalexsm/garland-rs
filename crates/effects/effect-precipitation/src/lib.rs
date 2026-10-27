//! Аддон «Дождь и снег»: падающие с верха Матрицы частицы.
//!
//! 2D-Эффект: частицы падают по своим столбцам, у каждой — хвост в пол
//! яркости. Медленные частицы белые (снег), быстрые — голубые (дождь).
//! Столбец, скорость и стартовый сдвиг каждой частицы выведены из её номера,
//! время приходит снаружи: кадры детерминированны и проверяются тестами
//! на хосте.

#![no_std]

use garland_core::{Effect, Frame, Rgb8};

/// Долей на Пиксель: позиции частиц считаются в 1/256 Пикселя, чтобы падение
/// шло плавно без float — целочисленно и одинаково на хосте и устройстве.
const SUBPIXEL: u32 = 256;

/// Свободных строк в цикле частицы: выйдя с низа Матрицы, частица появляется
/// сверху не сразу, а пройдя ещё `GAP_ROWS` строк невидимого пути.
const GAP_ROWS: u32 = 2;

/// Скорости падения в долях Пикселя за миллисекунду, по кругу на номер
/// частицы: смесь медленного снега и быстрых струй дождя.
const SPEEDS_SUB_MS: [u32; 4] = [1, 3, 2, 5];

/// Порог скорости: частицы быстрее — капли дождя (голубые), медленнее —
/// снежинки (белые).
const RAIN_SPEED_SUB_MS: u32 = 4;

/// Частиц на столбец — три на четыре столбца Матрицы (числитель дроби
/// `w·3/4`; на узкой Матрице — минимум одна).
const PARTICLES_NUMERATOR: u32 = 3;
const PARTICLES_DENOMINATOR: u32 = 4;

/// Цвет снежинки: белый с холодным отливом.
const SNOW_HEAD: Rgb8 = Rgb8 {
    r: 235,
    g: 240,
    b: 255,
};

/// Цвет капли дождя: голубой.
const RAIN_HEAD: Rgb8 = Rgb8 {
    r: 120,
    g: 170,
    b: 255,
};

/// Константа рассева столбцов: нечётное умножение и сдвиг перемешивают
/// номер частицы (константа Кнута) — детерминированно, без генератора
/// случайных чисел.
const SCATTER_MULTIPLIER: u64 = 2_654_435_761;
const SCATTER_SHIFT: u32 = 13;

/// Дождь и снег.
pub struct Precipitation;

/// Столбец частицы `i` из `columns` столбцов: нечётное умножение и сдвиг
/// перемешивают номер частицы (константа Кнута) — детерминированный рассев
/// без генератора случайных чисел.
fn scatter_column(i: u32, columns: u32) -> u16 {
    let scattered = (u64::from(i) * SCATTER_MULTIPLIER) >> SCATTER_SHIFT;
    (scattered % u64::from(columns)) as u16
}

impl Effect for Precipitation {
    fn render(&mut self, frame: &mut Frame<'_>, elapsed_ms: u32) {
        if frame.is_empty() {
            return;
        }
        // Контракт Эффекта — заполнить Кадр целиком: частицы занимают лишь
        // отдельные Пиксели, остальное гасится (в Прошивке Фреймбуферы
        // переиспользуются между Кадрами).
        frame.fill(Rgb8::BLACK);
        let w = u32::from(frame.width());
        let h = u32::from(frame.height());
        // Цикл частицы: весь путь по вертикали, включая паузу-«перерождение»
        // в [`GAP_ROWS`] строк; позиция — остаток пройденного пути в долях
        // Пикселя. Произведение скорости на время — в u64: шкала времени
        // wrapping, переполнения u32 не будет никогда.
        let span = (h + GAP_ROWS) * SUBPIXEL;
        let count = (w * PARTICLES_NUMERATOR / PARTICLES_DENOMINATOR).max(1);
        for i in 0..count {
            let speed = SPEEDS_SUB_MS[(i % SPEEDS_SUB_MS.len() as u32) as usize];
            let column = scatter_column(i, w);
            let offset = i * span / count;
            let fallen =
                (u64::from(speed) * u64::from(elapsed_ms) + u64::from(offset)) % u64::from(span);
            let row = (fallen / u64::from(SUBPIXEL)) as u16;
            if row < frame.height() {
                let head = if speed >= RAIN_SPEED_SUB_MS {
                    RAIN_HEAD
                } else {
                    SNOW_HEAD
                };
                frame.set(column, row, head);
                if row > 0 {
                    frame.set(column, row - 1, dim(head));
                }
            }
        }
    }
}

/// Хвост частицы: тот же цвет в половинной яркости.
fn dim(c: Rgb8) -> Rgb8 {
    Rgb8 {
        r: c.r / 2,
        g: c.g / 2,
        b: c.b / 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use garland_core::Rgb8;

    /// Цвет снежинки — литерал теста, независимый от константы кода.
    const SNOW: Rgb8 = Rgb8 {
        r: 235,
        g: 240,
        b: 255,
    };

    /// Цвет капли дождя — литерал теста, независимый от константы кода.
    const RAIN: Rgb8 = Rgb8 {
        r: 120,
        g: 170,
        b: 255,
    };

    /// Первый Кадр: частица 0 стартует с нулевого сдвига пути, её голова —
    /// в левом столбце верхней строки, снежного цвета.
    #[test]
    fn first_particle_starts_at_the_top_left() {
        let mut precipitation = Precipitation;
        let mut buffer = [Rgb8::BLACK; 64];
        let mut frame = Frame::new(8, 8, &mut buffer).unwrap();

        precipitation.render(&mut frame, 0);

        assert_eq!(frame.get(0, 0), Some(SNOW));
    }

    /// Частицы падают: за `SUBPIXEL` мс частица 0 (скорость 1 доля/мс)
    /// опускается ровно на одну строку, хвост догоняет в пол яркости.
    #[test]
    fn particles_fall_down_and_drag_a_tail() {
        let mut precipitation = Precipitation;
        let mut buffer = [Rgb8::BLACK; 64];
        let mut frame = Frame::new(8, 8, &mut buffer).unwrap();

        precipitation.render(&mut frame, SUBPIXEL);

        assert_eq!(frame.get(0, 1), Some(SNOW), "голова опустилась на строку");
        assert_eq!(frame.get(0, 0), Some(dim(SNOW)), "хвост — пол яркости");
    }

    /// Быстрые частицы — капли дождя: частица 3 (скорость из таблицы выше
    /// порога) рисуется голубым, с голубым хвостом — в ранний момент,
    /// когда соседних частиц в её столбце рядом нет.
    #[test]
    fn fast_particles_rain_blue() {
        let mut precipitation = Precipitation;
        let mut buffer = [Rgb8::BLACK; 64];
        let mut frame = Frame::new(8, 8, &mut buffer).unwrap();
        // Частица 3: скорость 5 долей/мс, стартовый сдвиг — половина цикла
        // (строка 5 на старте); за 10 мс она едва сползла, хвост над ней.
        let column = scatter_column(3, 8);

        precipitation.render(&mut frame, 10);

        assert_eq!(frame.get(column, 5), Some(RAIN));
        assert_eq!(frame.get(column, 4), Some(dim(RAIN)));
    }

    /// Палитра замкнута: в любом кадре только чёрный, два цвета голов
    /// и их полояркостные хвосты — никакой другой цвет не просачивается.
    #[test]
    fn colors_come_only_from_the_palette() {
        let palette = [Rgb8::BLACK, SNOW, dim(SNOW), RAIN, dim(RAIN)];
        let mut precipitation = Precipitation;
        for elapsed_ms in [0, 400, 1_300, 4_321] {
            let mut buffer = [Rgb8::BLACK; 64];
            let mut frame = Frame::new(8, 8, &mut buffer).unwrap();

            precipitation.render(&mut frame, elapsed_ms);

            for pixel in frame.pixels() {
                assert!(
                    palette.contains(pixel),
                    "мс {elapsed_ms}: {pixel:?} вне палитры"
                );
            }
        }
    }

    /// Падение идёт: кадры трёх моментов попарно различаются — снегопад
    /// не застыл и не топчется на месте.
    #[test]
    fn frames_advance_over_time() {
        let mut precipitation = Precipitation;
        let mut early = [Rgb8::BLACK; 64];
        let mut mid = [Rgb8::BLACK; 64];
        let mut late = [Rgb8::BLACK; 64];

        precipitation.render(&mut Frame::new(8, 8, &mut early).unwrap(), 0);
        precipitation.render(&mut Frame::new(8, 8, &mut mid).unwrap(), 1_000);
        precipitation.render(&mut Frame::new(8, 8, &mut late).unwrap(), 2_000);

        assert_ne!(early, mid);
        assert_ne!(mid, late);
        assert_ne!(early, late);
    }

    /// Эффект — чистая функция времени: новые экземпляры на один и тот же
    /// момент рисуют бит в бит одинаковый кадр.
    #[test]
    fn same_time_same_frame() {
        let mut first = Precipitation;
        let mut second = Precipitation;
        let mut left = [Rgb8::BLACK; 64];
        let mut right = [Rgb8::BLACK; 64];

        first.render(&mut Frame::new(8, 8, &mut left).unwrap(), 999);
        second.render(&mut Frame::new(8, 8, &mut right).unwrap(), 999);

        assert_eq!(left, right);
    }

    /// Кадр заполняется целиком: мусор от прошлых Кадров (Фреймбуферы
    /// Прошивки переиспользуются) не переживает рендер.
    #[test]
    fn dirty_buffer_is_cleared() {
        let mut precipitation = Precipitation;
        let mut buffer = [Rgb8 { r: 255, g: 0, b: 0 }; 64];
        let mut frame = Frame::new(8, 8, &mut buffer).unwrap();

        precipitation.render(&mut frame, 999);

        assert!(
            frame
                .pixels()
                .iter()
                .all(|c| c.r != 255 || c.g != 0 || c.b != 0)
        );
    }

    /// Одиночный столбец — допустимая Матрица: одна частица, падает по нему
    /// без паники и не пустит чёрный кадр.
    #[test]
    fn single_column_matrix_still_rains() {
        let mut precipitation = Precipitation;
        let mut buffer = [Rgb8::BLACK; 3];
        let mut frame = Frame::new(1, 3, &mut buffer).unwrap();

        precipitation.render(&mut frame, 100);

        assert!(frame.pixels().iter().any(|c| *c != Rgb8::BLACK));
    }

    /// Вырожденный пустой Кадр (0×0) не паникует.
    #[test]
    fn empty_frame_is_a_noop() {
        let mut precipitation = Precipitation;
        let mut buffer = [];
        let mut frame = Frame::new(0, 0, &mut buffer).unwrap();

        precipitation.render(&mut frame, 0);
    }
}
