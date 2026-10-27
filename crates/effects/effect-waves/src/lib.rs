//! Аддон «Синусоиды»: две цветные волны бегут по Матрице вправо.
//!
//! Эффект из демо-цикла старой BT-версии прошивки: в новой версии
//! исходника его нет.
//!
//! Каждый такт кадр в постоянном поле сдвигается на Пиксель вправо, левый
//! столбец гаснет, и в него ставятся точки двух синусоид: строка точки —
//! центр Матрицы плюс амплитуда, умноженная на синус растущей фазы
//! (в исходнике точка ставится в столбец, который кадр тянет вправо).
//! Отклонения от оригинала: синус берётся из таблицы [`sin8`] ядра
//! (круг 256 единиц вместо `sin()` на float); фаза волны хранится целиком
//! (0..359 — в оригинале `random(0, 360)` обрезалась до байта), а бросок
//! фазы — формула `random8` с широким пределом, потому что байтовый
//! предел `random8_below` ядра не вмещает 360; ось Y перевёрнута, здесь
//! y = 0 — верх Матрицы. Порядок бросков ГПСЧ при инициализации волн
//! фиксирован и входит в детерминизм порта.

#![no_std]

use garland_core::{
    Effect, Frame, Hsv8, Mapping, NOISE_MAX_SIDE, Rgb8, Rng16, advance_anchor, hsv2rgb_rainbow,
    sin8, steps_since,
};

/// Период такта, мс: пошаговые эффекты живут с темпом ~12 тактов/с.
const TICK_MS: u32 = 80;

/// Количество синусоид (`WAVES_AMOUNT` оригинала).
const WAVES_AMOUNT: usize = 2;

/// Нижняя граница частоты, градусов за такт (`random(17, 25)` оригинала).
const FREQ_MIN: u8 = 17;

/// Верхняя (исключающая) граница частоты.
const FREQ_LIMIT: u8 = 25;

/// Нижняя граница доли амплитуды (`random(4, 11) / 10` оригинала).
const AMP_MIN: u8 = 4;

/// Верхняя (исключающая) граница доли амплитуды.
const AMP_LIMIT: u8 = 11;

/// Знаменатель доли амплитуды: амплитуда — полувысота, умноженная на долю
/// 0,4..1,0 и поделённая на 10.
const AMP_DENOM: u16 = 10;

/// Полный круг фазы, градусов.
const DEG_CIRCLE: u32 = 360;

/// Предел броска фазы: случайная фаза 0..359 градусов (`random(0, 360)`).
const PHASE_LIMIT: u16 = 360;

/// Случайная фаза 0..359: формула [`Rng16::random8_below`] с широким
/// пределом — `random8_below(360)` задания не помещается в байтовый
/// предел метода ядра, формула та же: `(random8 · 360) >> 8`.
fn phase_roll(rng: &mut Rng16) -> u16 {
    (u16::from(rng.random8()) * PHASE_LIMIT) >> 8
}

/// Полный круг синуса ядра FastLED: [`sin8`] меряет угол в 1/256 круга.
const SIN_CIRCLE: u32 = 256;

/// Ноль синуса [`sin8`]: середина байта 0..255.
const SIN_ZERO: i16 = 128;

/// Делитель масштаба: смещение точки — амплитуда, умноженная на знаковый
/// синус и поделённая на [`SIN_ZERO`].
const SIN_SCALE: i16 = SIN_ZERO;

/// Предел броска тона: 9 дискретных оттенков (`random(0, 9) * 28` оригинала).
const HUE_COUNT: u8 = 9;

/// Шаг тона между соседними оттенками палитры волн.
const HUE_STEP: u8 = 28;

/// Полные насыщенность и яркость волн.
const FULL: u8 = 255;

/// Потолок размера Матрицы: постоянное поле квадрат 64×64 Пикселя, как у
/// шумовых эффектов ядра.
const FIELD_CAP: usize = (NOISE_MAX_SIDE * NOISE_MAX_SIDE) as usize;

