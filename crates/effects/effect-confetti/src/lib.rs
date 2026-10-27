//! Аддон «Конфетти»: случайные гаснущие вспышки на тёмной Матрице.
//!
//! Раз за такт ГПСЧ трижды выбирает клетку; чёрная клетка вспыхивает чистым
//! цветом случайного тона, затем всё поле гаснет на долю яркости — вспышки
//! живут несколько тактов и плавно осыпаются. Тики приходят снаружи, ГПСЧ
//! детерминированный (сид FastLED): кадры одинаковы на хосте и устройстве
//! и проверяются тестами.
//!
//!
//! Отклонения от оригинала: `random()` Arduino заменён на детерминированный
//! [`Rng16`] FastLED, тон вспышки — полный байт [`Rng16::random8`] (в C был
//! `random(0, 255)`, то есть 0..254); оси координат перевёрнуты (в исходнике
//! y = 0 — низ Матрицы, здесь y = 0 — верх), для конфетти это картинки
//! не меняет.

#![no_std]

use garland_core::{
    Effect, Frame, Hsv8, Mapping, NOISE_MAX_SIDE, Rgb8, Rng16, fade_pixel, hsv2rgb_rainbow,
    take_steps,
};

/// Период такта, мс: пошаговые эффекты шагают раз в 80 мс.
const TICK_MS: u32 = 80;

/// Попыток вспышки за такт (`DENSE` оригинала): клетка вспыхивает, только
/// если чёрная, поэтому реальных вспышек за такт — до трёх.
const FLASH_ATTEMPTS: u8 = 3;

/// Полная насыщенность и яркость вспышки: конфетти — чистые цвета спектра.
const FULL: u8 = 255;

/// Шаг затухания за такт (`BRIGHT_STEP` оригинала): доля 70/256 яркости.
const FADE_STEP: u8 = 70;

/// Потолок поля: квадрат [`NOISE_MAX_SIDE`]×[`NOISE_MAX_SIDE`] покрывает
/// любую поддерживаемую Матрицу (4 КиБ на экземпляр Эффекта; живёт всегда
/// ровно один экземпляр Текущего эффекта).
const FIELD_CAP: usize = (NOISE_MAX_SIDE * NOISE_MAX_SIDE) as usize;

/// Конфетти.
pub struct Confetti {
    /// Ширина Матрицы, Пикселей (из Привязки при создании).
    w: u16,
    /// Высота Матрицы, Пикселей (из Привязки при создании).
    h: u16,
    /// Постоянное поле (аналог `leds[]` оригинала): строка-мажор, верхняя
    /// строка — начало.
    field: [Rgb8; FIELD_CAP],
    /// Детерминированный ГПСЧ вспышек.
    rng: Rng16,
    /// Момент последнего догнанного такта на шкале времени Прошивки.
    last_ms: u32,
}

impl Confetti {
    /// Конфетти для Матрицы из Привязки `mapping`.
    pub fn new(mapping: Mapping) -> Self {
        Self {
            w: mapping.width(),
            h: mapping.height(),
            field: [Rgb8::BLACK; FIELD_CAP],
            rng: Rng16::new(),
            last_ms: 0,
        }
    }

    /// Один такт физики: три попытки вспышки и затухание всего поля. Как в
    /// `sparklesRoutine`, вспышки пишутся до затухания, поэтому свежая
    /// вспышка впервые видна уже притушенной на один шаг.
    fn step(&mut self) {
        // Стороны Матрицы не больше `NOISE_MAX_SIDE` (потолок поля), поэтому
        // байтовых границ ГПСЧ хватает.
        let w = usize::from(self.w);
        for _ in 0..FLASH_ATTEMPTS {
            let x = usize::from(self.rng.random8_below(self.w as u8));
            let y = usize::from(self.rng.random8_below(self.h as u8));
            let index = y * w + x;
            if self.field[index] == Rgb8::BLACK {
                let hue = self.rng.random8();
                self.field[index] = hsv2rgb_rainbow(Hsv8 {
                    h: hue,
                    s: FULL,
                    v: FULL,
                });
            }
        }
        for cell in &mut self.field {
            *cell = fade_pixel(*cell, FADE_STEP);
        }
    }
}

