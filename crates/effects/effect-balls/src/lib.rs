//! Аддон «Шарики»: три цветных шара-Пикселя летают по Матрице, отбиваясь
//! от краёв, и оставляют гаснущие хвосты.
//!
//!
//! Координаты и векторы шаров — в субпикселях, десятых долях Пикселя
//! (оригинал хранит их умноженными на 10): физика остаётся целочисленной,
//! а скорость шаров — 0,4–1,4 Пикселя за такт. Каждый такт всё поле
//! угасает (`fadePixel(70)` оригинала), шары сдвигаются на свои векторы,
//! отражаются от краёв и перерисовываются поверх хвостов. Состояние —
//! постоянное поле [`Rgb8`] (аналог `leds[]` исходного алгоритма), случайность —
//! детерминированный [`Rng16`]: кадры одинаковы на хосте и устройстве.

#![no_std]

use garland_core::{
    Effect, Frame, Hsv8, Mapping, NOISE_MAX_SIDE, Rgb8, Rng16, advance_anchor, fade_pixel,
    hsv2rgb_rainbow, steps_since,
};

/// Период такта, мс: пошаговые эффекты живут с темпом ~12 тактов/с.
const TICK_MS: u32 = 80;

/// Количество шаров (`BALLS_AMOUNT` оригинала).
const BALLS_AMOUNT: usize = 3;

/// Шаг угасания следов за такт (`TRACK_STEP` оригинала).
const TRACK_STEP: u8 = 70;

/// Единица субпиксельных координат: позиции шаров хранятся в десятых долях
/// Пикселя (оригинал: `coord` умножен на 10).
const SUBPIXEL: i32 = 10;

/// Нижняя граница скорости, десятых Пикселя за такт (`random(4, 15)` оригинала).
const SPEED_MIN: u8 = 4;

/// Верхняя (исключающая) граница скорости.
const SPEED_LIMIT: u8 = 15;

/// Предел броска тона: 9 дискретных оттенков (`random(0, 9) * 28` оригинала).
const HUE_COUNT: u8 = 9;

/// Шаг тона между соседними оттенками палитры шариков.
const HUE_STEP: u8 = 28;

/// Полные насыщенность и яркость шаров.
const FULL: u8 = 255;

/// Потолок размера Матрицы: постоянное поле квадрат 64×64 Пикселя, как у
/// шумовых эффектов ядра.
const FIELD_CAP: usize = (NOISE_MAX_SIDE * NOISE_MAX_SIDE) as usize;

/// Шарики.
pub struct Balls {
    /// Ширина Матрицы, Пикселей.
    w: u16,
    /// Высота Матрицы, Пикселей.
    h: u16,
    /// Детерминированный ГПСЧ FastLED: стартовые векторы и цвета шаров.
    rng: Rng16,
    /// Постоянное поле кадра (аналог `leds[]` исходного алгоритма), строка-мажорно.
    field: [Rgb8; FIELD_CAP],
    /// Координаты шаров в субпикселях (десятых долях Пикселя): `[x, y]`.
    coord: [[i32; 2]; BALLS_AMOUNT],
    /// Векторы шаров, десятых Пикселя за такт: `[vx, vy]`.
    vector: [[i32; 2]; BALLS_AMOUNT],
    /// Цвета шаров — 9 дискретных оттенков палитры исходного алгоритма.
    colors: [Rgb8; BALLS_AMOUNT],
    /// Момент времени последнего такта, мс.
    last_ms: u32,
}

/// Случайный знак ±1: бросок монеты исходного алгоритма (`random(0, 2) ? 1 : -1`).
fn signed(rng: &mut Rng16) -> i32 {
    i32::from(rng.random8_below(2)) * 2 - 1
}

