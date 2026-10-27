//! Аддон «Огонь»: пламя-камин на всю Матрицу с летящими угольками.
//!
//!
//! Пламя — стек из 8 поколений яркостей: раз в четыре такта (фаза `pcnt`
//! шагает по 30 и на сотне сдвигает стек) в основание входит новая
//! случайная строка 64..254, а кадр рисуется как интерполяция каждого
//! поколения с предыдущим (доля `pcnt`/100) минус маска формы факела; тон
//! берётся из второй маски (0 — красный, 25 — жёлтый): ядро пламени жёлтое,
//! края и кончики красные. Над факелом летят угольки: с шансом 1/20 за такт
//! над ярким Пикселем вспыхивает копия его цвета и дальше поднимается на
//! строку за такт.
//!
//! Отклонения от оригинала (остальное перенесено по смыслу бит-в-бит):
//! 1. Интерполяция поколений в исходнике — во float с делением на 100.0 и
//!    обрезающим cast; здесь та же формула целочисленно в u32, отличается
//!    только отбрасыванием дробной части.
//! 2. ГПСЧ Arduino `random()` заменён детерминированным [`Rng16`]: кадры
//!    одинаковы на хосте и устройстве (требование проекта).
//! 3. Отрисовка идёт в порядке оригинала — от верхних строк к нижним,
//!    основание последним: угольки обязаны читать строку предыдущего такта;
//!    обход снизу вверх размазал бы искру в столб за один такт.
//! 4. Основание берёт яркость из `line[x]` на всю ширину Матрицы: в v1.15
//!    нижняя строка тайлится каждые 16 столбцов (`newX`), что на широкой
//!    Матрице дублирует случайность.

#![no_std]

use garland_core::{
    Effect, Frame, Hsv8, Mapping, NOISE_MAX_SIDE, Rgb8, Rng16, advance_anchor, hsv2rgb_rainbow,
    steps_since,
};

/// Период такта исходных эффектов, мс: один шаг анимации раз в 80 мс.
const TICK_MS: u32 = 80;

/// Фаза интерполяции — проценты, как `pcnt` исходного алгоритма.
const PERCENT: u32 = 100;

/// Прирост фазы за такт: 30·4 ≥ 100 — новая строка пламени раз в 4 такта.
const PCNT_STEP: u8 = 30;

/// Порог фазы, после которого стек пламени сдвигается с новой строкой.
const PCNT_LIMIT: u8 = 100;

/// Границы яркости новой строки: 64..254 — как `random(64, 255)` исходного алгоритма.
const LINE_MIN: u8 = 64;
const LINE_LIMIT: u8 = 255;

/// Уголёк вспыхивает с шансом 1 из 20 за такт (`random(0, 20)` исходного алгоритма).
const SPARK_CHANCE: u8 = 20;

/// Размер масок факела из исходника: 8 поколений × 16 столбцов.
const FLAME_ROWS: usize = 8;
const MASK_COLUMNS: usize = 16;

/// Строка вспышек угольков и нижняя граница их шлейфа (строки 8..16).
const SPARK_ROW: usize = 8;
const MASK_ROWS: usize = 16;

/// Полные насыщенность и яркость: пламя — чистые цвета спектра.
const FULL: u8 = 255;

/// Потолок постоянного поля: квадрат [`NOISE_MAX_SIDE`] × [`NOISE_MAX_SIDE`].
const FIELD_CAP: usize = (NOISE_MAX_SIDE * NOISE_MAX_SIDE) as usize;

