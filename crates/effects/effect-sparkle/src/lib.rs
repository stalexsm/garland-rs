//! Аддон «Мерцание-снежинки»: снежинки мерцают на тёмной Матрице.
//!
//! Каждый Пиксель — снежинка со своей фазой: раз в цикл она мгновенно
//! вспыхивает холодным белым и тает квадратичным затуханием. Фазы соседей
//! перемешаны, вспышки не выстраиваются в волну; рассев детерминированный,
//! время приходит снаружи — кадры проверяются тестами на хосте.

#![no_std]

use garland_core::{Effect, Frame, Rgb8};

/// Цикл снежинки, мс: каждая вспыхивает раз в 4 с — на Стенде 8×8 горит
/// примерно десяток снежинок разом, плотное искристое мерцание.
const CYCLE_MS: u32 = 4000;

/// Время таяния вспышки, мс: снежинка загорается мгновенно и гаснет
/// за полсекунды — остальное время цикла Пиксель тёмный.
const FADE_MS: u32 = 500;

/// Цвет снежинки: холодный белый — как снег в «Дожде и снеге».
const SNOW: Rgb8 = Rgb8 {
    r: 235,
    g: 240,
    b: 255,
};

/// Константы финализатора splitmix64: умножение на нечётное и сдвиг
/// с исключением перемешивают номер Пикселя так, что фазы соседей не
/// выстраиваются в волну (один раунд умножения-сдвига даёт прогрессию).
/// Сдвиги — шаги каноничного финализатора: чётный, нечётный, финальный.
const GOLDEN_GAMMA: u64 = 0x9e37_79b9_7f4a_7c15;
const MIX_A: u64 = 0xbf58_476d_1ce4_e5b9;
const MIX_B: u64 = 0x94d0_49bb_1331_11eb;
const SHIFT_EVEN: u32 = 30;
const SHIFT_ODD: u32 = 27;
const SHIFT_FINAL: u32 = 31;

/// Фаза вспышки Пикселя `index` — момент внутри цикла, когда снежинка
/// загорается: детерминированный рассев без генератора случайных чисел —
/// картинка одинакова на хосте и устройстве.
fn flash_phase(index: u32) -> u32 {
    let mut mixed = u64::from(index).wrapping_add(GOLDEN_GAMMA);
    mixed = (mixed ^ (mixed >> SHIFT_EVEN)).wrapping_mul(MIX_A);
    mixed = (mixed ^ (mixed >> SHIFT_ODD)).wrapping_mul(MIX_B);
    ((mixed ^ (mixed >> SHIFT_FINAL)) % u64::from(CYCLE_MS)) as u32
}

/// Яркость снежинки через `local_ms` от начала вспышки: мгновенный пик
/// 255 и квадратичное таяние — быстро оторвалось, долго догорает.
fn twinkle(local_ms: u32) -> u8 {
    if local_ms >= FADE_MS {
        return 0;
    }
    let rest = FADE_MS - local_ms;
    (u32::from(u8::MAX) * rest * rest / (FADE_MS * FADE_MS)) as u8
}

/// Цвет `c` в яркости `v` из 255: поканальное умножение без округления.
fn scaled(c: Rgb8, v: u8) -> Rgb8 {
    let channel = |value: u8| (u16::from(value) * u16::from(v) / u16::from(u8::MAX)) as u8;
    Rgb8 {
        r: channel(c.r),
        g: channel(c.g),
        b: channel(c.b),
    }
}

/// Мерцание-снежинки.
pub struct Sparkle;

