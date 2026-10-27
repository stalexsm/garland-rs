//! Аддон «Матрица»: зелёный «дождь кода» — капли падают сверху столбцами
//! и гаснут, оставляя затухающий след.
//!
//! Раз за такт в верхней строке с малым шансом рождается чисто зелёная
//! капля (только над пустой клеткой), кадр сдвигается на строку вниз, след
//! капли гаснет ступенями по 32 — характерные «пальцы» кода. Тики приходят
//! снаружи, ГПСЧ детерминированный (сид FastLED): кадры одинаковы на хосте
//! и устройстве и проверяются тестами.
//!
//!
//! Отклонения от оригинала: арифметика над упакованным цветом `uint32_t`
//! (`0x00FF00`, минус `0x002000`) переведена в поканальные операции над
//! [`Rgb8`]; `random()` Arduino заменён на детерминированный [`Rng16`]
//! FastLED; оси координат перевёрнуты (в исходнике y = 0 — низ Матрицы,
//! здесь y = 0 — верх), падение — рост `y`.

#![no_std]

use garland_core::{Effect, Frame, Mapping, NOISE_MAX_SIDE, Rgb8, Rng16, take_steps};

/// Период такта, мс: пошаговые эффекты шагают раз в 80 мс.
const TICK_MS: u32 = 80;

/// Плотность капель: шанс рождения в пустой клетке верхней строки за такт —
/// 1 к [`SPAWN_DENSE`] (десятая часть оригинала `random(0, 10) == 0`).
const SPAWN_DENSE: u8 = 10;

/// Цвет капли: чистый зелёный (`0x00FF00` оригинала).
const DROP: Rgb8 = Rgb8 { r: 0, g: 255, b: 0 };

/// След гаснет по 32 за такт и ниже этого канала гаснет в чёрный целиком
/// (`0x002000` оригинала: и шаг затухания, и порог полной темноты).
const FADE_FLOOR: u8 = 32;

/// Потолок поля: квадрат [`NOISE_MAX_SIDE`]×[`NOISE_MAX_SIDE`] покрывает
/// любую поддерживаемую Матрицу (4 КиБ на экземпляр Эффекта; живёт всегда
/// ровно один экземпляр Текущего эффекта).
const FIELD_CAP: usize = (NOISE_MAX_SIDE * NOISE_MAX_SIDE) as usize;

/// Матрица.
pub struct Matrix {
    /// Ширина Матрицы, Пикселей (из Привязки при создании).
    w: u16,
    /// Высота Матрицы, Пикселей (из Привязки при создании).
    h: u16,
    /// Постоянное поле (аналог `leds[]` оригинала): строка-мажор, верхняя
    /// строка — начало.
    field: [Rgb8; FIELD_CAP],
    /// Детерминированный ГПСЧ рождения капель.
    rng: Rng16,
    /// Момент последнего догнанного такта на шкале времени Прошивки.
    last_ms: u32,
}

impl Matrix {
    /// Матрица для Матрицы из Привязки `mapping`.
    pub fn new(mapping: Mapping) -> Self {
        Self {
            w: mapping.width(),
            h: mapping.height(),
            field: [Rgb8::BLACK; FIELD_CAP],
            rng: Rng16::new(),
            last_ms: 0,
        }
    }

    /// Цвет Пикселя (`x`, `y`) постоянного поля.
    fn cell(&self, x: usize, y: usize) -> Rgb8 {
        self.field[y * usize::from(self.w) + x]
    }

    /// Записать цвет в Пиксель (`x`, `y`) постоянного поля.
    fn set_cell(&mut self, x: usize, y: usize, c: Rgb8) {
        self.field[y * usize::from(self.w) + x] = c;
    }

