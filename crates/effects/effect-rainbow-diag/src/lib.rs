//! Аддон «Радуга диагональная»: диагональный градиент полного спектра.
//!
//! Тон Пикселя — `hue + (x + y)·шаг`, где шаг — 255, делённые на
//! наибольшую сторону Матрицы: диагонали (x + y = const) окрашены ровно
//! одним тоном, каждые 80 мс общий сдвиг подрастает на 3 — спектр течёт
//! по диагонали.
//!
//! Отличия от оригинала:
//! 1. У исходного алгоритма тон считается во float с целочисленными подделениями
//!    (`WIDTH/HEIGHT*x` и `255/max(W,H)`); на квадратной Матрице это
//!    сводится к `(x+y)·шаг` — здесь формула сразу целочисленная, на
//!    неквадратной Матрице порт расходится с float-выкладкой оригинала.
//! 2. Шаг градиента целый: при стороне 256 и больше он вырождается в ноль —
//!    весь Кадр в одном бегущем тоне (в исходнике так же).
//! 3. Сдвиг тона привязан к такту 80 мс; оригинал гонял его на каждом
//!    проходе основного цикла.

#![no_std]

use garland_core::{Effect, Frame, Hsv8, Mapping, advance_anchor, hsv2rgb_rainbow, steps_since};

/// Период такта исходных эффектов, мс: один шаг анимации раз в 80 мс.
const TICK_MS: u32 = 80;

/// Прирост тона за такт: `hue += 3` исходного алгоритма — круг тона за ~8,5 с.
const HUE_STEP: u8 = 3;

/// Числитель шага градиента: круг тона на наибольшую сторону
/// (`255 / max(WIDTH, HEIGHT)` исходного алгоритма).
const GRADIENT_STEPS: u32 = 255;

/// Длина круга тона HSV: модуль byte-cast переполнением в исходнике.
const HUE_CIRCLE: u32 = 256;

/// Полные насыщенность и яркость: радуга — чистые цвета спектра.
const FULL: u8 = 255;

/// Радуга диагональная.
pub struct RainbowDiag {
    /// Ширина Матрицы, Пикселей.
    w: u16,
    /// Высота Матрицы, Пикселей.
    h: u16,
    /// Бегущий сдвиг тона (`hue` исходного алгоритма).
    hue: u8,
    /// Момент последнего такта, мс (шкала wrapping).
    last_ms: u32,
}

impl RainbowDiag {
    /// Радуга для Матрицы из Привязки `mapping`: от наибольшей стороны
    /// зависит шаг диагонального градиента.
    pub fn new(mapping: Mapping) -> Self {
        Self {
            w: mapping.width(),
            h: mapping.height(),
            hue: 0,
            last_ms: 0,
        }
    }

    /// Целочисленный шаг градиента: круг тона на наибольшую сторону; на
    /// стороне [`GRADIENT_STEPS`] и больше шаг вырождается в ноль — весь
    /// Кадр в одном тоне.
    fn gradient_step(&self) -> u32 {
        GRADIENT_STEPS
            .checked_div(u32::from(self.w.max(self.h)))
            .unwrap_or(0)
    }

    /// Один такт: сдвиг тона на [`HUE_STEP`].
    fn step(&mut self) {
        self.hue = self.hue.wrapping_add(HUE_STEP);
    }
}

