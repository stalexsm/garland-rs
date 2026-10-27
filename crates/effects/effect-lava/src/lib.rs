//! Аддон «Лава»: медленно клубящиеся тёплые пятна на Матрице.
//!
//! Эффект — тонкая обёртка над общим шумовым движком [`NoiseField`]
//! (`fillNoiseLED`): квадратное поле 8-битного шума Перлина
//! дрейфует по координатам каждый такт, окраска идёт через «лавовую»
//! палитру — тёмно-красные разломы с оранжевыми прожилками. Вся математика
//! целочисленная и уже портирована в ядро, здесь только тактирование.
//!

#![no_std]

use garland_core::{Effect, Frame, LAVA_PALETTE, Mapping, NoiseField, advance_anchor, steps_since};

/// Интервал такта, мс: шумовые эффекты исходного алгоритма — пошаговые.
const TICK_MS: u32 = 80;

/// Скорость шума: дрейф координат за такт — дефолт исходной прошивки.
const SPEED: u16 = 20;

/// Масштаб шума: размер «языков» — `LAVA_SCALE` исходника.
const SCALE: u16 = 50;

/// Тон палитры не циклится: в «Лаве» `colorLoop = 0`.
const COLOR_LOOP: bool = false;

/// Лава.
pub struct Lava {
    /// Шумовое поле: состояние эффекта (шум Перлина и дрейф координат).
    noise: NoiseField,
    /// Момент последнего такта, мс: шкала wrapping, разности — через
    /// [`steps_since`].
    last_ms: u32,
}

impl Lava {
    /// Лава для Матрицы из Привязки `mapping`.
    ///
    /// # Panics
    ///
    /// Паникует при стороне Матрицы больше `NOISE_MAX_SIDE`: шумовое поле
    /// квадратное `side × side`, большие Матрицы шумом не поддерживаются.
    /// Такая конфигурация отсеивается при старте Прошивки, поэтому паника
    /// допустима только как ошибка сборки.
    pub fn new(mapping: Mapping) -> Self {
        Self {
            noise: NoiseField::new(mapping.width(), mapping.height()).expect(
                "Шумовые эффекты поддерживают матрицы со стороной не больше NOISE_MAX_SIDE",
            ),
            last_ms: 0,
        }
    }
}

impl Effect for Lava {
    fn render(&mut self, frame: &mut Frame<'_>, elapsed_ms: u32) {
        // Пошаговая физика исходного алгоритма: догнать пропущенные такты разом
        // (задолженность ограничена потолком в [`steps_since`]).
        let steps = steps_since(self.last_ms, elapsed_ms, TICK_MS);
        self.last_ms = advance_anchor(self.last_ms, elapsed_ms, TICK_MS);
        if steps > 0 {
            for _ in 0..steps {
                self.noise.tick(SCALE, SPEED);
            }
        }
        // Контракт Эффекта выполняет движок: окраска пишет каждый Пиксель.
        self.noise.render(&LAVA_PALETTE, COLOR_LOOP, frame);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use garland_core::{Corner, Layout, Rgb8};

    /// Матрица 8×8: змейка по рядам — как на Стенде.
    fn mapping() -> Mapping {
        Mapping::new(8, 8, Layout::RowSerpentine, Corner::TopLeft)
    }

    /// Грязный Фреймбуфер не переживает рендер: на старте поле нулевое,
    /// движок гасит каждый Пиксель в чёрный.
    #[test]
    fn dirty_buffer_is_cleared() {
        let mut lava = Lava::new(mapping());
        let mut buffer = [Rgb8 { r: 9, g: 9, b: 9 }; 64];
        let mut frame = Frame::new(8, 8, &mut buffer).unwrap();

        lava.render(&mut frame, 0);

        assert!(frame.pixels().iter().all(|c| *c == Rgb8::BLACK));
    }

    /// Детерминизм: два экземпляра на один момент (восемь тактов, поле
    /// прогрето) рисуют бит-в-бит одинаковые Кадры.
    #[test]
    fn same_time_same_frame() {
        let mut left_lava = Lava::new(mapping());
        let mut right_lava = Lava::new(mapping());
        let mut left = [Rgb8::BLACK; 64];
        let mut right = [Rgb8::BLACK; 64];

        left_lava.render(&mut Frame::new(8, 8, &mut left).unwrap(), 8 * TICK_MS);
        right_lava.render(&mut Frame::new(8, 8, &mut right).unwrap(), 8 * TICK_MS);

        assert_eq!(left, right);
    }

    /// Граница шкалы времени (почти 49,7 суток) без паники: задолженность
    /// тактов ограничена потолком движка, Кадр рисуется.
    #[test]
    fn elapsed_at_u32_max_does_not_overflow() {
        let mut lava = Lava::new(mapping());
        let mut buffer = [Rgb8 { r: 9, g: 9, b: 9 }; 64];
        let mut frame = Frame::new(8, 8, &mut buffer).unwrap();

        lava.render(&mut frame, u32::MAX);

        assert!(
            frame
                .pixels()
                .iter()
                .any(|c| *c != Rgb8 { r: 9, g: 9, b: 9 })
        );
    }

    /// Пустой Кадр 0×0 — no-op: `NoiseField::new(0, 0)` возвращает поле,
    /// рендер по нулю Пикселей ничего не рисует и не паникует.
    #[test]
    fn empty_frame_is_a_noop() {
        let mut lava = Lava::new(Mapping::new(0, 0, Layout::RowSerpentine, Corner::TopLeft));
        let mut buffer = [];
        let mut frame = Frame::new(0, 0, &mut buffer).unwrap();

        lava.render(&mut frame, 8 * TICK_MS);
    }

    /// Такты двигают поле: на старте Кадр чёрный, через восемь тактов —
    /// живая картинка.
    #[test]
    fn frame_changes_after_eight_ticks() {
        let mut lava = Lava::new(mapping());
        let mut buffer = [Rgb8::BLACK; 64];
        let mut frame = Frame::new(8, 8, &mut buffer).unwrap();

        lava.render(&mut frame, 8 * TICK_MS);

        assert!(frame.pixels().iter().any(|c| *c != Rgb8::BLACK));
    }

    /// Палитра тёплая: после прогрева ни один Пиксель не имеет синего
    /// канала больше красного.
    #[test]
    fn lava_pixels_are_warm() {
        let mut lava = Lava::new(mapping());
        let mut buffer = [Rgb8::BLACK; 64];
        let mut frame = Frame::new(8, 8, &mut buffer).unwrap();

        lava.render(&mut frame, 8 * TICK_MS);

        assert!(frame.pixels().iter().all(|c| c.b <= c.r));
    }
}