impl Effect for Sparkle {
    fn render(&mut self, frame: &mut Frame<'_>, elapsed_ms: u32) {
        // Контракт Эффекта — заполнить Кадр целиком: вспышки занимают
        // отдельные Пиксели, остальное гасится (Фреймбуферы Прошивки
        // переиспользуются между Кадрами).
        frame.fill(Rgb8::BLACK);
        for (index, pixel) in frame.pixels_mut().iter_mut().enumerate() {
            // Локальное время от начала вспышки: разность с фазой — в u64,
            // шкала времени wrapping, переполнения u32 не будет никогда.
            let local = (u64::from(elapsed_ms) + u64::from(CYCLE_MS)
                - u64::from(flash_phase(index as u32)))
                % u64::from(CYCLE_MS);
            *pixel = scaled(SNOW, twinkle(local as u32));
        }
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

    /// Фаза вспышки Пикселя 0 — 3535 мс от начала цикла (выкладка микшера
    /// фаз): в этот момент снежинка 0 стоит на пике — чистый холодный
    /// белый на грязном переиспользуемом Фреймбуфере.
    #[test]
    fn first_snowflake_flashes_at_its_phase() {
        let mut sparkle = Sparkle;
        let mut buffer = [Rgb8 { r: 9, g: 9, b: 9 }; 64];
        let mut frame = Frame::new(8, 8, &mut buffer).unwrap();

        sparkle.render(&mut frame, 3535);

        assert_eq!(frame.pixels()[0], SNOW);
    }

    /// Снежинка тает квадратично (ручные выкладки): через 250 мс после
    /// вспышки — 63 из 255, цвет (58, 59, 63); через 400 мс — 10 из 255,
    /// цвет (9, 9, 10).
    #[test]
    fn snowflake_melts_quadratically() {
        let mut sparkle = Sparkle;
        let mut buffer = [Rgb8::BLACK; 64];
        let mut frame = Frame::new(8, 8, &mut buffer).unwrap();

        sparkle.render(&mut frame, 3535 + 250);
        assert_eq!(
            frame.pixels()[0],
            Rgb8 {
                r: 58,
                g: 59,
                b: 63
            }
        );

        sparkle.render(&mut frame, 3535 + 400);
        assert_eq!(frame.pixels()[0], Rgb8 { r: 9, g: 9, b: 10 });
    }

    /// Растаявшая снежинка темна до следующего цикла: и сразу после таяния
    /// (спустя 500 мс после вспышки), и за миллисекунду до следующей.
    #[test]
    fn snowflake_is_dark_between_flashes() {
        let mut sparkle = Sparkle;
        let mut after = [Rgb8::BLACK; 64];
        let mut before = [Rgb8::BLACK; 64];

        sparkle.render(&mut Frame::new(8, 8, &mut after).unwrap(), 3535 + 500);
        sparkle.render(&mut Frame::new(8, 8, &mut before).unwrap(), 3534);

        assert_eq!(after[0], Rgb8::BLACK);
        assert_eq!(before[0], Rgb8::BLACK);
    }

    /// Вспышка повторяется каждый цикл: на пике каждой из первых двух
    /// итераций Пиксель 0 — снова чистый снег.
    #[test]
    fn flash_repeats_every_cycle() {
        let mut sparkle = Sparkle;
        let mut first = [Rgb8::BLACK; 64];
        let mut second = [Rgb8::BLACK; 64];

        sparkle.render(&mut Frame::new(8, 8, &mut first).unwrap(), 3535);
        sparkle.render(&mut Frame::new(8, 8, &mut second).unwrap(), 3535 + CYCLE_MS);

        assert_eq!(first[0], SNOW);
        assert_eq!(second[0], SNOW);
    }

    /// Фазы разнесены: на пике Пикселя 0 горят единицы снежинок (окно
    /// таяния 500 мс из цикла 4000 мс), а не вся Лента разом.
    #[test]
    fn twinkles_are_staggered() {
        let mut sparkle = Sparkle;
        let mut buffer = [Rgb8::BLACK; 64];
        let mut frame = Frame::new(8, 8, &mut buffer).unwrap();

        sparkle.render(&mut frame, 3535);

        let lit = frame.pixels().iter().filter(|c| **c != Rgb8::BLACK).count();
        assert_eq!(frame.pixels()[0], SNOW);
        assert!(lit >= 2, "мерцает меньше двух снежинок: {lit}");
        assert!(lit < 32, "мерцает пол-Ленты разом: {lit}");
    }

    /// Эффект — чистая функция времени: новые экземпляры на один и тот же
    /// момент рисуют бит в бит одинаковый Кадр.
    #[test]
    fn same_time_same_frame() {
        let mut first = Sparkle;
        let mut second = Sparkle;
        let mut left = [Rgb8::BLACK; 64];
        let mut right = [Rgb8::BLACK; 64];

        first.render(&mut Frame::new(8, 8, &mut left).unwrap(), 999);
        second.render(&mut Frame::new(8, 8, &mut right).unwrap(), 999);

        assert_eq!(left, right);
    }

    /// Кадр заполняется целиком: мусор от прошлых Кадров не переживает
    /// рендер.
    #[test]
    fn dirty_buffer_is_cleared() {
        let mut sparkle = Sparkle;
        let mut buffer = [Rgb8 { r: 255, g: 0, b: 0 }; 64];
        let mut frame = Frame::new(8, 8, &mut buffer).unwrap();

        sparkle.render(&mut frame, 999);

        assert!(
            frame
                .pixels()
                .iter()
                .all(|c| c.r != 255 || c.g != 0 || c.b != 0)
        );
    }

    /// Вырожденный пустой Кадр (0×0) не паникует.
    #[test]
    fn empty_frame_is_a_noop() {
        let mut sparkle = Sparkle;
        let mut buffer = [];
        let mut frame = Frame::new(0, 0, &mut buffer).unwrap();

        sparkle.render(&mut frame, 0);
    }

    /// Шкала времени не переполняет u32: кадр на границе шкалы (почти
    /// 49,7 суток) считается без паники, мусор в буфере не переживает.
    #[test]
    fn elapsed_at_u32_max_does_not_overflow() {
        let mut sparkle = Sparkle;
        let mut buffer = [Rgb8 { r: 9, g: 9, b: 9 }; 64];
        let mut frame = Frame::new(8, 8, &mut buffer).unwrap();

        sparkle.render(&mut frame, u32::MAX);

        assert!(
            frame
                .pixels()
                .iter()
                .all(|c| !(c.r == 9 && c.g == 9 && c.b == 9))
        );
    }
}
