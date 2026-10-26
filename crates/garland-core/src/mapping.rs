//! Привязка координат: перевод (x, y) Матрицы в индекс Пикселя на Ленте.
//!
//! Чистая функция без состояния: Эффекты Привязки не видят. Канон — пример
//! FastLED XYMatrix: змейка по рядам или столбцам, настраиваемый стартовый угол.

/// Раскладка: порядок, в котором Лента физически проходит по Матрице.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layout {
    /// Змейка по горизонтальным рядам (дефолт Стенда): нечётные ряды развёрнуты.
    RowSerpentine,
    /// Ряды без разворота: каждый ряд проходится в одном направлении.
    RowPlain,
    /// Змейка по вертикальным столбцам: нечётные столбцы развёрнуты.
    ColSerpentine,
    /// Столбцы без разворота.
    ColPlain,
}

/// Стартовый угол: угол Матрицы, в котором находится Пиксель с индексом 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Corner {
    /// Левый верхний угол.
    TopLeft,
    /// Правый верхний угол.
    TopRight,
    /// Левый нижний угол.
    BottomLeft,
    /// Правый нижний угол.
    BottomRight,
}

/// Ошибка разбора строкового значения `Layout`/`Corner` из переменных окружения.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParseError;

/// Привязка координат Матрицы: размеры, Раскладка и стартовый угол.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mapping {
    w: u16,
    h: u16,
    layout: Layout,
    corner: Corner,
}

impl Mapping {
    /// Привязка координат для Матрицы `w`×`h` с заданной Раскладкой и углом.
    pub fn new(w: u16, h: u16, layout: Layout, corner: Corner) -> Self {
        Self {
            w,
            h,
            layout,
            corner,
        }
    }

    /// Число Пикселей на Ленте (размер Матрицы).
    pub fn pixel_count(&self) -> usize {
        self.w as usize * self.h as usize
    }

    /// Индекс Пикселя на Ленте для координат (x, y); `None` вне Матрицы.
    pub fn strip_index(&self, x: u16, y: u16) -> Option<usize> {
        if x >= self.w || y >= self.h {
            return None;
        }
        // Расстояния (u, v) точки от стартового угла; дальше индекс считается
        // в этих координатах, поэтому все четыре угла дает одна формула.
        let (u, v) = self.distances(x, y);
        let index = match self.layout {
            Layout::RowSerpentine | Layout::RowPlain => {
                let along = self.unroll(self.layout == Layout::RowSerpentine, v, u, self.w);
                v as usize * self.w as usize + along as usize
            }
            Layout::ColSerpentine | Layout::ColPlain => {
                let along = self.unroll(self.layout == Layout::ColSerpentine, u, v, self.h);
                u as usize * self.h as usize + along as usize
            }
        };
        Some(index)
    }

    /// Обратная Привязка: координаты Пикселя, стоящего на Ленте под номером
    /// `index`; паникует при `index` вне `0..pixel_count()`.
    pub fn coord(&self, index: usize) -> (u16, u16) {
        let count = self.pixel_count();
        assert!(
            index < count,
            "индекс {index} вне Ленты из {count} Пикселей"
        );
        match self.layout {
            Layout::RowSerpentine | Layout::RowPlain => {
                let v = index / self.w as usize;
                let along = index % self.w as usize;
                let u = self.roll(self.layout == Layout::RowSerpentine, v, along, self.w);
                self.coords(u, v as u16)
            }
            Layout::ColSerpentine | Layout::ColPlain => {
                let u = index / self.h as usize;
                let along = index % self.h as usize;
                let v = self.roll(self.layout == Layout::ColSerpentine, u, along, self.h);
                self.coords(u as u16, v)
            }
        }
    }

    /// Позиция вдоль Ленты в ряду/столбце `step` для расстояния `dist`:
    /// нечётные шаги змейки проходятся навстречу (длина отрезка — `span`).
    fn unroll(&self, serpentine: bool, step: u16, dist: u16, span: u16) -> u16 {
        if serpentine && step % 2 == 1 {
            span - 1 - dist
        } else {
            dist
        }
    }

