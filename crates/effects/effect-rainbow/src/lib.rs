//! Аддон «Радуга по проводу»: полный круг тона разложен по Ленте и бежит
//! по проводу.
//!
//! 1D-Эффект: Кадр пишется только через обход «по проводу»
//! ([`Frame::set_wire`]), поэтому физически радуга течёт вдоль Ленты —
//! на Матрице картинка следует Раскладке (змейке). Саму Привязку координат
//! Эффект не вычисляет — её передаёт Прошивка при создании. Время приходит
//! снаружи, кадры детерминированны.

#![no_std]

use garland_core::{Effect, Frame, Hsv8, Mapping, hsv_to_rgb};

/// Период пробега полного круга тона, мс: радуга оборачивается за 4 с.
const PERIOD_MS: u32 = 4000;

/// Длина полного круга тона в единицах HSV (см. [`Hsv8`]).
const HUE_CIRCLE: u32 = 256;

/// Полная насыщенность и яркость: радуга — чистые цвета спектра; потолок
/// общей яркости Ленты накладывает Прошивка при выводе.
const FULL: u8 = 255;

/// Радуга по проводу.
pub struct Rainbow {
    /// Привязка координат: задаёт физический порядок Ленты; копия приходит
    /// из фабрики Прошивки и хранится в Эффекте.
    mapping: Mapping,
}

impl Rainbow {
    /// Радуга по Привязке `mapping`: именно она определяет, где на Матрице
    /// стоит каждый Пиксель Ленты.
    pub fn new(mapping: Mapping) -> Self {
        Self { mapping }
    }
}

impl Effect for Rainbow {
    fn render(&mut self, frame: &mut Frame<'_>, elapsed_ms: u32) {
        let count = frame.len();
        if count == 0 {
            return;
        }
        // Фаза времени: полный круг тона за период; остаток от деления до
        // умножения исключает переполнение u32 на любом моменте шкалы.
        let phase = ((elapsed_ms % PERIOD_MS) * HUE_CIRCLE / PERIOD_MS) as u8;
        for index in 0..count {
            // Позиция вдоль Ленты: полный круг тона разложен на всю Ленту.
            let along = (index as u32 * HUE_CIRCLE / count as u32) as u8;
            let color = hsv_to_rgb(Hsv8 {
                h: along.wrapping_add(phase),
                s: FULL,
                v: FULL,
            });
            frame.set_wire(&self.mapping, index, color);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use garland_core::{Corner, Layout, Rgb8};

    /// Матрица 4×3, змейка по рядам: Лента 12 Пикселей с разворотом.
    fn mapping() -> Mapping {
        Mapping::new(4, 3, Layout::RowSerpentine, Corner::TopLeft)
    }

    /// Цвет тона `h` спектральной радуги (насыщенность и яркость полные).
    fn hue_color(h: u8) -> Rgb8 {
        hsv_to_rgb(Hsv8 {
            h,
            s: FULL,
            v: FULL,
        })
    }

    /// На старте Пиксель 0 Ленты — чистый красный: тон круга начинается
    /// с красного и нулевой фазы времени.
    #[test]
    fn wire_start_is_red_at_launch() {
        let mapping = mapping();
        let mut rainbow = Rainbow::new(mapping);
        let mut buffer = [Rgb8::BLACK; 12];
        let mut frame = Frame::new(4, 3, &mut buffer).unwrap();

        rainbow.render(&mut frame, 0);

        let (x, y) = mapping.coord(0);
        assert_eq!(frame.get(x, y), Some(Rgb8 { r: 255, g: 0, b: 0 }));
    }

    /// Круг тона разложен по Ленте: Пиксель Ленты 5 (на змейке 4×3 это
    /// координата (2, 1)) несёт тон 5·256/12 = 106, а строка-мажорный сосед
    /// (1, 1) — Пиксель Ленты 6 (ряд развёрнут) с тоном 128: радуга идёт
    /// по проводу, а не по строкам Фреймбуфера.
    #[test]
    fn hue_spreads_along_the_wire() {
        let mapping = mapping();
        let mut rainbow = Rainbow::new(mapping);
        let mut buffer = [Rgb8::BLACK; 12];
        let mut frame = Frame::new(4, 3, &mut buffer).unwrap();

        rainbow.render(&mut frame, 0);

        assert_eq!(frame.get(2, 1), Some(hue_color(106)));
        assert_eq!(frame.get(1, 1), Some(hue_color(128)));
        assert_ne!(frame.get(2, 1), frame.get(1, 1));
    }

    /// Радуга течёт: за четверть периода фаза тона сдвигается на четверть
    /// круга (256/4 = 64), а через полный период кадр совпадает с начальным.
    #[test]
    fn flows_with_time_and_wraps_around() {
        let mapping = mapping();
        let mut rainbow = Rainbow::new(mapping);
        let mut start = [Rgb8::BLACK; 12];
        let mut quarter = [Rgb8::BLACK; 12];
        let mut full = [Rgb8::BLACK; 12];

        rainbow.render(&mut Frame::new(4, 3, &mut start).unwrap(), 0);
        rainbow.render(&mut Frame::new(4, 3, &mut quarter).unwrap(), PERIOD_MS / 4);
        rainbow.render(&mut Frame::new(4, 3, &mut full).unwrap(), PERIOD_MS);

        assert_eq!(quarter[0], hue_color(64));
        assert_ne!(quarter, start);
        assert_eq!(full, start);
    }

    /// Яркость полная на всей Ленте: чёрных Пикселей нет ни в одном кадре.
    #[test]
    fn covers_the_whole_strip() {
        let mapping = mapping();
        let mut rainbow = Rainbow::new(mapping);
        let mut buffer = [Rgb8::BLACK; 12];
        let mut frame = Frame::new(4, 3, &mut buffer).unwrap();

        rainbow.render(&mut frame, 137);

        assert!(frame.pixels().iter().all(|c| *c != Rgb8::BLACK));
    }

    /// Вырожденный пустой Кадр (0×0) не паникует.
    #[test]
    fn empty_frame_is_a_noop() {
        let mapping = mapping();
        let mut rainbow = Rainbow::new(mapping);
        let mut buffer = [];
        let mut frame = Frame::new(0, 0, &mut buffer).unwrap();

        rainbow.render(&mut frame, 0);
    }

    /// Вырожденная Матрица 1×1 — единственный Пиксель: без паники, тон
    /// нулевой фазы (вдоль Ленты index·256/count = 0).
    #[test]
    fn single_pixel_frame_renders() {
        let mapping = Mapping::new(1, 1, Layout::RowSerpentine, Corner::TopLeft);
        let mut rainbow = Rainbow::new(mapping);
        let mut buffer = [Rgb8::BLACK];
        let mut frame = Frame::new(1, 1, &mut buffer).unwrap();

        rainbow.render(&mut frame, 0);

        assert_eq!(frame.pixels()[0], hue_color(0));
    }

    /// Шкала времени не переполняет u32: кадр на границе шкалы (почти
    /// 49,7 суток) считается без паники и остаётся в полную яркость.
    #[test]
    fn elapsed_at_u32_max_does_not_overflow() {
        let mapping = mapping();
        let mut rainbow = Rainbow::new(mapping);
        let mut buffer = [Rgb8::BLACK; 12];
        let mut frame = Frame::new(4, 3, &mut buffer).unwrap();

        rainbow.render(&mut frame, u32::MAX);

        assert!(frame.pixels().iter().all(|c| *c != Rgb8::BLACK));
    }
}
