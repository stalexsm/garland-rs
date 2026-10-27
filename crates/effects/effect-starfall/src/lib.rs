//! Аддон «Звездопад»: кометы летят по диагонали вниз-вправо, оставляя
//! гаснущие хвосты.
//!
//!
//! Головы комет рождаются на верхней строке (левая половина) и левом крае
//! (верхняя половина) — в исходнике это нижняя строка и левый край: ось Y
//! перевёрнута, здесь y = 0 — верх Матрицы. Раз в такт клетка линии
//! рождения с шансом 1/60 окрашивается случайным тоном; каждый такт всё
//! содержимое поля съезжает на Пиксель по диагонали вниз-вправо, а линии
//! рождения гаснут — за кометой тянется хвост. Состояние — постоянное
//! поле [`Rgb8`] (аналог `leds[]` исходного алгоритма), случайность — детерминированный
//! [`Rng16`]: кадры одинаковы на хосте и устройстве. Отклонение от
//! оригинала одно: порядок скана линий рождения при спавне зафиксирован
//! (сначала верхняя строка слева направо, затем левый край сверху вниз) —
//! порядок определяет расход ГПСЧ и входит в детерминизм порта.

#![no_std]

use garland_core::{
    Effect, Frame, Hsv8, Mapping, NOISE_MAX_SIDE, Rgb8, Rng16, advance_anchor, fade_pixel,
    hsv2rgb_rainbow, steps_since,
};

/// Период такта, мс: пошаговые эффекты живут с темпом ~12 тактов/с.
const TICK_MS: u32 = 80;

/// Обратная плотность спавна: комета рождается с шансом 1/60 за клетку
/// линии рождения за такт (`STAR_DENSE` оригинала).
const STAR_DENSE: u8 = 60;

/// Насыщенность комет (`SATURATION` оригинала): цвета чуть разбелены.
const SATURATION: u8 = 150;

/// Полная яркость голов комет.
const FULL_VALUE: u8 = 255;

/// Предел броска тона: случайный тон 0..199 — без зелёно-бирюзовой трети
/// круга (`random(0, 200)` оригинала).
const HUE_LIMIT: u8 = 200;

/// Шаг угасания линии рождения: хвост тускнеет на 100/256 за такт
/// (`TAIL_STEP` оригинала).
const TAIL_STEP: u8 = 100;

/// Потолок размера Матрицы: постоянное поле квадрат 64×64 Пикселя, как у
/// шумовых эффектов ядра.
const FIELD_CAP: usize = (NOISE_MAX_SIDE * NOISE_MAX_SIDE) as usize;

/// Звездопад.
pub struct Starfall {
    /// Ширина Матрицы, Пикселей.
    w: u16,
    /// Высота Матрицы, Пикселей.
    h: u16,
    /// Детерминированный ГПСЧ FastLED: броски спавна и тона комет.
    rng: Rng16,
    /// Постоянное поле кадра (аналог `leds[]` исходного алгоритма), строка-мажорно.
    field: [Rgb8; FIELD_CAP],
    /// Момент времени последнего такта, мс.
    last_ms: u32,
}

impl Starfall {
    /// Звездопад для Матрицы из [`Mapping`]: размеры запоминаются один раз.
    pub fn new(mapping: Mapping) -> Self {
        Self {
            w: mapping.width(),
            h: mapping.height(),
            rng: Rng16::new(),
            field: [Rgb8::BLACK; FIELD_CAP],
            last_ms: 0,
        }
    }

    /// Пиксель поля; за краем Матрицы — чёрный: соседи клеток рождения у
    /// края читаются как пустые, как `getPixColorXY` оригинала.
    fn get(&self, x: i32, y: i32) -> Rgb8 {
        if x < 0 || y < 0 || x >= i32::from(self.w) || y >= i32::from(self.h) {
            return Rgb8::BLACK;
        }
        self.field[y as usize * self.w as usize + x as usize]
    }