/// Маска формы факела `valueMask[8][16]`: сколько яркости вычитается из
/// поколения — кончики (верхние строки) гасятся сильнее основания.
const VALUE_MASK: [[u8; MASK_COLUMNS]; FLAME_ROWS] = [
    [32, 0, 0, 0, 0, 0, 0, 32, 32, 0, 0, 0, 0, 0, 0, 32],
    [64, 0, 0, 0, 0, 0, 0, 64, 64, 0, 0, 0, 0, 0, 0, 64],
    [96, 32, 0, 0, 0, 0, 32, 96, 96, 32, 0, 0, 0, 0, 32, 96],
    [
        128, 64, 32, 0, 0, 32, 64, 128, 128, 64, 32, 0, 0, 32, 64, 128,
    ],
    [
        160, 96, 64, 32, 32, 64, 96, 160, 160, 96, 64, 32, 32, 64, 96, 160,
    ],
    [
        192, 128, 96, 64, 64, 96, 128, 192, 192, 128, 96, 64, 64, 96, 128, 192,
    ],
    [
        255, 160, 128, 96, 96, 128, 160, 255, 255, 160, 128, 96, 96, 128, 160, 255,
    ],
    [
        255, 192, 160, 128, 128, 160, 192, 255, 255, 192, 160, 128, 128, 160, 192, 255,
    ],
];

/// Маска тона `hueMask[8][16]`: 0 — красный, 25 — жёлтый; ядро пламени
/// (центральные столбцы тайла) горячее и желтее краёв.
const HUE_MASK: [[u8; MASK_COLUMNS]; FLAME_ROWS] = [
    [1, 11, 19, 25, 25, 22, 11, 1, 1, 11, 19, 25, 25, 22, 11, 1],
    [1, 8, 13, 19, 25, 19, 8, 1, 1, 8, 13, 19, 25, 19, 8, 1],
    [1, 8, 13, 16, 19, 16, 8, 1, 1, 8, 13, 16, 19, 16, 8, 1],
    [1, 5, 11, 13, 13, 13, 5, 1, 1, 5, 11, 13, 13, 13, 5, 1],
    [1, 5, 11, 11, 11, 11, 5, 1, 1, 5, 11, 11, 11, 11, 5, 1],
    [0, 1, 5, 8, 8, 5, 1, 0, 0, 1, 5, 8, 8, 5, 1, 0],
    [0, 0, 1, 5, 5, 1, 0, 0, 0, 0, 1, 5, 5, 1, 0, 0],
    [0, 0, 0, 1, 1, 0, 0, 0, 0, 0, 0, 1, 1, 0, 0, 0],
];

/// Огонь.
pub struct Fire {
    /// Ширина Матрицы, Пикселей.
    w: u16,
    /// Высота Матрицы, Пикселей.
    h: u16,
    /// Стек поколений пламени `matrixValue[8][16]`: строка 0 — основание
    /// (самое свежее), строка 7 — кончики (самое старое).
    matrix_value: [[u8; MASK_COLUMNS]; FLAME_ROWS],
    /// Яркости новой нижней строки: ёмкость [`NOISE_MAX_SIDE`], рабочая
    /// длина — ширина Матрицы.
    line: [u8; NOISE_MAX_SIDE as usize],
    /// Фаза интерполяции в процентах между тактами (`pcnt` исходного алгоритма).
    pcnt: u8,
    /// Детерминированный ГПСЧ: новая строка и вспышки угольков.
    rng: Rng16,
    /// Постоянное поле Кадра (аналог `leds[]` исходного алгоритма): угольки читают
    /// предыдущий такт.
    field: [Rgb8; FIELD_CAP],
    /// Момент последнего такта, мс (шкала wrapping).
    last_ms: u32,
}

impl Fire {
    /// Огонь для Матрицы из Привязки `mapping`; сторона не больше
    /// [`NOISE_MAX_SIDE`] — потолок постоянного поля.
    pub fn new(mapping: Mapping) -> Self {
        let (w, h) = (mapping.width(), mapping.height());
        assert!(
            w <= NOISE_MAX_SIDE && h <= NOISE_MAX_SIDE,
            "Матрица {w}×{h} крупнее потолка {NOISE_MAX_SIDE} Пикселей у «Огня»"
        );
        let mut fire = Self {
            w,
            h,
            matrix_value: [[0; MASK_COLUMNS]; FLAME_ROWS],
            line: [0; NOISE_MAX_SIDE as usize],
            pcnt: 0,
            rng: Rng16::new(),
            field: [Rgb8::BLACK; FIELD_CAP],
            last_ms: 0,
        };
        // Как в исходнике при загрузке эффекта (`loadingFlag`): первая строка
        // сгенерирована сразу, а не после первого сдвига стека.
        fire.generate_line();
        fire
    }