impl Balls {
    /// Шарики для Матрицы из [`Mapping`]: размеры запоминаются один раз,
    /// шары разбрасываются из центра до первого кадра.
    pub fn new(mapping: Mapping) -> Self {
        let mut balls = Self {
            w: mapping.width(),
            h: mapping.height(),
            rng: Rng16::new(),
            field: [Rgb8::BLACK; FIELD_CAP],
            coord: [[0; 2]; BALLS_AMOUNT],
            vector: [[0; 2]; BALLS_AMOUNT],
            colors: [Rgb8::BLACK; BALLS_AMOUNT],
            last_ms: 0,
        };
        // Стартовый разброс шаров: все из центра, векторы случайных
        // направления и скорости, цвет — один из 9 дискретных оттенков;
        // порядок бросков ГПСЧ — как у оригинала.
        for j in 0..BALLS_AMOUNT {
            balls.coord[j][0] = (i32::from(balls.w) / 2) * SUBPIXEL;
            let sign_x = signed(&mut balls.rng);
            balls.vector[j][0] =
                i32::from(balls.rng.random8_range(SPEED_MIN, SPEED_LIMIT)) * sign_x;
            balls.coord[j][1] = (i32::from(balls.h) / 2) * SUBPIXEL;
            let sign_y = signed(&mut balls.rng);
            balls.vector[j][1] =
                i32::from(balls.rng.random8_range(SPEED_MIN, SPEED_LIMIT)) * sign_y;
            balls.colors[j] = hsv2rgb_rainbow(Hsv8 {
                h: balls.rng.random8_below(HUE_COUNT) * HUE_STEP,
                s: FULL,
                v: FULL,
            });
        }
        balls
    }