    /// Один такт: спавн голов, диагональный сдвиг, угасание линий рождения.
    fn step(&mut self) {
        let w = self.w as usize;
        // Спавн на верхней строке (левая половина): клетка пуста → бросок
        // кубика → оба соседа по строке пусты → голова случайного тона.
        for x in 0..self.w / 2 {
            if self.get(i32::from(x), 0) == Rgb8::BLACK
                && self.rng.random8_below(STAR_DENSE) == 0
                && self.get(i32::from(x) + 1, 0) == Rgb8::BLACK
                && self.get(i32::from(x) - 1, 0) == Rgb8::BLACK
            {
                self.field[x as usize] = hsv2rgb_rainbow(Hsv8 {
                    h: self.rng.random8_below(HUE_LIMIT),
                    s: SATURATION,
                    v: FULL_VALUE,
                });
            }
        }
        // Спавн на левом крае (верхняя половина): соседи по столбцу.
        for y in 0..self.h / 2 {
            if self.get(0, i32::from(y)) == Rgb8::BLACK
                && self.rng.random8_below(STAR_DENSE) == 0
                && self.get(0, i32::from(y) + 1) == Rgb8::BLACK
                && self.get(0, i32::from(y) - 1) == Rgb8::BLACK
            {
                self.field[y as usize * w] = hsv2rgb_rainbow(Hsv8 {
                    h: self.rng.random8_below(HUE_LIMIT),
                    s: SATURATION,
                    v: FULL_VALUE,
                });
            }
        }
        // Сдвиг содержимого по диагонали вниз-вправо: обход с конца читает
        // ещё не сдвинутые клетки (в исходнике сдвиг симметричный — вправо-вверх).
        for y in (1..self.h as usize).rev() {
            for x in (1..w).rev() {
                self.field[y * w + x] = self.field[(y - 1) * w + (x - 1)];
            }
        }
        // Хвосты: линии рождения гаснут на шаг за такт.
        for x in 0..self.w / 2 {
            self.field[x as usize] = fade_pixel(self.field[x as usize], TAIL_STEP);
        }
        for y in 0..self.h / 2 {
            self.field[y as usize * w] = fade_pixel(self.field[y as usize * w], TAIL_STEP);
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

impl Effect for Starfall {
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

    /// Цвет первой кометы — литерал теста: тон броска ГПСЧ (44) через
    /// `hsv2rgb_rainbow` при насыщенности 150.
    const COMET: Rgb8 = Rgb8 {
        r: 78,
        g: 237,
        b: 44,
    };

    /// Клетка рождения на такт позже: хвост погас на [`TAIL_STEP`]
    /// (78·156>>8 = 47, 237·156>>8 = 144, 44·156>>8 = 26).
    const COMET_FADED: Rgb8 = Rgb8 {
        r: 47,
        g: 144,
        b: 26,
    };

    /// Привязка Стенда 8×8: Эффекту нужны только размеры.
    fn mapping() -> Mapping {
        Mapping::new(8, 8, Layout::RowPlain, Corner::TopLeft)
    }

    /// Первая комета детерминированного ГПСЧ рождается на такте 4 в клетке
    /// (2, 0) верхней строки и в тот же такт съезжает на (3, 1); клетка
    /// рождения, погаснув на шаг, — её хвост. До того поле пусто.
    #[test]
    fn comet_spawns_on_top_line_and_slides_down_right() {
        let mut starfall = Starfall::new(mapping());
        let mut buffer = [Rgb8::BLACK; 64];
        let mut frame = Frame::new(8, 8, &mut buffer).unwrap();

        starfall.render(&mut frame, TICK_MS * 3);
        assert!(
            frame.pixels().iter().all(|c| *c == Rgb8::BLACK),
            "до такта 4 комет быть не должно"
        );

        starfall.render(&mut frame, TICK_MS * 4);
        assert_eq!(frame.get(3, 1), Some(COMET), "голова съехала по диагонали");
        assert_eq!(
            frame.get(2, 0),
            Some(COMET_FADED),
            "клетка рождения погасла на шаг"
        );
    }

    /// Движение вниз-вправо: на такт позже голова — на (4, 2), за ней хвост.
    #[test]
    fn comet_moves_down_right() {
        let mut starfall = Starfall::new(mapping());
        let mut buffer = [Rgb8::BLACK; 64];
        let mut frame = Frame::new(8, 8, &mut buffer).unwrap();

        starfall.render(&mut frame, TICK_MS * 4);
        starfall.render(&mut frame, TICK_MS * 5);

        assert_eq!(
            frame.get(4, 2),
            Some(COMET),
            "голова ушла на Пиксель вниз-вправо"
        );
        assert_eq!(frame.get(3, 1), Some(COMET_FADED), "позади тянется хвост");
    }

    /// Следы гаснут: клетка рождения тускнеет каждым тактом и к такту 9
    /// погасла совсем (хвосты не тлеют вечно).
    #[test]
    fn trails_die_out() {
        let mut starfall = Starfall::new(mapping());
        let mut buffer = [Rgb8::BLACK; 64];
        let mut frame = Frame::new(8, 8, &mut buffer).unwrap();

        starfall.render(&mut frame, TICK_MS * 4);
        assert_eq!(frame.get(2, 0), Some(COMET_FADED));
        for tick in 5..=9 {
            starfall.render(&mut frame, TICK_MS * tick);
        }

        assert_eq!(frame.get(2, 0), Some(Rgb8::BLACK), "хвост погас");
    }

    /// Эффект — чистая функция истории тактов: два экземпляра на один
    /// момент дают бит-в-бит равные кадры.
    #[test]
    fn same_history_same_frame() {
        let mut first = Starfall::new(mapping());
        let mut second = Starfall::new(mapping());
        let mut left = [Rgb8::BLACK; 64];
        let mut right = [Rgb8::BLACK; 64];

        first.render(&mut Frame::new(8, 8, &mut left).unwrap(), 800);
        second.render(&mut Frame::new(8, 8, &mut right).unwrap(), 800);

        assert_eq!(left, right);
    }

    /// Кадр заполняется целиком: мусор от прошлых Кадров не переживает рендер.
    #[test]
    fn dirty_buffer_is_cleared() {
        let mut starfall = Starfall::new(mapping());
        let mut buffer = [Rgb8 {
            r: 200,
            g: 200,
            b: 200,
        }; 64];
        let mut frame = Frame::new(8, 8, &mut buffer).unwrap();

        starfall.render(&mut frame, TICK_MS * 4);

        assert_eq!(frame.get(3, 1), Some(COMET), "комета на месте");
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
        let mut starfall = Starfall::new(mapping());
        let mut buffer = [];
        let mut frame = Frame::new(0, 0, &mut buffer).unwrap();

        starfall.render(&mut frame, 0);
        starfall.render(&mut frame, u32::MAX);
    }

    /// Шкала времени не переполняет u32: кадр на границе шкалы (почти
    /// 49,7 суток) считается без паники, догоняющих тактов не больше
    /// потолка ядра, мусор в буфере не переживает.
    #[test]
    fn elapsed_at_u32_max_does_not_overflow() {
        let mut starfall = Starfall::new(mapping());
        let mut buffer = [Rgb8 {
            r: 200,
            g: 200,
            b: 200,
        }; 64];
        let mut frame = Frame::new(8, 8, &mut buffer).unwrap();

        starfall.render(&mut frame, u32::MAX);

        assert!(
            frame
                .pixels()
                .iter()
                .all(|c| !(c.r == 200 && c.g == 200 && c.b == 200))
        );
    }
}
