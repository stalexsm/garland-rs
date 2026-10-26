//! Реестр эффектов и машина Гирлянды: Текущий эффект, Покой, листание кнопкой.
//!
//! Реестр — статический список фабрик Аддонов (решение ADR-0001): без
//! рантайм-инициализации, новый Аддон — зависимость Прошивки и одна строка.
//! Машина Гирлянды чистая: события кнопки и время приходят снаружи, поэтому
//! сценарии «короткое/длинное нажатие, Покой» проверяются тестами на хосте.

extern crate alloc;

use crate::button::ButtonEvent;
use alloc::boxed::Box;
use effect_rainbow::Rainbow;
use effect_solid::Solid;
use garland_core::{Effect, Frame, Mapping, Rgb8};

/// Фабрика Аддона: создаёт Эффект для заданной Привязки координат.
pub type Factory = fn(Mapping) -> Box<dyn Effect>;

/// Запись Реестра: имя Аддона и его фабрика.
pub type RegistryEntry = (&'static str, Factory);

/// Реестр эффектов Прошивки: имя и фабрика каждого Аддона; кнопка листает
/// Реестр по кругу.
pub static REGISTRY: &[RegistryEntry] = &[
    ("Статичный цвет", |_| Box::new(Solid)),
    ("Радуга по проводу", |mapping| {
        Box::new(Rainbow::new(mapping))
    }),
];

/// Машина Гирлянды: держит Текущий эффект Реестра и состояние Покоя.
pub struct Garland {
    /// Реестр, который машина листает.
    registry: &'static [RegistryEntry],
    /// Привязка координат для фабрик Аддонов.
    mapping: Mapping,
    /// Номер Текущего эффекта в Реестре.
    current: usize,
    /// Покой: эффекты не рендерятся, Лента полностью чёрная.
    asleep: bool,
    /// Экземпляр Текущего эффекта; в Покое сохраняется, чтобы короткое
    /// нажатие вернуло тот же Эффект.
    effect: Box<dyn Effect>,
    /// Момент запуска экземпляра (мс): у каждого Эффекта своя шкала,
    /// начинающаяся с нуля.
    started_ms: u32,
}

impl Garland {
    /// Гирлянда с первым Эффектом Реестра, включённая с момента `start_ms` —
    /// Прошивка заводит её после тестовой сетки, и шкала первого Эффекта
    /// начинается с нуля именно там.
    ///
    /// # Panics
    ///
    /// Паникует на пустом Реестре: листать нечего, это ошибка сборки.
    pub fn new(registry: &'static [RegistryEntry], mapping: Mapping, start_ms: u32) -> Self {
        assert!(!registry.is_empty(), "Реестр эффектов пуст");
        Self {
            registry,
            mapping,
            current: 0,
            asleep: false,
            effect: (registry[0].1)(mapping),
            started_ms: start_ms,
        }
    }

    /// Событие кнопки: короткое листает Реестр по кругу, а в Покое возвращает
    /// Текущий эффект; длинное переводит в Покой. Имя затронутого Эффекта
    /// для журнала — [`Garland::effect_name`].
    pub fn press(&mut self, event: ButtonEvent, now_ms: u32) {
        match event {
            ButtonEvent::LongPress => {
                self.asleep = true;
            }
            ButtonEvent::ShortPress => {
                if self.asleep {
                    // Покой не трогает ни номер, ни экземпляр Эффекта —
                    // пробуждение возвращает Текущий эффект на его шкале.
                    self.asleep = false;
                } else {
                    let next = (self.current + 1) % self.registry.len();
                    self.spawn(next, now_ms);
                }
            }
        }
    }

    /// Имя Текущего эффекта: в Покое — тот же, что будет после пробуждения.
    pub fn effect_name(&self) -> &'static str {
        self.registry[self.current].0
    }

    /// Отрисовать Кадр на момент `now_ms`: в Покое Фреймбуфер полностью
    /// чёрный, иначе — вывод Текущего эффекта по его собственной шкале.
    pub fn render(&mut self, frame: &mut Frame<'_>, now_ms: u32) {
        if self.asleep {
            frame.fill(Rgb8::BLACK);
            return;
        }
        let elapsed_ms = now_ms.wrapping_sub(self.started_ms);
        self.effect.render(frame, elapsed_ms);
    }

    /// Запустить Эффект под номером `index` заново со шкалой от `now_ms`.
    fn spawn(&mut self, index: usize, now_ms: u32) {
        self.current = index;
        self.effect = (self.registry[index].1)(self.mapping);
        self.started_ms = now_ms;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use garland_core::{Corner, Layout};

    // Цвет первого Аддона — литерал теста, независимый от кода Аддона:
    // случайная смена цвета ловится здесь.
    const WARM_WHITE: Rgb8 = Rgb8 {
        r: 255,
        g: 180,
        b: 120,
    };

    /// Матрица 4×3, змейка по рядам.
    fn mapping() -> Mapping {
        Mapping::new(4, 3, Layout::RowSerpentine, Corner::TopLeft)
    }

    /// Цвет Пикселя Ленты `index` на текущем кадре.
    fn wire_pixel(frame: &Frame<'_>, mapping: &Mapping, index: usize) -> Rgb8 {
        let (x, y) = mapping.coord(index);
        frame.get(x, y).expect("Пиксель внутри Матрицы")
    }

    /// Гирлянда стартует с первого Реестра: статичный цвет горит всей Лентой.
    #[test]
    fn starts_with_first_effect_lit() {
        let mut garland = Garland::new(REGISTRY, mapping(), 0);
        let mut buffer = [Rgb8::BLACK; 12];
        let mut frame = Frame::new(4, 3, &mut buffer).unwrap();

        garland.render(&mut frame, 100);

        assert_eq!(frame.pixels(), &[WARM_WHITE; 12]);
    }

    /// Короткое нажатие включает следующий Эффект Реестра: после него Лента —
    /// не заливка, а радуга (Пиксель 0 Ленты — красный, Пиксели разные).
    #[test]
    fn short_press_advances_to_next_effect() {
        let mapping = mapping();
        let mut garland = Garland::new(REGISTRY, mapping, 0);
        let mut buffer = [Rgb8::BLACK; 12];
        let mut frame = Frame::new(4, 3, &mut buffer).unwrap();

        garland.press(ButtonEvent::ShortPress, 200);
        garland.render(&mut frame, 200);

        assert_eq!(wire_pixel(&frame, &mapping, 0), Rgb8 { r: 255, g: 0, b: 0 });
        let first = frame.pixels()[0];
        assert!(
            frame.pixels().iter().any(|c| *c != first),
            "радуга не заливка"
        );
    }

    /// Имя Текущего эффекта идёт за номером в Реестре; Покой номер не меняет.
    #[test]
    fn effect_name_follows_the_registry() {
        let mut garland = Garland::new(REGISTRY, mapping(), 0);

        assert_eq!(garland.effect_name(), "Статичный цвет");
        garland.press(ButtonEvent::ShortPress, 100);
        assert_eq!(garland.effect_name(), "Радуга по проводу");
        garland.press(ButtonEvent::LongPress, 200);
        assert_eq!(
            garland.effect_name(),
            "Радуга по проводу",
            "Покой не меняет Текущий эффект"
        );
    }

    /// Короткие нажатия листают Реестр по кругу: два нажатия на Реестре из
    /// двух Эффектов возвращают статичный цвет.
    #[test]
    fn short_press_wraps_around_registry() {
        let mut garland = Garland::new(REGISTRY, mapping(), 0);
        let mut buffer = [Rgb8::BLACK; 12];
        let mut frame = Frame::new(4, 3, &mut buffer).unwrap();

        garland.press(ButtonEvent::ShortPress, 200);
        garland.press(ButtonEvent::ShortPress, 300);
        garland.render(&mut frame, 300);

        assert_eq!(frame.pixels(), &[WARM_WHITE; 12]);
    }

    /// Длинное нажатие — Покой: Лента полностью чёрная в любой момент;
    /// повторное длинное ничего не меняет.
    #[test]
    fn long_press_darkens_strip() {
        let mut garland = Garland::new(REGISTRY, mapping(), 0);
        let mut buffer = [Rgb8::BLACK; 12];
        let mut frame = Frame::new(4, 3, &mut buffer).unwrap();

        garland.press(ButtonEvent::LongPress, 500);
        garland.press(ButtonEvent::LongPress, 600);
        garland.render(&mut frame, 700);

        assert_eq!(frame.pixels(), &[Rgb8::BLACK; 12]);
    }

    /// Короткое нажатие в Покое возвращает Текущий эффект: после радуги
    /// Лента снова горит, и фаза радуги продолжает идти с момента её
    /// включения, а не сбрасывается пробуждением.
    #[test]
    fn short_press_in_idle_restores_current_effect() {
        let mapping = mapping();
        let mut garland = Garland::new(REGISTRY, mapping, 0);
        garland.press(ButtonEvent::ShortPress, 200);
        garland.press(ButtonEvent::LongPress, 1_000);

        let mut idle = [Rgb8::BLACK; 12];
        garland.render(&mut Frame::new(4, 3, &mut idle).unwrap(), 1_100);
        assert_eq!(idle, [Rgb8::BLACK; 12], "в Покое Лента погашена");

        garland.press(ButtonEvent::ShortPress, 5_000);
        let mut restored = [Rgb8::BLACK; 12];
        garland.render(&mut Frame::new(4, 3, &mut restored).unwrap(), 5_000);

        // Эталон: та же радуга без Покоя, прожившая те же 4 800 мс со своего
        // включения.
        let mut reference = Garland::new(REGISTRY, mapping, 0);
        reference.press(ButtonEvent::ShortPress, 200);
        let mut expected = [Rgb8::BLACK; 12];
        reference.render(&mut Frame::new(4, 3, &mut expected).unwrap(), 5_000);

        assert_ne!(restored, [Rgb8::BLACK; 12], "после пробуждения Лента горит");
        assert_eq!(restored, expected, "фаза радуги продолжается");
    }
}
