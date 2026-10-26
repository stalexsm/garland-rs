//! Фреймбуфер: массив цветов всех Пикселей Матрицы на текущий Кадр.
//!
//! Логический вид «ширина × высота × Пиксели», строка-мажор, БЕЗ змейки:
//! Привязка координат к Ленте — отдельный слой ([`crate::mapping`]), Эффекты
//! её не видят. Заимствует срез Пикселей, не владеет памятью — буферы
//! прошивки статические.

use crate::color::Rgb8;
use crate::mapping::Mapping;

/// Фреймбуфер: логическая Матрица `w`×`h` поверх среза Пикселей.
#[derive(Debug)]
pub struct Frame<'a> {
    w: u16,
    h: u16,
    pixels: &'a mut [Rgb8],
}

impl<'a> Frame<'a> {
    /// Фреймбуфер поверх среза `pixels`; длина среза обязана равняться `w`·`h`,
    /// иначе `None`.
    pub fn new(w: u16, h: u16, pixels: &'a mut [Rgb8]) -> Option<Self> {
        if pixels.len() != w as usize * h as usize {
            return None;
        }
        Some(Self { w, h, pixels })
    }

    /// Ширина Матрицы, Пикселей.
    pub const fn width(&self) -> u16 {
        self.w
    }

    /// Высота Матрицы, Пикселей.
    pub const fn height(&self) -> u16 {
        self.h
    }

    /// Число Пикселей Фреймбуфера (w·h).
    pub fn len(&self) -> usize {
        self.pixels.len()
    }

    /// Пуст ли Фреймбуфер (нулевые размеры).
    pub fn is_empty(&self) -> bool {
        self.pixels.is_empty()
    }

    /// Номер Пикселя (x, y) в строка-мажорном срезе; `None` вне Матрицы.
    fn index_of(&self, x: u16, y: u16) -> Option<usize> {
        if x >= self.w || y >= self.h {
            return None;
        }
        Some(x as usize + self.w as usize * y as usize)
    }

    /// Цвет Пикселя (x, y); `None` вне Матрицы.
    pub fn get(&self, x: u16, y: u16) -> Option<Rgb8> {
        let index = self.index_of(x, y)?;
        Some(self.pixels[index])
    }

    /// Записать цвет в Пиксель (x, y); запись вне Матрицы игнорируется.
    pub fn set(&mut self, x: u16, y: u16, c: Rgb8) {
        if let Some(index) = self.index_of(x, y) {
            self.pixels[index] = c;
        }
    }

    /// Залить весь Фреймбуфер одним цветом.
    pub fn fill(&mut self, c: Rgb8) {
        self.pixels.fill(c);
    }

    /// Записать цвет в Пиксель, стоящий на Ленте под номером `index`, — обход
    /// «по проводу»: Пиксели нумеруются в физическом порядке Привязки
    /// `mapping` (змейка, стартовый угол), а не строка-мажорно. Паникует при
    /// `index` вне Ленты. 1D-Эффекты пишут Кадр только через этот метод:
    /// Привязку им передаёт Прошивка, арифметику по координатам они
    /// не выполняют сами.
    pub fn set_wire(&mut self, mapping: &Mapping, index: usize, c: Rgb8) {
        let (x, y) = mapping.coord(index);
        self.set(x, y, c);
    }