    /// Один такт физики: обновление верхней строки (рождение капли, гашение
    /// следа) и сдвиг кадра вниз. Порядок исходного алгоритма сохранён: сначала верхняя
    /// строка, потом сдвиг; бросок ГПСЧ — только для чёрной клетки.
    fn step(&mut self) {
        let w = usize::from(self.w);
        let h = usize::from(self.h);
        for x in 0..w {
            let c = self.cell(x, 0);
            if c == Rgb8::BLACK {
                if self.rng.random8_below(SPAWN_DENSE) == 0 {
                    self.set_cell(x, 0, DROP);
                }
            } else if c.r == 0 && c.b == 0 && c.g < FADE_FLOOR {
                self.set_cell(x, 0, Rgb8::BLACK);
            } else {
                self.set_cell(
                    x,
                    0,
                    Rgb8 {
                        r: 0,
                        g: c.g - FADE_FLOOR,
                        b: 0,
                    },
                );
            }
        }
        // Сдвиг вниз: строка `y` получает содержимое строки `y-1`; обход
        // снизу вверх читает ещё не перезаписанные строки.
        for y in (1..h).rev() {
            for x in 0..w {
                let from = self.cell(x, y - 1);
                self.set_cell(x, y, from);
            }
        }
    }
}

impl Effect for Matrix {
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

    /// Мусорный цвет для проверки «Кадр заполняется целиком»: на поле живут
    /// только чёрный и оттенки зелёного.
    const JUNK: Rgb8 = Rgb8 { r: 255, g: 0, b: 0 };

    /// Матрица Стенда 8×8, змейка — для эффекта Привязка не важна.
    fn mapping8() -> Mapping {
        Mapping::new(8, 8, Layout::RowSerpentine, Corner::TopLeft)
    }