impl Effect for Confetti {
    fn render(&mut self, frame: &mut Frame<'_>, elapsed_ms: u32) {
        if frame.is_empty() {
            return;
        }
        // Время снаружи: все пропущенные такты догоняются разом.
        for _ in 0..take_steps(&mut self.last_ms, elapsed_ms, TICK_MS) {
            self.step();
        }
        // Кадр — копия постоянного поля: контракт Эффекта — заполнить
        // Фреймбуфер целиком (буферы Прошивки переиспользуются).
        let w = usize::from(self.w);
        for y in 0..self.h {
            for x in 0..self.w {
                frame.set(x, y, self.field[usize::from(y) * w + usize::from(x)]);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use garland_core::{Corner, Layout};

    /// Мусорный цвет для проверки «Кадр заполняется целиком»: в спектре
    /// конфетти (полная насыщенность) пурпурного (255, 0, 255) не бывает.
    const JUNK: Rgb8 = Rgb8 {
        r: 255,
        g: 0,
        b: 255,
    };

    /// Матрица Стенда 8×8, змейка — для конфетти Привязка не важна.
    fn mapping8() -> Mapping {
        Mapping::new(8, 8, Layout::RowSerpentine, Corner::TopLeft)
    }

    /// Фреймбуфер 8×8 поверх буфера теста.
    fn frame8(buffer: &mut [Rgb8; 64]) -> Frame<'_> {
        Frame::new(8, 8, buffer).unwrap()
    }

    /// Первые вспышки детерминированы: на первом такте ГПСЧ (сид 1337)
    /// выбирает клетки (2, 4), (7, 0) и (2, 5), все чёрные — вспыхивают все
    /// три; тона 37, 72 и 20 после первого затухания вспышки.
    #[test]
    fn first_flashes_are_exact() {
        let mut confetti = Confetti::new(mapping8());
        let mut buffer = [JUNK; 64];
        let mut frame = frame8(&mut buffer);

        confetti.render(&mut frame, TICK_MS);

        assert_eq!(
            frame.get(2, 4),
            Some(Rgb8 {
                r: 124,
                g: 71,
                b: 0
            })
        );
        assert_eq!(
            frame.get(7, 0),
            Some(Rgb8 {
                r: 93,
                g: 138,
                b: 0
            })
        );
        assert_eq!(
            frame.get(2, 5),
            Some(Rgb8 {
                r: 146,
                g: 38,
                b: 0
            })
        );
        let lit = frame.pixels().iter().filter(|c| **c != Rgb8::BLACK).count();
        assert_eq!(lit, 3, "вспышек на первом такте: {lit}");
    }

    /// Затухание вспышки — точная цепочка `fade_pixel(_, 70)` (умножение на
    /// 186/256 за такт, отсечка каналов ниже 30): каналы вспышки (2, 4)
    /// проходят 124 → 90 → 65 → 47 → 34 → 24, на 7-й такт после кадра
    /// вспышки Пиксель уходит в чёрный (утверждённый потолок: самый долгий
    /// цвет — с каналом 255 — гаснет ровно за 7 тактов).
    #[test]
    fn flash_fades_to_black_within_seven_ticks() {
        let mut confetti = Confetti::new(mapping8());
        let mut buffer = [Rgb8::BLACK; 64];
        let mut frame = frame8(&mut buffer);

        confetti.render(&mut frame, TICK_MS);
        assert_eq!(
            frame.get(2, 4),
            Some(Rgb8 {
                r: 124,
                g: 71,
                b: 0
            })
        );

        let chain = [
            (Rgb8 { r: 90, g: 51, b: 0 }, 2),
            (Rgb8 { r: 65, g: 37, b: 0 }, 3),
            (Rgb8 { r: 47, g: 26, b: 0 }, 4),
            (Rgb8 { r: 34, g: 18, b: 0 }, 5),
            (Rgb8 { r: 24, g: 13, b: 0 }, 6),
        ];
        for (color, tick) in chain {
            confetti.render(&mut frame, tick * TICK_MS);
            assert_eq!(frame.get(2, 4), Some(color), "такт {tick}");
        }

        confetti.render(&mut frame, 7 * TICK_MS);
        assert_eq!(frame.get(2, 4), Some(Rgb8::BLACK));
    }

    /// К 10-му такту на поле 8×8 горит хотя бы один Пиксель: три попытки
    /// вспышки за такт при семитактовой жизни вспышки гарантируют свечение.
    #[test]
    fn lit_by_tenth_tick() {
        let mut confetti = Confetti::new(mapping8());
        let mut buffer = [Rgb8::BLACK; 64];
        let mut frame = frame8(&mut buffer);

        for tick in 0..=10_u32 {
            confetti.render(&mut frame, tick * TICK_MS);
        }

        let lit = frame.pixels().iter().filter(|c| **c != Rgb8::BLACK).count();
        assert!(lit >= 1, "к 10-му такту всё поле чёрное");
    }

    /// Кадр заполняется целиком: мусор от прошлых Кадров не переживает
    /// рендер.
    #[test]
    fn dirty_buffer_is_cleared() {
        let mut confetti = Confetti::new(mapping8());
        let mut buffer = [JUNK; 64];
        let mut frame = frame8(&mut buffer);

        confetti.render(&mut frame, 10 * TICK_MS);

        assert!(frame.pixels().iter().all(|c| *c != JUNK));
    }

    /// Эффект — чистая функция истории тактов: два экземпляра с одним сидом
    /// на один и тот же момент рисуют бит-в-бит одинаковый Кадр.
    #[test]
    fn same_time_same_frame() {
        let mut first = Confetti::new(mapping8());
        let mut second = Confetti::new(mapping8());
        let mut left = [Rgb8::BLACK; 64];
        let mut right = [Rgb8::BLACK; 64];

        first.render(&mut Frame::new(8, 8, &mut left).unwrap(), 10 * TICK_MS);
        second.render(&mut Frame::new(8, 8, &mut right).unwrap(), 10 * TICK_MS);

        assert_eq!(left, right);
    }

    /// Шкала времени не переполняет u32: кадр на границе шкалы (почти
    /// 49,7 суток) считается без паники, мусор в буфере не переживает.
    #[test]
    fn elapsed_at_u32_max_does_not_overflow() {
        let mut confetti = Confetti::new(mapping8());
        let mut buffer = [JUNK; 64];
        let mut frame = frame8(&mut buffer);

        confetti.render(&mut frame, u32::MAX);

        assert!(frame.pixels().iter().all(|c| *c != JUNK));
    }

    /// Вырожденный пустой Кадр (0×0) не паникует.
    #[test]
    fn empty_frame_is_a_noop() {
        let mut confetti = Confetti::new(mapping8());
        let mut buffer = [];
        let mut frame = Frame::new(0, 0, &mut buffer).unwrap();

        confetti.render(&mut frame, 0);
    }
}
