//! Аддон «Кольца и волны»: расходящиеся от центра Матрицы кольца тона.
//!
//! 2D-Эффект: цвет Пикселя зависит только от его расстояния до центра
//! Матрицы и времени — кольца разбегаются от центра волнами, независимо
//! от Раскладки Ленты. Время приходит снаружи, состояние не накапливается:
//! кадры детерминированны и проверяются тестами на хосте.

#![no_std]

use garland_core::{Effect, Frame, Hsv8, hsv_to_rgb};

/// Период пробега полного круга тона, мс: волна оборачивается за 3 с.
const PERIOD_MS: u32 = 3000;

/// Длина полного круга тона в единицах HSV (см. [`Hsv8`]).
const HUE_CIRCLE: u32 = 256;

/// Шаг тона на единицу расстояния (полупиксель): тон повторяется каждые
/// 16 полупикселей — на Стенде 8×8 видно несколько колец сразу.
const HUE_PER_STEP: u32 = 16;

/// Полная насыщенность и яркость: кольца — чистые цвета спектра; потолок
/// общей яркости Ленты накладывает Прошивка при выводе.
const FULL: u8 = 255;

/// Кольца и волны.
pub struct Rings;

impl Effect for Rings {
    fn render(&mut self, frame: &mut Frame<'_>, elapsed_ms: u32) {
        if frame.is_empty() {
            return;
        }
        // Фаза времени: полный круг тона за период; вычитание сдвигает
        // картинку от центра — кольца разбегаются, а не сходятся.
        let phase = ((elapsed_ms % PERIOD_MS) * HUE_CIRCLE / PERIOD_MS) as u8;
        // Удвоенные координаты: центр есть у любой Матрицы, и чётной, и
        // нечётной — расстояния считаются в полупикселях без дробей.
        let w = u32::from(frame.width());
        let h = u32::from(frame.height());
        for y in 0..frame.height() {
            let dy = (2 * u32::from(y) + 1).abs_diff(h);
            for x in 0..frame.width() {
                let dx = (2 * u32::from(x) + 1).abs_diff(w);
                let distance = (dx * dx + dy * dy).isqrt();
                let hue = (distance * HUE_PER_STEP) as u8;
                let color = hsv_to_rgb(Hsv8 {
                    h: hue.wrapping_sub(phase),
                    s: FULL,
                    v: FULL,
                });
                frame.set(x, y, color);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use garland_core::{Hsv8, Rgb8, hsv_to_rgb};

    /// Полные насыщенность и яркость — литерал теста.
    const FULL: u8 = 255;

    /// Цвет тона `h` спектральной радуги — независимый источник ожиданий.
    fn hue_color(h: u8) -> Rgb8 {
        hsv_to_rgb(Hsv8 {
            h,
            s: FULL,
            v: FULL,
        })
    }

    /// Кольцо — один цвет: Пиксели равного расстояния от центра горят
    /// одинаково, соседнее кольцо — иначе. На Стенде 8×8 кольцо расстояния 7
    /// (полупикселей) образуют восемь Пикселей середины сторон, углы —
    /// дальнее кольцо 9.
    #[test]
    fn ring_shares_one_color() {
        let mut rings = Rings;
        let mut buffer = [Rgb8::BLACK; 64];
        let mut frame = Frame::new(8, 8, &mut buffer).unwrap();

        rings.render(&mut frame, 0);

        let ring7 = frame.get(3, 0).unwrap();
        for (x, y) in [
            (3, 0),
            (4, 0),
            (3, 7),
            (4, 7),
            (0, 3),
            (7, 3),
            (0, 4),
            (7, 4),
        ] {
            assert_eq!(frame.get(x, y), Some(ring7), "кольцо 7: ({x}, {y})");
        }
        assert_ne!(frame.get(0, 0), Some(ring7), "угол — другое кольцо");
    }

    /// Кольца разбегаются: за шаг фазы (прирост тона на один шаг расстояния)
    /// тон каждого Пикселя уезжает к следующему внутреннему кольцу —
    /// центральный блок становится красным, угол (кольцо 9) сдвигается
    /// на 16 тонов назад по кругу.
    #[test]
    fn rings_expand_outward_with_time() {
        let mut rings = Rings;
        // Момент, когда фаза дорастает до одного шага тона.
        let step_ms = (HUE_PER_STEP * PERIOD_MS).div_ceil(HUE_CIRCLE);
        let mut buffer = [Rgb8::BLACK; 64];
        let mut frame = Frame::new(8, 8, &mut buffer).unwrap();

        rings.render(&mut frame, step_ms);

        assert_eq!(frame.get(3, 3), Some(hue_color(0)), "центр — красный");
        assert_eq!(frame.get(0, 0), Some(hue_color(128)), "угол — тон 144−16");
    }

    /// Волна зациклена: через полный период кадр совпадает с начальным,
    /// а в середине периода — другой.
    #[test]
    fn wraps_after_full_period() {
        let mut rings = Rings;
        let mut start = [Rgb8::BLACK; 64];
        let mut mid = [Rgb8::BLACK; 64];
        let mut full = [Rgb8::BLACK; 64];

        rings.render(&mut Frame::new(8, 8, &mut start).unwrap(), 0);
        rings.render(&mut Frame::new(8, 8, &mut mid).unwrap(), PERIOD_MS / 2);
        rings.render(&mut Frame::new(8, 8, &mut full).unwrap(), PERIOD_MS);

        assert_eq!(full, start);
        assert_ne!(mid, start);
    }

    /// Кадр залит целиком: чёрных Пикселей нет ни в одном кольце.
    #[test]
    fn covers_the_whole_matrix() {
        let mut rings = Rings;
        let mut buffer = [Rgb8::BLACK; 64];
        let mut frame = Frame::new(8, 8, &mut buffer).unwrap();

        rings.render(&mut frame, 137);

        assert!(frame.pixels().iter().all(|c| *c != Rgb8::BLACK));
    }

    /// Эффект — чистая функция времени: два экземпляра на один и тот же
    /// момент рисуют бит в бит одинаковый кадр.
    #[test]
    fn same_time_same_frame() {
        let mut first = Rings;
        let mut second = Rings;
        let mut left = [Rgb8::BLACK; 64];
        let mut right = [Rgb8::BLACK; 64];

        first.render(&mut Frame::new(8, 8, &mut left).unwrap(), 1_234);
        second.render(&mut Frame::new(8, 8, &mut right).unwrap(), 1_234);

        assert_eq!(left, right);
    }

    /// Вырожденный пустой Кадр (0×0) не паникует.
    #[test]
    fn empty_frame_is_a_noop() {
        let mut rings = Rings;
        let mut buffer = [];
        let mut frame = Frame::new(0, 0, &mut buffer).unwrap();

        rings.render(&mut frame, 0);
    }

    /// Центр нечётной Матрицы — Пиксель нулевого расстояния: на старте он
    /// красный, с нулевого тона разбегаются кольца.
    #[test]
    fn center_of_odd_matrix_is_red_at_launch() {
        let mut rings = Rings;
        let mut buffer = [Rgb8::BLACK; 25];
        let mut frame = Frame::new(5, 5, &mut buffer).unwrap();

        rings.render(&mut frame, 0);

        assert_eq!(frame.get(2, 2), Some(Rgb8 { r: 255, g: 0, b: 0 }));
    }
}