/// Синусоиды.
pub struct Waves {
    /// Ширина Матрицы, Пикселей.
    w: u16,
    /// Высота Матрицы, Пикселей.
    h: u16,
    /// Детерминированный ГПСЧ FastLED: параметры и цвета волн.
    rng: Rng16,
    /// Постоянное поле кадра (аналог `leds[]` исходного алгоритма), строка-мажорно.
    field: [Rgb8; FIELD_CAP],
    /// Частоты волн, градусов за такт (`w[j]` оригинала).
    freq: [u8; WAVES_AMOUNT],
    /// Фазы волн, градусов (`phi[j]` оригинала).
    phase: [u16; WAVES_AMOUNT],
    /// Амплитуды волн, Пикселей (`A[j]` оригинала).
    amp: [u8; WAVES_AMOUNT],
    /// Цвета волн — 9 дискретных оттенков палитры исходного алгоритма.
    colors: [Rgb8; WAVES_AMOUNT],
    /// Накопленная фаза, градусов (`t` оригинала).
    t: u32,
    /// Момент времени последнего такта, мс.
    last_ms: u32,
}

impl Waves {
    /// Синусоиды для Матрицы из [`Mapping`]: размеры и параметры волн
    /// запоминаются до первого кадра.
    pub fn new(mapping: Mapping) -> Self {
        let mut waves = Self {
            w: mapping.width(),
            h: mapping.height(),
            rng: Rng16::new(),
            field: [Rgb8::BLACK; FIELD_CAP],
            freq: [0; WAVES_AMOUNT],
            phase: [0; WAVES_AMOUNT],
            amp: [0; WAVES_AMOUNT],
            colors: [Rgb8::BLACK; WAVES_AMOUNT],
            t: 0,
            last_ms: 0,
        };
        // Параметры волн: частота 17..24 градусов за такт, фаза 0..359,
        // амплитуда — доля 0,4..1,0 от полувысоты, цвет — один из 9
        // дискретных оттенков; порядок бросков ГПСЧ — как у оригинала.
        for j in 0..WAVES_AMOUNT {
            waves.freq[j] = waves.rng.random8_range(FREQ_MIN, FREQ_LIMIT);
            waves.phase[j] = phase_roll(&mut waves.rng);
            let half = waves.h / 2;
            let share = u16::from(waves.rng.random8_range(AMP_MIN, AMP_LIMIT));
            waves.amp[j] = (half * share / AMP_DENOM) as u8;
            waves.colors[j] = hsv2rgb_rainbow(Hsv8 {
                h: waves.rng.random8_below(HUE_COUNT) * HUE_STEP,
                s: FULL,
                v: FULL,
            });
        }
        waves
    }

    /// Один такт: сдвиг кадра вправо, чёрный левый столбец, точки волн.
    fn step(&mut self) {
        let w = self.w as usize;
        // Сдвиг всех Пикселей вправо: обход с конца читает ещё не сдвинутое.
        for x in (1..w).rev() {
            for y in 0..self.h as usize {
                self.field[y * w + x] = self.field[y * w + x - 1];
            }
        }
        // Левый столбец гаснет.
        for y in 0..self.h as usize {
            self.field[y * w] = Rgb8::BLACK;
        }
        // Фаза растёт каждым тактом и оборачивается через круг градусов.
        self.t += 1;
        if self.t > DEG_CIRCLE {
            self.t = 0;
        }
        // Точка каждой волны: строка исходного алгоритма «центр + синус фазы»,
        // перевёрнутая в нашу систему (y = 0 — верх Матрицы).
        for j in 0..WAVES_AMOUNT {
            let deg = (u32::from(self.freq[j]) * self.t + u32::from(self.phase[j])) % DEG_CIRCLE;
            let theta = (deg * SIN_CIRCLE / DEG_CIRCLE) as u8;
            let off = (i16::from(self.amp[j]) * (i16::from(sin8(theta)) - SIN_ZERO)) / SIN_SCALE;
            let gy = i32::from(self.h) / 2 + i32::from(off);
            let y = i32::from(self.h) - 1 - gy;
            if y >= 0 && y < i32::from(self.h) {
                self.field[y as usize * w] = self.colors[j];
            }
        }
    }