    /// Пиксели Фреймбуфера в логическом порядке (строка-мажор, без змейки).
    pub fn pixels(&self) -> &[Rgb8] {
        self.pixels
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapping::{Corner, Layout};

    const W: u16 = 4;
    const H: u16 = 3;

    #[test]
    fn slice_length_must_match_matrix_size() {
        let mut buffer = [Rgb8::BLACK; 12];
        assert!(Frame::new(W, H, &mut buffer).is_some());

        let mut short = [Rgb8::BLACK; 11];
        assert!(Frame::new(W, H, &mut short).is_none());

        let mut long = [Rgb8::BLACK; 13];
        assert!(Frame::new(W, H, &mut long).is_none());
    }

    #[test]
    fn dimensions_and_len() {
        let mut buffer = [Rgb8::BLACK; 12];
        let frame = Frame::new(W, H, &mut buffer).unwrap();
        assert_eq!(frame.width(), W);
        assert_eq!(frame.height(), H);
        assert_eq!(frame.len(), 12);
        assert!(!frame.is_empty());

        let mut empty = [];
        let frame = Frame::new(0, 0, &mut empty).unwrap();
        assert!(frame.is_empty());
    }

    /// set/get проходят через логические координаты; хранение строка-мажор.
    #[test]
    fn set_get_roundtrip_is_row_major() {
        let mut buffer = [Rgb8::BLACK; 12];
        let mut frame = Frame::new(W, H, &mut buffer).unwrap();

        let magenta = Rgb8 {
            r: 255,
            g: 0,
            b: 255,
        };
        frame.set(2, 1, magenta);
        assert_eq!(frame.get(2, 1), Some(magenta));
        // Строка-мажор: Пиксель (2, 1) лежит в срезе под номером 2 + w·1.
        assert_eq!(frame.pixels()[2 + W as usize], magenta);
        // Остальные Пиксели не тронуты.
        assert_eq!(frame.get(1, 1), Some(Rgb8::BLACK));
    }

    #[test]
    fn get_out_of_bounds_is_none() {
        let mut buffer = [Rgb8::BLACK; 12];
        let frame = Frame::new(W, H, &mut buffer).unwrap();
        assert_eq!(frame.get(W, 0), None);
        assert_eq!(frame.get(0, H), None);
        assert_eq!(frame.get(u16::MAX, u16::MAX), None);
    }

    /// Запись вне Матрицы игнорируется: не паникует и не портит соседей.
    #[test]
    fn set_out_of_bounds_is_ignored() {
        let mut buffer = [Rgb8::BLACK; 12];
        let mut frame = Frame::new(W, H, &mut buffer).unwrap();

        let red = Rgb8 { r: 255, g: 0, b: 0 };
        frame.set(W, 0, red);
        frame.set(0, H, red);
        assert_eq!(frame.pixels(), &[Rgb8::BLACK; 12]);
    }

    /// Вид «по проводу»: `set_wire` кладёт цвет в Пиксель по индексу Ленты,
    /// а не строка-мажно: на змейке 4×3 Пиксель Ленты 5 стоит в (2, 1),
    /// тогда как строка-мажорный индекс 5 — это (1, 1).
    #[test]
    fn set_wire_follows_strip_order() {
        let mapping = Mapping::new(W, H, Layout::RowSerpentine, Corner::TopLeft);
        let mut buffer = [Rgb8::BLACK; 12];
        let mut frame = Frame::new(W, H, &mut buffer).unwrap();

        let green = Rgb8 { r: 0, g: 255, b: 0 };
        frame.set_wire(&mapping, 5, green);
        assert_eq!(frame.get(1, 1), Some(Rgb8::BLACK));
        assert_eq!(frame.get(2, 1), Some(green));
    }

    /// Индекс вне Ленты в обходе «по проводу» паникует — как [`Mapping::coord`].
    #[test]
    #[should_panic(expected = "вне Ленты")]
    fn set_wire_out_of_strip_panics() {
        let mapping = Mapping::new(W, H, Layout::RowSerpentine, Corner::TopLeft);
        let mut buffer = [Rgb8::BLACK; 12];
        let mut frame = Frame::new(W, H, &mut buffer).unwrap();
        frame.set_wire(&mapping, 12, Rgb8::BLACK);
    }

    #[test]
    fn fill_covers_every_pixel() {
        let mut buffer = [Rgb8::BLACK; 12];
        let mut frame = Frame::new(W, H, &mut buffer).unwrap();

        let green = Rgb8 { r: 0, g: 255, b: 0 };
        frame.set(0, 0, Rgb8 { r: 9, g: 9, b: 9 });
        frame.fill(green);
        assert_eq!(frame.pixels(), &[green; 12]);
    }
}
