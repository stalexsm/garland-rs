//! Аддон «Пульс»: вся Лента дышит яркостью.
//!
//! Кадр целиком заливается цветом Аддона, умноженным на яркость дыхания:
//! от чёрного до полной и обратно, синусоподобная парабола без float.
//! Цвет и период фиксированы константами Аддона — параметры эффектов
//! в Прошивке v1 не настраиваются. Время приходит снаружи, кадры
//! детерминированны и проверяются тестами на хосте.

#![no_std]

use garland_core::{Effect, Frame, Rgb8};

/// Период дыхания, мс: вдох-выдох за 4 с — темп спокойного дыхания.
const PERIOD_MS: u32 = 4000;

/// Полный размах фазы дыхания в шагах: парабола дышит на 0..256,
/// нули по краям, максимум в середине.
const BREATH_STEPS: u32 = 256;

/// Коэффициент параболы 4p(256−p): поднимает горб до единицы в середине.
const PARABOLA_GAIN: u32 = 4;

/// Пик параболы `4p(256−p)` в середине размаха — знаменатель яркости:
/// 4·128·128 = 65536.
const BREATH_PEAK: u32 = BREATH_STEPS * BREATH_STEPS;

/// Цвет Аддона: тёплый янтарный — заметно отличается от статичного
/// тёплого белого.
const AMBER: Rgb8 = Rgb8 {
    r: 255,
    g: 130,
    b: 20,
};

/// Яркость дыхания `0..255` на момент `elapsed_ms`: фаза периода
/// разворачивается в параболу `4p(256−p)` — дешёвая целочисленная замена
/// синуса без таблиц и float: нули по краям, плавный максимум в середине.
fn breath_brightness(elapsed_ms: u32) -> u8 {
    let phase = (elapsed_ms % PERIOD_MS) * BREATH_STEPS / PERIOD_MS;
    (PARABOLA_GAIN * phase * (BREATH_STEPS - phase) * u32::from(u8::MAX) / BREATH_PEAK) as u8
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

/// Пульс — дыхание яркостью.
pub struct Pulse;

impl Effect for Pulse {
    fn render(&mut self, frame: &mut Frame<'_>, elapsed_ms: u32) {
        frame.fill(scaled(AMBER, breath_brightness(elapsed_ms)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use garland_core::Rgb8;

    /// Цвет Аддона — литерал теста, независимый от константы кода:
    /// случайная правка цвета ловится этим литералом.
    const AMBER: Rgb8 = Rgb8 {
        r: 255,
        g: 130,
        b: 20,
    };

    /// Дыхание начинается с тёмного Кадра: на старте Лента погашена,
    /// мусор переиспользуемого Фреймбуфера не переживает рендер.
    #[test]
    fn breath_starts_dark() {
        let mut pulse = Pulse;
        let mut buffer = [Rgb8 { r: 9, g: 9, b: 9 }; 12];
        let mut frame = Frame::new(4, 3, &mut buffer).unwrap();

        pulse.render(&mut frame, 0);

        assert_eq!(frame.pixels(), &[Rgb8::BLACK; 12]);
    }

    /// За четверть периода Лента доходит до 191 из 255 (ручная выкладка:
    /// фаза 64, парабола 4·64·192·255/65536 = 191), весь Кадр — янтарный
    /// цвет этой яркости: (191, 97, 14).
    #[test]
    fn quarter_period_glows_at_worked_example_brightness() {
        let mut pulse = Pulse;
        let mut buffer = [Rgb8::BLACK; 12];
        let mut frame = Frame::new(4, 3, &mut buffer).unwrap();

        pulse.render(&mut frame, PERIOD_MS / 4);

        assert_eq!(
            frame.pixels(),
            &[Rgb8 {
                r: 191,
                g: 97,
                b: 14
            }; 12]
        );
    }

    /// В середине периода дыхание доходит до полной яркости: весь Кадр —
    /// чистый янтарный литерал.
    #[test]
    fn half_period_is_the_full_color() {
        let mut pulse = Pulse;
        let mut buffer = [Rgb8::BLACK; 12];
        let mut frame = Frame::new(4, 3, &mut buffer).unwrap();

        pulse.render(&mut frame, PERIOD_MS / 2);

        assert_eq!(frame.pixels(), &[AMBER; 12]);
    }

    /// Дыхание периодично: Кадры через полный период совпадают бит в бит
    /// (и на подъёме, и возле пика).
    #[test]
    fn breath_wraps_around_the_period() {
        let mut pulse = Pulse;
        let mut first = [Rgb8::BLACK; 12];
        let mut second = [Rgb8::BLACK; 12];

        pulse.render(&mut Frame::new(4, 3, &mut first).unwrap(), 137);
        pulse.render(&mut Frame::new(4, 3, &mut second).unwrap(), 137 + PERIOD_MS);

        assert_eq!(first, second);
    }

    /// Эффект — чистая функция времени: новые экземпляры на один и тот же
    /// момент рисуют бит в бит одинаковый Кадр.
    #[test]
    fn same_time_same_frame() {
        let mut first = Pulse;
        let mut second = Pulse;
        let mut left = [Rgb8::BLACK; 12];
        let mut right = [Rgb8::BLACK; 12];

        first.render(&mut Frame::new(4, 3, &mut left).unwrap(), 999);
        second.render(&mut Frame::new(4, 3, &mut right).unwrap(), 999);

        assert_eq!(left, right);
    }

    /// Вырожденный пустой Кадр (0×0) не паникует.
    #[test]
    fn empty_frame_is_a_noop() {
        let mut pulse = Pulse;
        let mut buffer = [];
        let mut frame = Frame::new(0, 0, &mut buffer).unwrap();

        pulse.render(&mut frame, 0);
    }

    /// Шкала времени не переполняет u32: кадр на границе шкалы считается
    /// без паники и совпадает с кадром той же фазы периода — дыхание
    /// периодично и на границе шкалы.
    #[test]
    fn elapsed_at_u32_max_does_not_overflow() {
        let mut border = Pulse;
        let mut border_buffer = [Rgb8::BLACK; 12];
        border.render(&mut Frame::new(4, 3, &mut border_buffer).unwrap(), u32::MAX);

        let mut same_phase = Pulse;
        let mut same_phase_buffer = [Rgb8::BLACK; 12];
        same_phase.render(
            &mut Frame::new(4, 3, &mut same_phase_buffer).unwrap(),
            u32::MAX % PERIOD_MS,
        );

        assert_eq!(
            border_buffer, same_phase_buffer,
            "дыхание периодично на границе шкалы"
        );
    }
}
