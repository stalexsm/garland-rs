//! Аддон «Снегопад»: бело-голубые хлопья сыплются сверху и падают столбцами.
//!
//! Раз за такт кадр сдвигается на строку вниз, а в верхней строке с малым
//! шансом засеивается снежинка — но не над другой снежинкой, чтобы хлопьи
//! не слипались в столбы. Тики приходят снаружи, ГПСЧ детерминированный
//! (сид FastLED): кадры одинаковы на хосте и устройстве и проверяются
//! тестами.
//!
//!
//! Отклонения от оригинала: `random()` Arduino заменён на детерминированный
//! [`Rng16`] FastLED; палитра `0xE0FFFF - 0x101010·k` развёрнута в таблицу
//! констант; оси координат перевёрнуты (в исходнике y = 0 — низ Матрицы,
//! здесь y = 0 — верх), падение — рост `y`.

#![no_std]

use garland_core::{
    Effect, Frame, Mapping, NOISE_MAX_SIDE, Rgb8, Rng16, advance_anchor, steps_since,
};

/// Период такта, мс: пошаговые эффекты шагают раз в 80 мс.
const TICK_MS: u32 = 80;

/// Плотность снегопада (`SNOW_DENSE` оригинала): шанс засева клетки
/// верхней строки за такт — 1 к [`SNOW_DENSE`].
const SNOW_DENSE: u8 = 10;

/// Палитра снежинок: `0xE0FFFF - 0x101010·k`, k = 0..3 — белые с холодным
/// отливом, четыре ступени яркости (все каналы гаснут равномерно).
const SNOW: [Rgb8; 4] = [
    Rgb8 {
        r: 224,
        g: 255,
        b: 255,
    },
    Rgb8 {
        r: 208,
        g: 239,
        b: 239,
    },
    Rgb8 {
        r: 192,
        g: 223,
        b: 223,
    },
    Rgb8 {
        r: 176,
        g: 207,
        b: 207,
    },
];

/// Потолок поля: квадрат [`NOISE_MAX_SIDE`]×[`NOISE_MAX_SIDE`] покрывает
/// любую поддерживаемую Матрицу (4 КиБ на экземпляр Эффекта; живёт всегда
/// ровно один экземпляр Текущего эффекта).
const FIELD_CAP: usize = (NOISE_MAX_SIDE * NOISE_MAX_SIDE) as usize;

/// Снегопад.
pub struct Snowfall {
    /// Ширина Матрицы, Пикселей (из Привязки при создании).
    w: u16,
    /// Высота Матрицы, Пикселей (из Привязки при создании).
    h: u16,
    /// Постоянное поле (аналог `leds[]` оригинала): строка-мажор, верхняя
    /// строка — начало.
    field: [Rgb8; FIELD_CAP],
    /// Детерминированный ГПСЧ засева.
    rng: Rng16,
    /// Момент последнего догнанного такта на шкале времени Прошивки.
    last_ms: u32,
}

impl Snowfall {
    /// Снегопад для Матрицы из Привязки `mapping`.
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

    /// Один такт физики: сдвиг кадра вниз и засев верхней строки. Порядок
    /// исходного алгоритма сохранён: сначала сдвиг, потом засев; бросок ГПСЧ делается
    /// только для клетки, под которой пусто (иначе хлопьи слипались бы
    /// в столбы) — от порядка зависит детерминизм последовательности.
    fn step(&mut self) {
        let w = usize::from(self.w);
        let h = usize::from(self.h);
        // Сдвиг вниз: строка `y` получает содержимое строки `y-1`; обход
        // снизу вверх читает ещё не перезаписанные строки.
        for y in (1..h).rev() {
            for x in 0..w {
                let from = self.cell(x, y - 1);
                self.set_cell(x, y, from);
            }
        }
        // Засев верхней строки: не над снежинкой, с шансом 1/`SNOW_DENSE`.
        for x in 0..w {
            let below_is_clear = h < 2 || self.cell(x, 1) == Rgb8::BLACK;
            if below_is_clear && self.rng.random8_below(SNOW_DENSE) == 0 {
                let snow = SNOW[usize::from(self.rng.random8_below(SNOW.len() as u8))];
                self.set_cell(x, 0, snow);
            } else {
                self.set_cell(x, 0, Rgb8::BLACK);
            }
        }
    }
}