    /// Один такт: угасание поля, физика шаров с отражениями, перерисовка
    /// голов поверх хвостов.
    fn step(&mut self) {
        for cell in self.field.iter_mut() {
            *cell = fade_pixel(*cell, TRACK_STEP);
        }
        let (w, h) = (i32::from(self.w), i32::from(self.h));
        for j in 0..BALLS_AMOUNT {
            // Движение и отражение от левого и верхнего краёв.
            for i in 0..2 {
                self.coord[j][i] += self.vector[j][i];
                if self.coord[j][i] < 0 {
                    self.coord[j][i] = 0;
                    self.vector[j][i] = -self.vector[j][i];
                }
            }
            // Отражение от правого и нижнего краёв.
            if self.coord[j][0] > (w - 1) * SUBPIXEL {
                self.coord[j][0] = (w - 1) * SUBPIXEL;
                self.vector[j][0] = -self.vector[j][0];
            }
            if self.coord[j][1] > (h - 1) * SUBPIXEL {
                self.coord[j][1] = (h - 1) * SUBPIXEL;
                self.vector[j][1] = -self.vector[j][1];
            }
            // Голова шара перекрывает хвост: запись, не смешение.
            let x = (self.coord[j][0] / SUBPIXEL) as usize;
            let y = (self.coord[j][1] / SUBPIXEL) as usize;
            self.field[y * self.w as usize + x] = self.colors[j];
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

impl Effect for Balls {
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

    /// Цвет шара 0 — литерал теста: тон 0 броска ГПСЧ (чистый красный).
    const RED: Rgb8 = Rgb8 { r: 255, g: 0, b: 0 };
    /// Цвет шара 1: тон 28 (оранжево-янтарный).
    const AMBER: Rgb8 = Rgb8 {
        r: 180,
        g: 75,
        b: 0,
    };
    /// Цвет шара 2: тон 56 (пурпурный).
    const PURPLE: Rgb8 = Rgb8 {
        r: 95,
        g: 0,
        b: 161,
    };
    /// Хвост красного шара на такт позже: 255·(255−70+1)>>8 = 185.
    const RED_FADED: Rgb8 = Rgb8 { r: 185, g: 0, b: 0 };

    /// Привязка Стенда 8×8: Эффекту нужны только размеры.
    fn mapping() -> Mapping {
        Mapping::new(8, 8, Layout::RowPlain, Corner::TopLeft)
    }

    /// Шары стартуют из центра: на первом такте все три вышли из (4, 4)
    /// в стороны своих векторов, каждый окрашен своим из 9 оттенков;
    /// остальное поле чисто.
    #[test]
    fn balls_start_at_center_in_distinct_colors() {
        let mut balls = Balls::new(mapping());
        let mut buffer = [Rgb8::BLACK; 64];
        let mut frame = Frame::new(8, 8, &mut buffer).unwrap();

        balls.render(&mut frame, TICK_MS);

        assert_eq!(frame.get(3, 2), Some(RED), "вектор шара 0 — (−9, −14)");
        assert_eq!(frame.get(5, 2), Some(PURPLE), "вектор шара 2 — (13, −11)");
        assert_eq!(frame.get(3, 4), Some(AMBER), "вектор шара 1 — (−7, 4)");
        let lit = frame.pixels().iter().filter(|c| **c != Rgb8::BLACK).count();
        assert_eq!(lit, 3, "на первом такте видны ровно три головы");
    }

    /// Хвосты: на такт позже красный шар — в (2, 1), а его прошлая клетка
    /// (3, 2) погасла на шаг угасания.
    #[test]
    fn trails_fade_behind_balls() {
        let mut balls = Balls::new(mapping());
        let mut buffer = [Rgb8::BLACK; 64];
        let mut frame = Frame::new(8, 8, &mut buffer).unwrap();

        balls.render(&mut frame, TICK_MS);
        balls.render(&mut frame, TICK_MS * 2);

        assert_eq!(frame.get(2, 1), Some(RED), "голова ушла");
        assert_eq!(frame.get(3, 2), Some(RED_FADED), "след погас на шаг");
    }

    /// Шары живут: за 50 тактов картинка изменилась — позиции голов не
    /// стоят на месте (сравнение двух кадров).
    #[test]
    fn balls_move_within_50_ticks() {
        let mut balls = Balls::new(mapping());
        let mut first = [Rgb8::BLACK; 64];
        let mut later = [Rgb8::BLACK; 64];

        balls.render(&mut Frame::new(8, 8, &mut first).unwrap(), TICK_MS);
        for tick in 2..=50 {
            balls.render(&mut Frame::new(8, 8, &mut later).unwrap(), TICK_MS * tick);
        }

        assert_ne!(first, later, "за 50 тактов картинка не изменилась");
        assert!(later.contains(&RED), "голова красного шара видна в кадре");
    }

    /// Долгий прогон: 200 тактов — рендер без паники, головы и следы в
    /// границах Матрицы (поле рисуется целиком), картинка живая.
    #[test]
    fn two_hundred_ticks_stay_inside_the_matrix() {
        let mut balls = Balls::new(mapping());
        let mut buffer = [Rgb8::BLACK; 64];
        let mut frame = Frame::new(8, 8, &mut buffer).unwrap();

        for tick in 1..=200 {
            balls.render(&mut frame, TICK_MS * tick);
        }

        let lit = frame.pixels().iter().filter(|c| **c != Rgb8::BLACK).count();
        assert!(lit >= 3, "после 200 тактов шаров не видно: {lit}");
    }

    /// Эффект — чистая функция истории тактов: два экземпляра на один
    /// момент дают бит-в-бит равные кадры.
    #[test]
    fn same_history_same_frame() {
        let mut first = Balls::new(mapping());
        let mut second = Balls::new(mapping());
        let mut left = [Rgb8::BLACK; 64];
        let mut right = [Rgb8::BLACK; 64];

        first.render(&mut Frame::new(8, 8, &mut left).unwrap(), 400);
        second.render(&mut Frame::new(8, 8, &mut right).unwrap(), 400);

        assert_eq!(left, right);
    }

    /// Кадр заполняется целиком: мусор от прошлых Кадров не переживает рендер.
    #[test]
    fn dirty_buffer_is_cleared() {
        let mut balls = Balls::new(mapping());
        let mut buffer = [Rgb8 {
            r: 200,
            g: 200,
            b: 200,
        }; 64];
        let mut frame = Frame::new(8, 8, &mut buffer).unwrap();

        balls.render(&mut frame, TICK_MS * 2);

        assert_eq!(frame.get(2, 1), Some(RED), "голова шара на месте");
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
        let mut balls = Balls::new(mapping());
        let mut buffer = [];
        let mut frame = Frame::new(0, 0, &mut buffer).unwrap();

        balls.render(&mut frame, 0);
        balls.render(&mut frame, u32::MAX);
    }

    /// Шкала времени не переполняет u32: кадр на границе шкалы (почти
    /// 49,7 суток) считается без паники, догоняющих тактов не больше
    /// потолка ядра, физика шаров не выходит за границы.
    #[test]
    fn elapsed_at_u32_max_does_not_overflow() {
        let mut balls = Balls::new(mapping());
        let mut buffer = [Rgb8::BLACK; 64];
        let mut frame = Frame::new(8, 8, &mut buffer).unwrap();

        balls.render(&mut frame, u32::MAX);

        // Забег на годы — сброс шкалы: такты не копятся, кадр остаётся
        // в начальном состоянии, паники от арифметики физики нет.
        let lit = frame.pixels().iter().filter(|c| **c != Rgb8::BLACK).count();
        assert_eq!(lit, 0, "сброс шкалы не должен двигать шарики");
    }
}