    /// Обратная операция к [`Mapping::unroll`]: расстояние от позиции вдоль Ленты.
    fn roll(&self, serpentine: bool, step: usize, along: usize, span: u16) -> u16 {
        if serpentine && step % 2 == 1 {
            span - 1 - along as u16
        } else {
            along as u16
        }
    }

    /// Расстояния (u, v) точки от стартового угла: u — по горизонтали,
    /// v — по вертикали.
    fn distances(&self, x: u16, y: u16) -> (u16, u16) {
        match self.corner {
            Corner::TopLeft => (x, y),
            Corner::TopRight => (self.w - 1 - x, y),
            Corner::BottomLeft => (x, self.h - 1 - y),
            Corner::BottomRight => (self.w - 1 - x, self.h - 1 - y),
        }
    }

    /// Координаты точки по её расстояниям (u, v) от стартового угла.
    fn coords(&self, u: u16, v: u16) -> (u16, u16) {
        match self.corner {
            Corner::TopLeft => (u, v),
            Corner::TopRight => (self.w - 1 - u, v),
            Corner::BottomLeft => (u, self.h - 1 - v),
            Corner::BottomRight => (self.w - 1 - u, self.h - 1 - v),
        }
    }
}

impl core::str::FromStr for Layout {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "serpentine-rows" => Ok(Self::RowSerpentine),
            "plain-rows" => Ok(Self::RowPlain),
            "serpentine-cols" => Ok(Self::ColSerpentine),
            "plain-cols" => Ok(Self::ColPlain),
            _ => Err(ParseError),
        }
    }
}

impl core::str::FromStr for Corner {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "top-left" => Ok(Self::TopLeft),
            "top-right" => Ok(Self::TopRight),
            "bottom-left" => Ok(Self::BottomLeft),
            "bottom-right" => Ok(Self::BottomRight),
            _ => Err(ParseError),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::str::FromStr as _;

    /// Матрица 8×8, змейка по рядам, старт в левом верхнем углу — Стенд по умолчанию.
    fn stand() -> Mapping {
        Mapping::new(8, 8, Layout::RowSerpentine, Corner::TopLeft)
    }

    /// Полный обход 4×3 змейкой от каждого из четырёх углов: Лента рисована
    /// от руки, ожидаемые координаты — источник истины, независимый от кода.
    #[test]
    fn serpentine_walk_from_all_four_corners() {
        let cases: [(Corner, [(u16, u16); 12]); 4] = [
            // Старт слева сверху: вправо по верхней строке, разворот, влево...
            (
                Corner::TopLeft,
                [
                    (0, 0),
                    (1, 0),
                    (2, 0),
                    (3, 0), //
                    (3, 1),
                    (2, 1),
                    (1, 1),
                    (0, 1), //
                    (0, 2),
                    (1, 2),
                    (2, 2),
                    (3, 2),
                ],
            ),
            // Старт справа сверху: влево по верхней строке...
            (
                Corner::TopRight,
                [
                    (3, 0),
                    (2, 0),
                    (1, 0),
                    (0, 0), //
                    (0, 1),
                    (1, 1),
                    (2, 1),
                    (3, 1), //
                    (3, 2),
                    (2, 2),
                    (1, 2),
                    (0, 2),
                ],
            ),
            // Старт слева снизу: вправо по нижней строке, вверх...
            (
                Corner::BottomLeft,
                [
                    (0, 2),
                    (1, 2),
                    (2, 2),
                    (3, 2), //
                    (3, 1),
                    (2, 1),
                    (1, 1),
                    (0, 1), //
                    (0, 0),
                    (1, 0),
                    (2, 0),
                    (3, 0),
                ],
            ),
            // Старт справа снизу: влево по нижней строке...
            (
                Corner::BottomRight,
                [
                    (3, 2),
                    (2, 2),
                    (1, 2),
                    (0, 2), //
                    (0, 1),
                    (1, 1),
                    (2, 1),
                    (3, 1), //
                    (3, 0),
                    (2, 0),
                    (1, 0),
                    (0, 0),
                ],
            ),
        ];

        for (corner, walk) in cases {
            let mapping = Mapping::new(4, 3, Layout::RowSerpentine, corner);
            for (index, expected) in walk.iter().enumerate() {
                assert_eq!(
                    mapping.coord(index),
                    *expected,
                    "угол {corner:?}: индекс {index} ленты 4×3"
                );
                assert_eq!(
                    mapping.strip_index(expected.0, expected.1),
                    Some(index),
                    "угол {corner:?}: обратная привязка {expected:?}"
                );
            }
        }
    }