    /// Фреймбуфер 8×8 поверх буфера теста.
    fn frame8(buffer: &mut [Rgb8; 64]) -> Frame<'_> {
        Frame::new(8, 8, buffer).unwrap()
    }

    /// Капли падают: содержимое второй строки на такте `t` без изменений
    /// переезжает в третью на такте `t + 1` (обновление затрагивает только
    /// верхнюю строку, сдвиг копирует её уже затушенной — потому сравниваем
    /// со второй строки и ниже).
    #[test]
    fn drops_fall_one_row_per_tick() {
        let mut matrix = Matrix::new(mapping8());
        let mut buffer = [Rgb8::BLACK; 64];
        let mut frame = frame8(&mut buffer);

        matrix.render(&mut frame, TICK_MS);
        let mut second = [Rgb8::BLACK; 8];
        for (x, cell) in second.iter_mut().enumerate() {
            *cell = frame.get(x as u16, 1).unwrap();
        }

        matrix.render(&mut frame, 2 * TICK_MS);
        let mut third = [Rgb8::BLACK; 8];
        for (x, cell) in third.iter_mut().enumerate() {
            *cell = frame.get(x as u16, 2).unwrap();
        }

        assert_eq!(second, third);
    }

    /// Дождь идёт: за 60 тактов на поле побывал хотя бы один не чёрный
    /// Пиксель — при шансе 1/10 на клетку за такт это гарантировано.
    #[test]
    fn rain_happens() {
        let mut matrix = Matrix::new(mapping8());
        let mut buffer = [Rgb8::BLACK; 64];
        let mut frame = frame8(&mut buffer);

        let mut ever_lit = false;
        for tick in 0..=60_u32 {
            matrix.render(&mut frame, tick * TICK_MS);
            ever_lit |= frame.pixels().iter().any(|c| *c != Rgb8::BLACK);
        }

        assert!(ever_lit, "за 60 тактов не упало ни одной капли");
    }

    /// Цветовая дисциплина: на поле живут только чёрный и оттенки чистого
    /// зелёного — след капли гаснет по зелёному каналу, не трогая остальные.
    #[test]
    fn only_green_shades_on_field() {
        let mut matrix = Matrix::new(mapping8());
        let mut buffer = [Rgb8::BLACK; 64];
        let mut frame = frame8(&mut buffer);

        for tick in 0..=60_u32 {
            matrix.render(&mut frame, tick * TICK_MS);
        }

        assert!(
            frame
                .pixels()
                .iter()
                .all(|c| *c == Rgb8::BLACK || (c.r == 0 && c.b == 0)),
            "чужой цвет на поле"
        );
    }

    /// След затухает: канал капли (255) гаснет по 32 за такт и уходит в
    /// чёрный не позднее 9 тактов (255 → 223 → … → 31 → чёрный).
    #[test]
    fn trail_fades_within_nine_ticks() {
        // Капля рождается в чёрной клетке верхней строки; её след проходит
        // полные ступени 255−32·k, пока канал не упадёт ниже [`FADE_FLOOR`].
        let mut matrix = Matrix::new(mapping8());
        let mut buffer = [Rgb8::BLACK; 64];
        let mut frame = frame8(&mut buffer);

        // Ищем детерминированный момент рождения первой капли.
        let mut spawn_tick = None;
        for tick in 1..=20_u32 {
            matrix.render(&mut frame, tick * TICK_MS);
            if frame.get(0, 1) == Some(DROP) || frame.pixels().contains(&DROP) {
                spawn_tick = Some(tick);
                break;
            }
        }
        let spawn_tick = spawn_tick.expect("за 20 тактов капля не родилась");

        // Каждая строка — затухающая копия капли: зелёный канал подчинён
        // сетке 255−32·k либо чёрный; к 9 тактам после рождения капля на
        // любой строке поля погашена (след не живёт вечно).
        for tick in spawn_tick + 1..=spawn_tick + 9 {
            matrix.render(&mut frame, tick * TICK_MS);
            assert!(frame.pixels().iter().all(|c| {
                *c == Rgb8::BLACK || (c.r == 0 && c.b == 0 && (255 - c.g) % FADE_FLOOR == 0)
            }));
        }
    }

    /// Кадр заполняется целиком: мусор от прошлых Кадров не переживает
    /// рендер.
    #[test]
    fn dirty_buffer_is_cleared() {
        let mut matrix = Matrix::new(mapping8());
        let mut buffer = [JUNK; 64];
        let mut frame = frame8(&mut buffer);

        matrix.render(&mut frame, 10 * TICK_MS);

        assert!(frame.pixels().iter().all(|c| *c != JUNK));
    }

    /// Эффект — чистая функция истории тактов: два экземпляра с одним сидом
    /// на один и тот же момент рисуют бит-в-бит одинаковый Кадр.
    #[test]
    fn same_time_same_frame() {
        let mut first = Matrix::new(mapping8());
        let mut second = Matrix::new(mapping8());
        let mut left = [Rgb8::BLACK; 64];
        let mut right = [Rgb8::BLACK; 64];

        first.render(&mut Frame::new(8, 8, &mut left).unwrap(), 23 * TICK_MS);
        second.render(&mut Frame::new(8, 8, &mut right).unwrap(), 23 * TICK_MS);

        assert_eq!(left, right);
    }

    /// Шкала времени не переполняет u32: кадр на границе шкалы (почти
    /// 49,7 суток) считается без паники, мусор в буфере не переживает.
    #[test]
    fn elapsed_at_u32_max_does_not_overflow() {
        let mut matrix = Matrix::new(mapping8());
        let mut buffer = [JUNK; 64];
        let mut frame = frame8(&mut buffer);

        matrix.render(&mut frame, u32::MAX);

        assert!(frame.pixels().iter().all(|c| *c != JUNK));
    }

    /// Вырожденный пустой Кадр (0×0) не паникует: ни обновления строки,
    /// ни сдвига.
    #[test]
    fn empty_frame_is_a_noop() {
        let mut matrix = Matrix::new(mapping8());
        let mut buffer = [];
        let mut frame = Frame::new(0, 0, &mut buffer).unwrap();

        matrix.render(&mut frame, 0);
    }
}