    /// Выложить поле в Кадр целиком: за пределами поля — чёрный.
    fn draw(&self, frame: &mut Frame<'_>) {
        let (w, h) = (self.w as usize, self.h as usize);
        for y in 0..frame.height() as usize {
            for x in 0..frame.width() as usize {
                let c = if x < w && y < h {
                    self.field[y * w + x]
                } else {
                    Rgb8::BLACK
                };
                frame.set(x as u16, y as u16, c);
            }
        }
    }
}

impl Effect for Waves {
    fn render(&mut self, frame: &mut Frame<'_>, elapsed_ms: u32) {
        if frame.is_empty() {
            return;
        }
        let steps = steps_since(self.last_ms, elapsed_ms, TICK_MS);
        self.last_ms = advance_anchor(self.last_ms, elapsed_ms, TICK_MS);
        if steps > 0 {
            for _ in 0..steps {
                self.step();
            }
        }
        self.draw(frame);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use garland_core::{Corner, Layout};

    /// Ширина тестовой Матрицы, Пикселей.
    const W: usize = 8;
    /// Высота тестовой Матрицы, Пикселей.
    const H: usize = 32;

    /// Цвет волны 0 — литерал теста: тон 8·28 = 224 (пурпурный).
    const WAVE_0: Rgb8 = Rgb8 {
        r: 170,
        g: 0,
        b: 85,
    };
    /// Цвет волны 1 — литерал теста: тон 5·28 = 140 (морской).
    const WAVE_1: Rgb8 = Rgb8 {
        r: 0,
        g: 107,
        b: 149,
    };

    /// Привязка Матрицы 8×32: на высокой Матрице амплитуды волн различимы.
    fn mapping() -> Mapping {
        Mapping::new(W as u16, H as u16, Layout::RowPlain, Corner::TopLeft)
    }

    /// Точки волн появляются в левом столбце каждым тактом: на такте 1 —
    /// волна 1 (фаза 101) в строке 8, волна 0 (фаза 188) в строке 18; на
    /// такте 2 фазы выросли — точки в строках 9 и 20.
    #[test]
    fn wave_dots_appear_in_left_column() {
        let mut waves = Waves::new(mapping());
        let mut buffer = [Rgb8::BLACK; W * H];
        let mut frame = Frame::new(W as u16, H as u16, &mut buffer).unwrap();

        waves.render(&mut frame, TICK_MS);
        assert_eq!(
            frame.get(0, 8),
            Some(WAVE_1),
            "точка волны 1 в левом столбце"
        );
        assert_eq!(
            frame.get(0, 18),
            Some(WAVE_0),
            "точка волны 0 в левом столбце"
        );

        waves.render(&mut frame, TICK_MS * 2);
        assert_eq!(frame.get(0, 9), Some(WAVE_1), "фаза волны 1 выросла");
        assert_eq!(frame.get(0, 20), Some(WAVE_0), "фаза волны 0 выросла");
    }

    /// Картинка уезжает вправо: кадр с такта 10 сдвигается на Пиксель за
    /// такт — клетка (x, y) на такте 10+x равна клетке (0, y) на такте 10.
    #[test]
    fn picture_shifts_right() {
        let mut waves = Waves::new(mapping());
        let mut buffer = [Rgb8::BLACK; W * H];
        let mut frame = Frame::new(W as u16, H as u16, &mut buffer).unwrap();
        let mut history = [[Rgb8::BLACK; W * H]; 4];

        for tick in 1..=13 {
            waves.render(&mut frame, TICK_MS * tick);
            if tick >= 10 {
                history[(tick - 10) as usize].copy_from_slice(frame.pixels());
            }
        }

        for y in 0..H {
            let base = history[0][y * W];
            assert_eq!(
                history[1][y * W + 1],
                base,
                "такт 11: (1, {y}) — копия (0, {y})"
            );
            assert_eq!(
                history[2][y * W + 2],
                base,
                "такт 12: (2, {y}) — копия (0, {y})"
            );
            assert_eq!(
                history[3][y * W + 3],
                base,
                "такт 13: (3, {y}) — копия (0, {y})"
            );
        }
    }

    /// Точки не выходят за пределы амплитуды: обе волны получили амплитуды
    /// 8 и 9 Пикселей (сид ГПСЧ), центр высоты 32 — строка 15, поэтому все
    /// точки левого столбца за 100 тактов живут в полосе 6..24; до первого
    /// наложения точек (такт 27) их ровно две.
    #[test]
    fn dots_stay_within_amplitude() {
        let mut waves = Waves::new(mapping());
        let mut buffer = [Rgb8::BLACK; W * H];
        let mut frame = Frame::new(W as u16, H as u16, &mut buffer).unwrap();

        let (center, amp_max) = (15usize, 9usize);
        for tick in 1..=100 {
            waves.render(&mut frame, TICK_MS * tick);
            let mut dots = 0;
            for y in 0..H {
                if frame.get(0, y as u16) != Some(Rgb8::BLACK) {
                    dots += 1;
                    assert!(
                        (center - amp_max..=center + amp_max).contains(&y),
                        "такт {tick}: точка y={y} вышла из полосы амплитуды"
                    );
                }
            }
            if tick <= 26 {
                assert_eq!(dots, 2, "такт {tick}: в левом столбце не две точки");
            }
        }
    }

    /// Эффект — чистая функция истории тактов: два экземпляра на один
    /// момент дают бит-в-бит равные кадры.
    #[test]
    fn same_history_same_frame() {
        let mut first = Waves::new(mapping());
        let mut second = Waves::new(mapping());
        let mut left = [Rgb8::BLACK; W * H];
        let mut right = [Rgb8::BLACK; W * H];

        first.render(&mut Frame::new(W as u16, H as u16, &mut left).unwrap(), 800);
        second.render(
            &mut Frame::new(W as u16, H as u16, &mut right).unwrap(),
            800,
        );

        assert_eq!(left, right);
    }

    /// Кадр заполняется целиком: мусор от прошлых Кадров не переживает рендер.
    #[test]
    fn dirty_buffer_is_cleared() {
        let mut waves = Waves::new(mapping());
        let mut buffer = [Rgb8 {
            r: 200,
            g: 200,
            b: 200,
        }; W * H];
        let mut frame = Frame::new(W as u16, H as u16, &mut buffer).unwrap();

        waves.render(&mut frame, TICK_MS);

        assert_eq!(frame.get(0, 8), Some(WAVE_1), "точка волны на месте");
        assert!(
            frame
                .pixels()
                .iter()
                .all(|c| !(c.r == 200 && c.g == 200 && c.b == 200)),
            "серый мусор остался в Кадре"
        );
    }

    /// Вырожденный пустой Кадр (0×0) — no-op без паники.
    #[test]
    fn empty_frame_is_a_noop() {
        let mut waves = Waves::new(mapping());
        let mut buffer = [];
        let mut frame = Frame::new(0, 0, &mut buffer).unwrap();

        waves.render(&mut frame, 0);
        waves.render(&mut frame, u32::MAX);
    }

    /// Шкала времени не переполняет u32: кадр на границе шкалы (почти
    /// 49,7 суток) считается без паники, догоняющих тактов не больше
    /// потолка ядра, точки волн в кадре есть.
    #[test]
    fn elapsed_at_u32_max_does_not_overflow() {
        let mut waves = Waves::new(mapping());
        let mut buffer = [Rgb8::BLACK; W * H];
        let mut frame = Frame::new(W as u16, H as u16, &mut buffer).unwrap();

        waves.render(&mut frame, u32::MAX);

        let dots = (0..H)
            .filter(|y| frame.get(0, *y as u16) != Some(Rgb8::BLACK))
            .count();
        assert_eq!(dots, 0, "сброс шкалы не должен двигать волны");
    }
}