    /// Разворот змейки на Стенде 8×8: нечётные ряды проходятся навстречу.
    #[test]
    fn serpentine_reverses_odd_rows_on_stand() {
        let mapping = stand();
        // Чётный ряд: слева направо; нечётный: справо налево.
        assert_eq!(mapping.strip_index(0, 0), Some(0));
        assert_eq!(mapping.strip_index(7, 0), Some(7));
        assert_eq!(mapping.strip_index(7, 1), Some(8));
        assert_eq!(mapping.strip_index(0, 1), Some(15));
        assert_eq!(mapping.strip_index(0, 2), Some(16));
        assert_eq!(mapping.strip_index(7, 2), Some(23));
    }

    /// Крайние Пиксели Стенда 8×8: углы Матрицы — начало и конец Ленты.
    #[test]
    fn stand_edge_pixels() {
        let mapping = stand();
        assert_eq!(mapping.strip_index(0, 0), Some(0)); // старт
        assert_eq!(mapping.strip_index(7, 0), Some(7)); // конец первого ряда
        assert_eq!(mapping.strip_index(7, 7), Some(56)); // последний Пиксель
        assert_eq!(mapping.strip_index(0, 7), Some(63)); // конец Ленты
        assert_eq!(mapping.pixel_count(), 64);
    }

    /// Вырожденные Матрицы: одна строка (4×1) и один столбец (1×4) — Лента
    /// последовательна от стартового угла, разворот змейки ничего не ломает.
    #[test]
    fn degenerate_single_row_and_column() {
        for corner in [
            Corner::TopLeft,
            Corner::TopRight,
            Corner::BottomLeft,
            Corner::BottomRight,
        ] {
            // 4×1: один ряд, индексы 0..3 по горизонтали от угла.
            let row = Mapping::new(4, 1, Layout::RowSerpentine, corner);
            let xs: [u16; 4] = match corner {
                Corner::TopLeft | Corner::BottomLeft => [0, 1, 2, 3],
                Corner::TopRight | Corner::BottomRight => [3, 2, 1, 0],
            };
            for (index, x) in xs.iter().enumerate() {
                assert_eq!(row.strip_index(*x, 0), Some(index), "угол {corner:?}");
            }

            // 1×4: один столбец, индексы 0..3 по вертикали от угла.
            let col = Mapping::new(1, 4, Layout::RowSerpentine, corner);
            let ys: [u16; 4] = match corner {
                Corner::TopLeft | Corner::TopRight => [0, 1, 2, 3],
                Corner::BottomLeft | Corner::BottomRight => [3, 2, 1, 0],
            };
            for (index, y) in ys.iter().enumerate() {
                assert_eq!(col.strip_index(0, *y), Some(index), "угол {corner:?}");
            }
        }
    }

    /// Раскладки без разворота и змейка по столбцам.
    #[test]
    fn plain_and_column_layouts() {
        // Ряды без разворота, 2×2.
        let plain = Mapping::new(2, 2, Layout::RowPlain, Corner::TopLeft);
        assert_eq!(plain.strip_index(0, 0), Some(0));
        assert_eq!(plain.strip_index(1, 0), Some(1));
        assert_eq!(plain.strip_index(0, 1), Some(2));
        assert_eq!(plain.strip_index(1, 1), Some(3));

        // Ряды без разворота, старт справа: ряды зеркалятся.
        let mirrored = Mapping::new(2, 2, Layout::RowPlain, Corner::TopRight);
        assert_eq!(mirrored.strip_index(1, 0), Some(0));
        assert_eq!(mirrored.strip_index(0, 0), Some(1));
        assert_eq!(mirrored.strip_index(1, 1), Some(2));
        assert_eq!(mirrored.strip_index(0, 1), Some(3));

        // Змейка по столбцам, 3×4: нечётные столбцы проходятся снизу вверх.
        let cols = Mapping::new(3, 4, Layout::ColSerpentine, Corner::TopLeft);
        assert_eq!(cols.strip_index(0, 0), Some(0));
        assert_eq!(cols.strip_index(0, 3), Some(3));
        assert_eq!(cols.strip_index(1, 3), Some(4));
        assert_eq!(cols.strip_index(1, 0), Some(7));
        assert_eq!(cols.strip_index(2, 0), Some(8));
        assert_eq!(cols.strip_index(2, 3), Some(11));

        // Столбцы без разворота, 2×3: индекс = x*h + y.
        let col_plain = Mapping::new(2, 3, Layout::ColPlain, Corner::TopLeft);
        assert_eq!(col_plain.strip_index(0, 2), Some(2));
        assert_eq!(col_plain.strip_index(1, 0), Some(3));
    }