    /// Один такт анимации исходного алгоритма: на сотне фазы стек сдвигается с новой
    /// строкой, затем поле перерисовывается на текущей фазе интерполяции.
    fn step(&mut self) {
        if self.pcnt >= PCNT_LIMIT {
            self.shift_up();
            self.generate_line();
            self.pcnt = 0;
        }
        self.draw_field();
        self.pcnt += PCNT_STEP;
    }

    /// Сдвиг стека поколений вверх: строка 7 теряется, в основание входит
    /// предыдущая новая строка (свежая придёт в [`Fire::generate_line`]).
    fn shift_up(&mut self) {
        for row in (1..FLAME_ROWS).rev() {
            self.matrix_value[row] = self.matrix_value[row - 1];
        }
        for c in 0..usize::from(self.w).min(MASK_COLUMNS) {
            self.matrix_value[0][c] = self.line[c];
        }
    }

    /// Новая нижняя строка яркостей: 64..254, как `random(64, 255)` исходного алгоритма.
    fn generate_line(&mut self) {
        for slot in self.line.iter_mut().take(usize::from(self.w)) {
            *slot = self.rng.random8_range(LINE_MIN, LINE_LIMIT);
        }
    }

    /// Перерисовать поле на текущей фазе: поколения пламени (строки 1..8)
    /// интерполируются с поколением ниже минус маска формы, угольки (строка
    /// 8 и выше) копируют Пиксель под собой с прошлого такта. Порядок
    /// оригинала — от верхних строк к нижним, основание последним: тогда
    /// чтения угольков видят предыдущий такт.
    fn draw_field(&mut self) {
        let (w, h) = (usize::from(self.w), usize::from(self.h));
        if w == 0 || h == 0 {
            return;
        }
        let pcnt = u32::from(self.pcnt);
        let rest = PERCENT - pcnt;
        for gy in (1..h).rev() {
            for x in 0..w {
                // Маски тайлятся каждые 16 столбцов (`newX = x % 16` исходного алгоритма).
                let c = x % MASK_COLUMNS;
                let y = h - 1 - gy;
                let color = if gy < FLAME_ROWS {
                    // Пламя: интерполяция поколений минус маска формы.
                    let interp = (rest * u32::from(self.matrix_value[gy][c])
                        + pcnt * u32::from(self.matrix_value[gy - 1][c]))
                        / PERCENT;
                    let v = interp.saturating_sub(u32::from(VALUE_MASK[gy][c])) as u8;
                    hsv2rgb_rainbow(Hsv8 {
                        h: HUE_MASK[gy][c],
                        s: FULL,
                        v,
                    })
                } else if gy == SPARK_ROW {
                    // Уголёк: вспышка со случайным шансом над ярким пламенем.
                    let below = self.field[(y + 1) * w + x];
                    if self.rng.random8_below(SPARK_CHANCE) == 0 && below != Rgb8::BLACK {
                        below
                    } else {
                        Rgb8::BLACK
                    }
                } else if gy < MASK_ROWS {
                    // Шлейф уголька: копия Пикселя под собой с прошлого такта.
                    let below = self.field[(y + 1) * w + x];
                    if below != Rgb8::BLACK {
                        below
                    } else {
                        Rgb8::BLACK
                    }
                } else {
                    // Выше зоны масок пламя не рисуется — остаётся чёрным.
                    Rgb8::BLACK
                };
                self.field[y * w + x] = color;
            }
        }
        // Основание пламени (строка 0): интерполяция с новой строкой, без
        // маски формы — самая горячая зона.
        for x in 0..w {
            let c = x % MASK_COLUMNS;
            let v = ((rest * u32::from(self.matrix_value[0][c]) + pcnt * u32::from(self.line[x]))
                / PERCENT) as u8;
            self.field[(h - 1) * w + x] = hsv2rgb_rainbow(Hsv8 {
                h: HUE_MASK[0][c],
                s: FULL,
                v,
            });
        }
    }
}