impl Effect for Snowfall {
    fn render(&mut self, frame: &mut Frame<'_>, elapsed_ms: u32) {
        if frame.is_empty() {
            return;
        }
        // Время снаружи: все пропущенные такты догоняются разом.
        let steps = steps_since(self.last_ms, elapsed_ms, TICK_MS);
        self.last_ms = advance_anchor(self.last_ms, elapsed_ms, TICK_MS);
        if steps > 0 {
            for _ in 0..steps {
                self.step();
            }
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

    /// Мусорный цвет для проверки «Кадр заполняется целиком»: в палитре
    /// снега (r = 224) чисто красного не бывает.
    const JUNK: Rgb8 = Rgb8 { r: 255, g: 0, b: 0 };

    /// Матрица Стенда 8×8, змейка — для снегопада Привязка не важна.
    fn mapping8() -> Mapping {
        Mapping::new(8, 8, Layout::RowSerpentine, Corner::TopLeft)
    }

    /// Фреймбуфер 8×8 поверх буфера теста.
    fn frame8(buffer: &mut [Rgb8; 64]) -> Frame<'_> {
        Frame::new(8, 8, buffer).unwrap()
    }

    /// Снежинки падают: содержимое верхней строки на такте `t` без изменений
    /// переезжает во вторую строку на такте `t + 1` — во вторую строку больше
    /// ничего не пишется (засев только в нулевую).
    #[test]
    fn snow_falls_one_row_per_tick() {
        let mut snowfall = Snowfall::new(mapping8());
        let mut buffer = [Rgb8::BLACK; 64];
        let mut frame = frame8(&mut buffer);

        snowfall.render(&mut frame, TICK_MS);
        let mut top = [Rgb8::BLACK; 8];
        for (x, cell) in top.iter_mut().enumerate() {
            *cell = frame.get(x as u16, 0).unwrap();
        }

        snowfall.render(&mut frame, 2 * TICK_MS);
        let mut second = [Rgb8::BLACK; 8];
        for (x, cell) in second.iter_mut().enumerate() {
            *cell = frame.get(x as u16, 1).unwrap();
        }

        assert_eq!(top, second);
    }

    /// Все зажжённые Пиксели — цвета палитры снега: чужих оттенков на поле
    /// не появляется.
    #[test]
    fn lit_pixels_are_snow_palette() {
        let mut snowfall = Snowfall::new(mapping8());
        let mut buffer = [Rgb8::BLACK; 64];
        let mut frame = frame8(&mut buffer);

        for tick in 0..=30_u32 {
            snowfall.render(&mut frame, tick * TICK_MS);
        }

        assert!(
            frame
                .pixels()
                .iter()
                .all(|c| *c == Rgb8::BLACK || SNOW.contains(c)),
            "чужой цвет на поле"
        );
    }

    /// Снег идёт: за 30 тактов на поле побывал хотя бы один не чёрный
    /// Пиксель — при шансе 1/10 на клетку за такт это гарантировано.
    #[test]
    fn snow_happens() {
        let mut snowfall = Snowfall::new(mapping8());
        let mut buffer = [Rgb8::BLACK; 64];
        let mut frame = frame8(&mut buffer);

        let mut ever_lit = false;
        for tick in 0..=30_u32 {
            snowfall.render(&mut frame, tick * TICK_MS);
            ever_lit |= frame.pixels().iter().any(|c| *c != Rgb8::BLACK);
        }

        assert!(ever_lit, "за 30 тактов не выпало ни одной снежинки");
    }

    /// Кадр заполняется целиком: мусор от прошлых Кадров не переживает
    /// рендер.
    #[test]
    fn dirty_buffer_is_cleared() {
        let mut snowfall = Snowfall::new(mapping8());
        let mut buffer = [JUNK; 64];
        let mut frame = frame8(&mut buffer);

        snowfall.render(&mut frame, 10 * TICK_MS);

        assert!(frame.pixels().iter().all(|c| *c != JUNK));
    }

    /// Эффект — чистая функция истории тактов: два экземпляра с одним сидом
    /// на один и тот же момент рисуют бит-в-бит одинаковый Кадр.
    #[test]
    fn same_time_same_frame() {
        let mut first = Snowfall::new(mapping8());
        let mut second = Snowfall::new(mapping8());
        let mut left = [Rgb8::BLACK; 64];
        let mut right = [Rgb8::BLACK; 64];

        first.render(&mut Frame::new(8, 8, &mut left).unwrap(), 17 * TICK_MS);
        second.render(&mut Frame::new(8, 8, &mut right).unwrap(), 17 * TICK_MS);

        assert_eq!(left, right);
    }

    /// Шкала времени не переполняет u32: кадр на границе шкалы (почти
    /// 49,7 суток) считается без паники, мусор в буфере не переживает.
    #[test]
    fn elapsed_at_u32_max_does_not_overflow() {
        let mut snowfall = Snowfall::new(mapping8());
        let mut buffer = [JUNK; 64];
        let mut frame = frame8(&mut buffer);

        snowfall.render(&mut frame, u32::MAX);

        assert!(frame.pixels().iter().all(|c| *c != JUNK));
    }

    /// Вырожденный пустой Кадр (0×0) не паникует: ни сдвига, ни засева.
    #[test]
    fn empty_frame_is_a_noop() {
        let mut snowfall = Snowfall::new(mapping8());
        let mut buffer = [];
        let mut frame = Frame::new(0, 0, &mut buffer).unwrap();

        snowfall.render(&mut frame, 0);
    }
}