    /// Змейка не рвёт Ленту: соседние Пиксели по ходу Раскладки — соседние
    /// индексы (нет «прыжков» через Матрицу).
    #[test]
    fn adjacent_cells_have_adjacent_indexes() {
        for layout in [Layout::RowSerpentine, Layout::RowPlain] {
            let mapping = Mapping::new(8, 8, layout, Corner::TopLeft);
            for y in 0..8 {
                for x in 0..7u16 {
                    let x1 = x + 1;
                    let a = mapping.strip_index(x, y).unwrap();
                    let b = mapping.strip_index(x1, y).unwrap();
                    assert_eq!(b.abs_diff(a), 1, "{layout:?}: ({x}, {y}) → ({x1}, {y})");
                }
            }
        }
        for layout in [Layout::ColSerpentine, Layout::ColPlain] {
            let mapping = Mapping::new(8, 8, layout, Corner::BottomRight);
            for y in 0..7u16 {
                for x in 0..8u16 {
                    let y1 = y + 1;
                    let a = mapping.strip_index(x, y).unwrap();
                    let b = mapping.strip_index(x, y1).unwrap();
                    assert_eq!(b.abs_diff(a), 1, "{layout:?}: ({x}, {y}) → ({x}, {y1})");
                }
            }
        }
    }

    /// Взаимно однозначное соответствие координат и индексов Ленты.
    #[test]
    fn mapping_roundtrips() {
        for layout in [
            Layout::RowSerpentine,
            Layout::RowPlain,
            Layout::ColSerpentine,
            Layout::ColPlain,
        ] {
            for corner in [
                Corner::TopLeft,
                Corner::TopRight,
                Corner::BottomLeft,
                Corner::BottomRight,
            ] {
                let mapping = Mapping::new(5, 3, layout, corner);
                assert_eq!(mapping.pixel_count(), 15);
                for index in 0..15u16 {
                    let (x, y) = mapping.coord(index as usize);
                    assert_eq!(
                        mapping.strip_index(x, y),
                        Some(index as usize),
                        "{layout:?}/{corner:?}: индекс {index}"
                    );
                }
            }
        }
    }

    /// Координаты вне Матрицы — `None`, а не зацикленный индекс.
    #[test]
    fn out_of_bounds_is_none() {
        let mapping = stand();
        assert_eq!(mapping.strip_index(8, 0), None);
        assert_eq!(mapping.strip_index(0, 8), None);
        assert_eq!(mapping.strip_index(8, 8), None);
    }

    /// Строковые значения переменных окружения разбираются в Раскладку и угол.
    #[test]
    fn env_strings_parse() {
        assert_eq!(
            Layout::from_str("serpentine-rows"),
            Ok(Layout::RowSerpentine)
        );
        assert_eq!(Layout::from_str("plain-rows"), Ok(Layout::RowPlain));
        assert_eq!(
            Layout::from_str("serpentine-cols"),
            Ok(Layout::ColSerpentine)
        );
        assert_eq!(Layout::from_str("plain-cols"), Ok(Layout::ColPlain));
        assert_eq!(Layout::from_str("змейка"), Err(ParseError));

        assert_eq!(Corner::from_str("top-left"), Ok(Corner::TopLeft));
        assert_eq!(Corner::from_str("top-right"), Ok(Corner::TopRight));
        assert_eq!(Corner::from_str("bottom-left"), Ok(Corner::BottomLeft));
        assert_eq!(Corner::from_str("bottom-right"), Ok(Corner::BottomRight));
        assert_eq!(Corner::from_str("center"), Err(ParseError));
    }
}