impl Effect for Fire {
    fn render(&mut self, frame: &mut Frame<'_>, elapsed_ms: u32) {
        // Такты исходного алгоритма: досчитать все пропущенные шаги, затем вывести поле.
        let steps = steps_since(self.last_ms, elapsed_ms, TICK_MS);
        self.last_ms = advance_anchor(self.last_ms, elapsed_ms, TICK_MS);
        if steps > 0 {
            for _ in 0..steps {
                self.step();
            }
        }
        // Контракт Эффекта — заполнить Кадр целиком: поле и есть картинка
        // (Фреймбуферы Прошивки переиспользуются между Кадрами).
        let (w, h) = (usize::from(frame.width()), usize::from(frame.height()));
        for y in 0..h {
            for x in 0..w {
                frame.set(x as u16, y as u16, self.field[y * w + x]);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use garland_core::{Corner, Layout};

    /// Привязка Матрицы 8×8 со змейкой — размер стандартного Стенда.
    fn mapping8() -> Mapping {
        Mapping::new(8, 8, Layout::RowSerpentine, Corner::TopLeft)
    }

    /// Цикл фазы виден по основанию: четыре такта основание тянет долю
    /// `pcnt` от первой строки (10·0 + 90·line)/100, на пятом такте стек
    /// сдвинулся — и основание получило строку целиком. Картинка такта
    /// зависит именно от сгенерированной `line` (ГПСЧ с сидом 1337).
    #[test]
    fn base_row_lights_from_generated_line_on_fifth_tick() {
        // Ожидаемая первая строка: та же последовательность ГПСЧ, что и в
        // конструкторе эффекта.
        let mut rng = Rng16::new();
        let line: [u8; 8] = core::array::from_fn(|_| rng.random8_range(LINE_MIN, LINE_LIMIT));

        let mut fire = Fire::new(mapping8());
        let mut buffer = [Rgb8::BLACK; 64];
        let mut frame = Frame::new(8, 8, &mut buffer).unwrap();

        fire.render(&mut frame, 4 * TICK_MS);
        for x in 0..8usize {
            let v = (90 * u32::from(line[x]) / PERCENT) as u8;
            let expected = hsv2rgb_rainbow(Hsv8 {
                h: HUE_MASK[0][x],
                s: FULL,
                v,
            });
            assert_eq!(frame.get(x as u16, 7), Some(expected), "x = {x}");
            for y in 0..7u16 {
                assert_eq!(
                    frame.get(x as u16, y),
                    Some(Rgb8::BLACK),
                    "x = {x}, y = {y}"
                );
            }
        }

        fire.render(&mut frame, 5 * TICK_MS);
        for x in 0..8usize {
            let expected = hsv2rgb_rainbow(Hsv8 {
                h: HUE_MASK[0][x],
                s: FULL,
                v: line[x],
            });
            assert_eq!(frame.get(x as u16, 7), Some(expected), "x = {x}");
            for y in 0..7u16 {
                assert_eq!(
                    frame.get(x as u16, y),
                    Some(Rgb8::BLACK),
                    "x = {x}, y = {y}"
                );
            }
        }
    }

    /// После прогрева (30 тактов) пламя горит внизу: нижняя треть Матрицы
    /// суммарно ярче верхней трети, и цвет тёплый — есть Пиксели, у которых
    /// красный канал больше синего (у пламени синий всегда нулевой).
    #[test]
    fn after_warmup_bottom_blaze_beats_top_and_stays_warm() {
        let mut fire = Fire::new(mapping8());
        let mut buffer = [Rgb8::BLACK; 64];
        let mut frame = Frame::new(8, 8, &mut buffer).unwrap();
        for t in 1..=30 {
            fire.render(&mut frame, t * TICK_MS);
        }

        let brightness = |y: usize| {
            (0..8usize)
                .map(|x| frame.get(x as u16, y as u16).unwrap())
                .map(|c| u32::from(c.r.max(c.g).max(c.b)))
                .sum::<u32>()
        };
        let top: u32 = (0..8).take(8 / 3).map(brightness).sum();
        let bottom: u32 = (0..8).skip(8 - 8 / 3).map(brightness).sum();

        assert!(bottom > top, "низ {bottom} не ярче верха {top}");
        assert!(frame.pixels().iter().any(|c| c.r > c.b));
    }

    /// Кадр заполняется целиком: мусор от прошлых Кадров не переживает
    /// рендер (у огня синий канал всегда нулевой — «девятки» не остаются).
    #[test]
    fn dirty_buffer_is_cleared() {
        let mut fire = Fire::new(mapping8());
        let mut buffer = [Rgb8 { r: 9, g: 9, b: 9 }; 64];
        let mut frame = Frame::new(8, 8, &mut buffer).unwrap();
        for t in 1..=10 {
            fire.render(&mut frame, t * TICK_MS);
        }

        assert!(frame.pixels().iter().all(|c| c.b == 0));
    }

    /// Эффект — чистая функция истории тактов: два экземпляра с одинаковой
    /// историей рисуют бит-в-бит одинаковые Кадры.
    #[test]
    fn same_history_same_frame() {
        let mut first = Fire::new(mapping8());
        let mut second = Fire::new(mapping8());
        let mut left = [Rgb8::BLACK; 64];
        let mut right = [Rgb8::BLACK; 64];

        for elapsed in [400, 1234] {
            first.render(&mut Frame::new(8, 8, &mut left).unwrap(), elapsed);
            second.render(&mut Frame::new(8, 8, &mut right).unwrap(), elapsed);
        }

        assert_eq!(left, right);
    }

    /// Матрица 16×16 — зона угольков над факелом активна (вспышки 1/20 и
    /// шлейф): тридцать тактов без паники, картинка появляется, цвет остаётся
    /// тёплым.
    #[test]
    fn renders_on_16x16_with_sparks() {
        let mut fire = Fire::new(Mapping::new(16, 16, Layout::RowSerpentine, Corner::TopLeft));
        let mut buffer = [Rgb8::BLACK; 256];
        let mut frame = Frame::new(16, 16, &mut buffer).unwrap();
        for t in 1..=30 {
            fire.render(&mut frame, t * TICK_MS);
        }

        assert!(frame.pixels().iter().any(|c| *c != Rgb8::BLACK));
        assert!(frame.pixels().iter().all(|c| c.b == 0));
    }

    /// Вырожденный пустой Кадр (0×0) не паникует — и такты на вырожденной
    /// Матрице тоже (сдвиг стека и перерисовка без единого Пикселя).
    #[test]
    fn empty_frame_is_a_noop() {
        let mut fire = Fire::new(Mapping::new(0, 0, Layout::RowSerpentine, Corner::TopLeft));
        let mut buffer = [];
        let mut frame = Frame::new(0, 0, &mut buffer).unwrap();

        fire.render(&mut frame, 0);
        fire.render(&mut frame, 5 * TICK_MS);
    }

    /// Шкала времени не переполняет u32: кадр на границе шкалы (почти
    /// 49,7 суток) считается без паники, мусор в буфере не переживает.
    #[test]
    fn elapsed_at_u32_max_does_not_overflow() {
        let mut fire = Fire::new(mapping8());
        let mut buffer = [Rgb8 { r: 9, g: 9, b: 9 }; 64];
        let mut frame = Frame::new(8, 8, &mut buffer).unwrap();

        fire.render(&mut frame, u32::MAX);

        assert!(frame.pixels().iter().all(|c| c.b == 0));
    }
}