impl Effect for RainbowDiag {
    fn render(&mut self, frame: &mut Frame<'_>, elapsed_ms: u32) {
        // Такты исходного алгоритма: досчитать все пропущенные шаги, затем рисовать.
        let steps = steps_since(self.last_ms, elapsed_ms, TICK_MS);
        self.last_ms = advance_anchor(self.last_ms, elapsed_ms, TICK_MS);
        if steps > 0 {
            for _ in 0..steps {
                self.step();
            }
        }
        let step = self.gradient_step();
        let (w, h) = (usize::from(frame.width()), usize::from(frame.height()));
        for y in 0..h {
            for x in 0..w {
                // Тон диагонали (x + y = const): byte-cast переполнением,
                // как в исходнике.
                let hue = self
                    .hue
                    .wrapping_add(((x + y) as u32 * step % HUE_CIRCLE) as u8);
                frame.set(
                    x as u16,
                    y as u16,
                    hsv2rgb_rainbow(Hsv8 {
                        h: hue,
                        s: FULL,
                        v: FULL,
                    }),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use garland_core::{Corner, Layout, Rgb8};

    /// Привязка Матрицы 8×8 со змейкой — размер стандартного Стенда.
    fn mapping8() -> Mapping {
        Mapping::new(8, 8, Layout::RowSerpentine, Corner::TopLeft)
    }

    /// Цвет спектральной радуги тона `h` (насыщенность и яркость полные).
    fn spectrum(h: u8) -> Rgb8 {
        hsv2rgb_rainbow(Hsv8 {
            h,
            s: FULL,
            v: FULL,
        })
    }

    /// Диагонали (x + y = const) окрашены одним тоном: (1,0) = (0,1),
    /// (2,0) = (1,1) = (0,2), а соседние диагонали различимы.
    #[test]
    fn diagonal_keeps_one_color() {
        let mut rainbow = RainbowDiag::new(mapping8());
        let mut buffer = [Rgb8::BLACK; 64];
        let mut frame = Frame::new(8, 8, &mut buffer).unwrap();

        rainbow.render(&mut frame, 137);

        assert_eq!(frame.get(1, 0), frame.get(0, 1));
        assert_eq!(frame.get(2, 0), frame.get(1, 1));
        assert_eq!(frame.get(2, 0), frame.get(0, 2));
        assert_ne!(frame.get(0, 0), frame.get(1, 0));
    }

    /// Тон растёт вправо-вниз: старт — чистый красный, соседняя диагональ —
    /// шаг градиента 255/8 = 31 (в любую из сторон).
    #[test]
    fn hue_grows_along_diagonals() {
        let mut rainbow = RainbowDiag::new(mapping8());
        let mut buffer = [Rgb8::BLACK; 64];
        let mut frame = Frame::new(8, 8, &mut buffer).unwrap();

        rainbow.render(&mut frame, 0);

        assert_eq!(frame.get(0, 0), Some(Rgb8 { r: 255, g: 0, b: 0 }));
        assert_eq!(frame.get(1, 0), Some(spectrum(31)));
        assert_eq!(frame.get(0, 1), Some(spectrum(31)));
    }

    /// Один такт — сдвиг тона на 3 (`hue += 3` исходного алгоритма): старт красный,
    /// через 80 мс тон 3, ещё через 160 мс — 9.
    #[test]
    fn hue_steps_three_per_tick() {
        let mut rainbow = RainbowDiag::new(mapping8());
        let mut first = [Rgb8::BLACK; 64];
        let mut next = [Rgb8::BLACK; 64];
        let mut later = [Rgb8::BLACK; 64];

        rainbow.render(&mut Frame::new(8, 8, &mut first).unwrap(), 0);
        rainbow.render(&mut Frame::new(8, 8, &mut next).unwrap(), TICK_MS);
        rainbow.render(&mut Frame::new(8, 8, &mut later).unwrap(), 3 * TICK_MS);

        assert_eq!(next[0], spectrum(3));
        assert_eq!(later[0], spectrum(9));
        assert_ne!(next, first);
    }

    /// Кадр заполняется целиком: мусор от прошлых Кадров не переживает
    /// рендер (спектральная радуга всегда яркая — «девятки» не остаются).
    #[test]
    fn dirty_buffer_is_cleared() {
        let mut rainbow = RainbowDiag::new(mapping8());
        let mut buffer = [Rgb8 { r: 9, g: 9, b: 9 }; 64];
        let mut frame = Frame::new(8, 8, &mut buffer).unwrap();

        rainbow.render(&mut frame, 137);

        assert!(
            frame
                .pixels()
                .iter()
                .all(|c| *c != Rgb8 { r: 9, g: 9, b: 9 })
        );
    }

    /// Эффект — чистая функция истории тактов: новые экземпляры с той же
    /// историей рисуют бит-в-бит одинаковые Кадры.
    #[test]
    fn same_history_same_frame() {
        let mut first = RainbowDiag::new(mapping8());
        let mut second = RainbowDiag::new(mapping8());
        let mut left = [Rgb8::BLACK; 64];
        let mut right = [Rgb8::BLACK; 64];

        for elapsed in [80, 1000] {
            first.render(&mut Frame::new(8, 8, &mut left).unwrap(), elapsed);
            second.render(&mut Frame::new(8, 8, &mut right).unwrap(), elapsed);
        }

        assert_eq!(left, right);
    }

    /// Вырожденный пустой Кадр (0×0) не паникует — и такты на вырожденной
    /// Матрице тоже (шаг градиента нулевой, Пикселей нет).
    #[test]
    fn empty_frame_is_a_noop() {
        let mut rainbow =
            RainbowDiag::new(Mapping::new(0, 0, Layout::RowSerpentine, Corner::TopLeft));
        let mut buffer = [];
        let mut frame = Frame::new(0, 0, &mut buffer).unwrap();

        rainbow.render(&mut frame, 0);
        rainbow.render(&mut frame, 3 * TICK_MS);
    }

    /// Шкала времени не переполняет u32: кадр на границе шкалы (почти
    /// 49,7 суток) считается без паники, мусор в буфере не переживает.
    #[test]
    fn elapsed_at_u32_max_does_not_overflow() {
        let mut rainbow = RainbowDiag::new(mapping8());
        let mut buffer = [Rgb8 { r: 9, g: 9, b: 9 }; 64];
        let mut frame = Frame::new(8, 8, &mut buffer).unwrap();

        rainbow.render(&mut frame, u32::MAX);

        assert!(
            frame
                .pixels()
                .iter()
                .all(|c| *c != Rgb8 { r: 9, g: 9, b: 9 })
        );
    }
}
