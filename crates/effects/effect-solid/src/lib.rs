//! Аддон «Статичный цвет»: вся Лента горит одним цветом.
//!
//! Простейший Эффект Реестра: Кадр целиком заливается цветом Аддона,
//! независимо от времени. Цвет фиксирован константой Аддона — параметры
//! эффектов в Прошивке v1 не настраиваются.
#![no_std]

use garland_core::{Effect, Frame, Rgb8};

/// Цвет Аддона: тёплый белый.
const COLOR: Rgb8 = Rgb8 {
    r: 255,
    g: 180,
    b: 120,
};

/// Статичный цвет.
pub struct Solid;

impl Effect for Solid {
    fn render(&mut self, frame: &mut Frame<'_>, _elapsed_ms: u32) {
        frame.fill(COLOR);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ожидаемый цвет — литерал теста, независимый от константы кода:
    /// случайная правка цвета Аддона ловится этим тестом.
    const WARM_WHITE: Rgb8 = Rgb8 {
        r: 255,
        g: 180,
        b: 120,
    };

    /// Весь Кадр залит тёплым белым, чёрного не остаётся.
    #[test]
    fn fills_whole_frame_with_warm_white() {
        let mut solid = Solid;
        let mut buffer = [Rgb8::BLACK; 12];
        let mut frame = Frame::new(4, 3, &mut buffer).unwrap();

        solid.render(&mut frame, 0);

        assert_eq!(frame.pixels(), &[WARM_WHITE; 12]);
    }

    /// Эффект статичен: Кадры в разные моменты времени совпадают.
    #[test]
    fn is_the_same_at_any_time() {
        let mut solid = Solid;
        let mut early = [Rgb8::BLACK; 12];
        let mut late = [Rgb8::BLACK; 12];

        solid.render(&mut Frame::new(4, 3, &mut early).unwrap(), 0);
        solid.render(&mut Frame::new(4, 3, &mut late).unwrap(), 9_999);

        assert_eq!(early, late);
    }
}
