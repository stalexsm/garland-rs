//! Реестр эффектов и машина Гирлянды: Текущий эффект, Покой, листание кнопкой,
//! кроссфейд и память Текущего эффекта.
//!
//! Реестр — статический список фабрик Аддонов (решение ADR-0001): без
//! рантайм-инициализации, новый Аддон — зависимость Прошивки и одна строка.
//! Машина Гирлянды чистая: события кнопки, время и хранилище приходят
//! снаружи (шов [`EffectMemory`]), поэтому сценарии «короткое/длинное
//! нажатие, Покой, восстановление питания» проверяются тестами на хосте
//! на подставном хранилище и подставном времени.

// Фабрики Реестра собирают Аддона в кадре стека (~12 КБ поля Эффекта)
// перед переездом в Box — одноразово, в момент смены Эффекта; живёт всегда
// ровно один экземпляр, embassy-задача Ленты выделяет свои кадры статически
// (bss), стек Гирлянды при работе не растёт (см. прицельное разрешение
// large_stack_frames в main.rs).
#![allow(
    clippy::large_stack_frames,
    reason = "одноразовый кадр фабрики Аддона (~12 КБ поля) при переезде в Box"
)]

extern crate alloc;

use crate::button::ButtonEvent;
use alloc::boxed::Box;
use effect_balls::Balls;
use effect_clouds::Clouds;
use effect_confetti::Confetti;
use effect_fire::Fire;
use effect_lava::Lava;
use effect_matrix::Matrix;
use effect_plasma::Plasma;
use effect_rainbow_diag::RainbowDiag;
use effect_rainbow_veil::RainbowVeil;
use effect_snowfall::Snowfall;
use effect_starfall::Starfall;
use effect_waves::Waves;
use garland_core::{Effect, Frame, MAX_ALPHA, Mapping, Rgb8, blend};

/// Длительность кроссфейда смены эффектов, мс: ~0,8 с (канон WLED, спека
/// Гирлянды); в течение кроссфейда рендерятся оба эффекта — уходящий
/// и Текущий — а вывод смешивается с 16-битной альфой.
const CROSSFADE_MS: u32 = 800;

/// Хранилище Текущего эффекта: машина пишет при переключении и читает на
/// старте. На Прошивке это NVS-раздел вне Слотов (запись блокирующая —
/// прямо при переключении, см. `memory.rs` и ADR-0003), в тестах —
/// подставка; ошибки носителя решаются внутри реализации (например,
/// журналом), машина остаётся чистой.
pub trait EffectMemory {
    /// Запомнить номер Текущего эффекта.
    fn save(&mut self, index: usize);

    /// Прочитать сохранённый номер; `None` — ничего не сохранено. Вызывается
    /// один раз при старте машины.
    fn load(&mut self) -> Option<usize>;
}

/// Фабрика Аддона: создаёт Эффект для заданной Привязки координат.
pub type Factory = fn(Mapping) -> Box<dyn Effect>;

/// Запись Реестра: имя Аддона и его фабрика.
pub type RegistryEntry = (&'static str, Factory);

/// Реестр эффектов Прошивки: имя и фабрика каждого Аддона; кнопка листает
/// Реестр по кругу.
pub static REGISTRY: &[RegistryEntry] = &[
    ("Огонь", |mapping| Box::new(Fire::new(mapping))),
    ("Снегопад", |mapping| {
        Box::new(Snowfall::new(mapping))
    }),
    ("Матрица", |mapping| Box::new(Matrix::new(mapping))),
    ("Звездопад", |mapping| {
        Box::new(Starfall::new(mapping))
    }),
    ("Конфетти", |mapping| {
        Box::new(Confetti::new(mapping))
    }),
    ("Шарики", |mapping| Box::new(Balls::new(mapping))),
    ("Синусоиды", |mapping| {
        Box::new(Waves::new(mapping))
    }),
    ("Радуга диагональная", |mapping| {
        Box::new(RainbowDiag::new(mapping))
    }),
    ("Лава", |mapping| Box::new(Lava::new(mapping))),
    ("Облака", |mapping| Box::new(Clouds::new(mapping))),
    ("Плазма", |mapping| Box::new(Plasma::new(mapping))),
    ("Радужные переливы", |mapping| {
        Box::new(RainbowVeil::new(mapping))
    }),
];

/// Уходящий Эффект: живёт, пока идёт его кроссфейд с Текущим.
struct FadeOut {
    /// Экземпляр уходящего Эффекта — продолжает свою анимацию.
    effect: Box<dyn Effect>,
    /// Момент запуска уходящего экземпляра (мс): своя шкала сохраняется.
    started_ms: u32,
    /// Момент начала кроссфейда (мс): от него считается альфа смешения.
    from_ms: u32,
}

/// Снимок вывода машины: источник кроссфейда, начавшегося посреди другого
/// кроссфейда, — застывший Кадр на момент нажатия. Шкалы времени не имеет:
/// картинка не меняется, уходит лишь альфа её смешения.
struct Frozen {
    /// Пиксели застывшего Кадра.
    pixels: Box<[Rgb8]>,
}

impl Effect for Frozen {
    fn render(&mut self, frame: &mut Frame<'_>, _elapsed_ms: u32) {
        frame.pixels_mut().copy_from_slice(&self.pixels);
    }
}

/// Машина Гирлянды: держит Текущий эффект Реестра и состояние Покоя.
pub struct Garland<'m> {
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
    /// Уходящий Эффект во время кроссфейда; вне кроссфейда — `None`.
    fading: Option<FadeOut>,
    /// Отложенное переключение: номер нового эффекта, чей кроссфейд начнётся
    /// от снимка текущего вывода на ближайшем кадре.
    pending: Option<usize>,
    /// Хранилище Текущего эффекта: пишется при переключении, читается
    /// на старте.
    memory: &'m mut dyn EffectMemory,
}

impl<'m> Garland<'m> {
    /// Гирлянда с эффектом из хранилища `memory` (без сохранённого или при
    /// номере вне Реестра — с первым), включённая с момента `start_ms` —
    /// Прошивка заводит её после приветствия-смайла, и шкала первого Эффекта
    /// начинается с нуля именно там.
    ///
    /// # Panics
    ///
    /// Паникует на пустом Реестре: листать нечего, это ошибка сборки.
    pub fn new(
        registry: &'static [RegistryEntry],
        mapping: Mapping,
        start_ms: u32,
        memory: &'m mut dyn EffectMemory,
    ) -> Self {
        assert!(!registry.is_empty(), "Реестр эффектов пуст");
        // Номер вне Реестра (сузился после обновления Прошивки) — как
        // отсутствие сохранённого: начинаем с первого.
        let current = memory
            .load()
            .filter(|index| *index < registry.len())
            .unwrap_or(0);
        Self {
            registry,
            mapping,
            current,
            asleep: false,
            effect: (registry[current].1)(mapping),
            started_ms: start_ms,
            fading: None,
            pending: None,
            memory,
        }
    }

    /// Событие кнопки: короткое листает Реестр по кругу (кроссфейдом), а в
    /// Покое возвращает Текущий эффект мгновенно; длинное переводит в Покой.
    /// Номер и хранилище меняются сразу; во время кроссфейда картинка
    /// догоняет на ближайшем кадре — от снимка текущего вывода (без рывка
    /// альфы). Имя затронутого Эффекта — [`Garland::effect_name`].
    pub fn press(&mut self, event: ButtonEvent, now_ms: u32) {
        match event {
            ButtonEvent::LongPress => {
                self.asleep = true;
                // Покой гасит Ленту мгновенно: незавершённый кроссфейд не
                // вспыхнет после пробуждения.
                self.fading = None;
            }
            ButtonEvent::ShortPress => {
                if self.asleep {
                    // Покой не трогает ни номер, ни экземпляр Эффекта —
                    // пробуждение возвращает Текущий эффект на его шкале.
                    self.asleep = false;
                } else {
                    let next = (self.current + 1) % self.registry.len();
                    self.current = next;
                    self.memory.save(next);
                    if self.fading_active(now_ms) {
                        self.pending = Some(next);
                    } else {
                        // Просроченный, ещё не снятый рендером кроссфейд — не
                        // источник снимка: смена сразу, без отложенного такта.
                        self.fading = None;
                        self.start(next, now_ms);
                    }
                }
            }
        }
    }

    /// Имя Текущего эффекта: в Покое — тот же, что будет после пробуждения.
    pub fn effect_name(&self) -> &'static str {
        self.registry[self.current].0
    }

    /// Идёт ли кроссфейд на момент `now_ms`: ещё не истёк по времени.
    /// Единый предикат для `press` и `render`: иначе в такте, где кроссфейд
    /// уже истёк, но ещё не снят рендером, `press` отложил бы переключение
    /// в `pending`, а рендер того же такта исполнил бы его мгновенно —
    /// без снимка и без плавности.
    fn fading_active(&self, now_ms: u32) -> bool {
        matches!(
            &self.fading,
            Some(fade) if now_ms.wrapping_sub(fade.from_ms) < CROSSFADE_MS
        )
    }

    /// Отрисовать Кадр на момент `now_ms`. В Покое Фреймбуфер полностью
    /// чёрный; во время кроссфейда — смесь уходящего (`outgoing`) и Текущего
    /// (`frame`) Эффектов с 16-битной альфой, вне его — вывод Текущего
    /// эффекта по его собственной шкале. Отложенное переключение исполняется
    /// на ближайшем кадре от снимка текущего вывода. Буферы обязаны
    /// описывать одну Матрицу.
    pub fn render(&mut self, frame: &mut Frame<'_>, outgoing: &mut Frame<'_>, now_ms: u32) {
        if self.asleep {
            frame.fill(Rgb8::BLACK);
            return;
        }
        debug_assert_eq!(
            frame.len(),
            outgoing.len(),
            "Фреймбуферы кроссфейда разного размера"
        );

        // Отложенное переключение: источник нового кроссфейда — снимок
        // текущего вывода, чтобы альфа не прыгала с текущей на нулевую.
        if let Some(index) = self.pending.take() {
            let fading = self.fading_active(now_ms);
            if fading {
                self.blend_frame(frame, outgoing, now_ms);
                self.fading = Some(FadeOut {
                    effect: Box::new(Frozen {
                        pixels: Box::from(frame.pixels()),
                    }),
                    started_ms: now_ms,
                    from_ms: now_ms,
                });
            }
            self.effect = (self.registry[index].1)(self.mapping);
            self.started_ms = now_ms;
            if fading {
                return; // кадр уже выведен — это и есть точка непрерывности
            }
        }

        // Просроченный кроссфейд снимается до рендера: после его конца
        // Лента — чистый вывод Текущего эффекта.
        if let Some(fade) = &self.fading
            && now_ms.wrapping_sub(fade.from_ms) >= CROSSFADE_MS
        {
            self.fading = None;
        }
        match &mut self.fading {
            Some(_) => self.blend_frame(frame, outgoing, now_ms),
            None => self
                .effect
                .render(frame, now_ms.wrapping_sub(self.started_ms)),
        }
    }

    /// Кадр кроссфейда: уходящий Эффект рендерится в `outgoing`, Текущий —
    /// в `frame`, вывод — их 16-битная смесь по прогрессу кроссфейда.
    fn blend_frame(&mut self, frame: &mut Frame<'_>, outgoing: &mut Frame<'_>, now_ms: u32) {
        let Some(fade) = &mut self.fading else {
            return;
        };
        fade.effect
            .render(outgoing, now_ms.wrapping_sub(fade.started_ms));
        self.effect
            .render(frame, now_ms.wrapping_sub(self.started_ms));
        let progress = now_ms.wrapping_sub(fade.from_ms);
        let alpha = (progress * u32::from(MAX_ALPHA) / CROSSFADE_MS) as u16;
        for (dst, src) in frame.pixels_mut().iter_mut().zip(outgoing.pixels()) {
            *dst = blend(*src, *dst, alpha);
        }
    }

    /// Смена эффекта вне идущего кроссфейда: прежний экземпляр уходит
    /// источником нового кроссфейда и продолжает свою анимацию до конца.
    fn start(&mut self, index: usize, now_ms: u32) {
        let outgoing = core::mem::replace(&mut self.effect, (self.registry[index].1)(self.mapping));
        self.fading = Some(FadeOut {
            effect: outgoing,
            started_ms: self.started_ms,
            from_ms: now_ms,
        });
        self.started_ms = now_ms;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use garland_core::{Corner, Layout, MAX_ALPHA};

    /// Подставное хранилище: копит последний сохранённый индекс.
    struct FakeMemory(Option<usize>);

    impl EffectMemory for FakeMemory {
        fn save(&mut self, index: usize) {
            self.0 = Some(index);
        }
        fn load(&mut self) -> Option<usize> {
            self.0
        }
    }

    /// Матрица 4×3, змейка по рядам.
    fn mapping() -> Mapping {
        Mapping::new(4, 3, Layout::RowSerpentine, Corner::TopLeft)
    }

    /// Рабочий и уходящий Фреймбуферы 4×3 — шов кроссфейда машины.
    fn frame_pair() -> (Frame<'static>, Frame<'static>) {
        let frame = Frame::new(4, 3, Box::leak(vec![Rgb8::BLACK; 12].into_boxed_slice())).unwrap();
        let outgoing =
            Frame::new(4, 3, Box::leak(vec![Rgb8::BLACK; 12].into_boxed_slice())).unwrap();
        (frame, outgoing)
    }

    /// Эталонный кадр Эффекта Реестра под номером `index`, прожившего
    /// `elapsed_ms` со своего запуска — минуя машину Гирлянды: фабрика
    /// Реестра и трейт Эффекта, без промежуточных слоёв.
    fn registry_frame(index: usize, mapping: &Mapping, elapsed_ms: u32) -> Box<[Rgb8]> {
        let mut effect = (REGISTRY[index].1)(*mapping);
        let mut buffer = vec![Rgb8::BLACK; 12];
        effect.render(&mut Frame::new(4, 3, &mut buffer).unwrap(), elapsed_ms);
        buffer.into_boxed_slice()
    }

    /// Канал `c` лежит между `a` и `b` включительно — свойство поканального
    /// смешения без выбросов (рывка).
    fn between(a: Rgb8, b: Rgb8, c: Rgb8) -> bool {
        let inside = |x: u8, y: u8, z: u8| (x <= z && z <= y) || (y <= z && z <= x);
        inside(a.r, b.r, c.r) && inside(a.g, b.g, c.g) && inside(a.b, b.b, c.b)
    }

    #[test]
    fn every_addon_is_reachable_by_button_and_fades_in() {
        let mapping = mapping();
        for (offset, (name, _)) in REGISTRY[1..].iter().enumerate() {
            let index = offset + 1;
            let previous = offset;
            let mut memory = FakeMemory(Some(previous));
            let mut garland = Garland::new(REGISTRY, mapping, 0, &mut memory);
            let (mut frame, mut outgoing) = frame_pair();

            garland.press(ButtonEvent::ShortPress, 200);
            let mid = 200 + CROSSFADE_MS / 2;
            garland.render(&mut frame, &mut outgoing, mid);
            let old = registry_frame(previous, &mapping, mid);
            let new = registry_frame(index, &mapping, mid - 200);
            for (i, pixel) in frame.pixels().iter().enumerate() {
                assert!(
                    between(old[i], new[i], *pixel),
                    "Аддон {name}: Пиксель {i} вне кроссфейда"
                );
            }

            let end = 200 + CROSSFADE_MS;
            garland.render(&mut frame, &mut outgoing, end);
            assert_eq!(
                frame.pixels(),
                registry_frame(index, &mapping, end - 200).as_ref(),
                "Аддон {name}: после кроссфейда Лента — чистый Эффект"
            );
        }
    }

    /// Гирлянда стартует с первого Реестра: огонь за секунду разгорается
    /// и рисует ровно эталонный кадр Аддона.
    #[test]
    fn starts_with_first_effect_lit() {
        let mut memory = FakeMemory(None);
        let mut garland = Garland::new(REGISTRY, mapping(), 0, &mut memory);
        let (mut frame, mut outgoing) = frame_pair();

        garland.render(&mut frame, &mut outgoing, 1_000);

        assert_eq!(
            frame.pixels(),
            registry_frame(0, &mapping(), 1_000).as_ref(),
            "старт — первый Аддон Реестра на своей шкале"
        );
        assert!(
            frame.pixels().iter().any(|c| *c != Rgb8::BLACK),
            "огонь за секунду не разгорелся"
        );
    }

    /// Короткое нажатие включает следующий Эффект Реестра: по завершении
    /// кроссфейда Лента — радуга (совпадает с эталоном по его шкале).
    #[test]
    fn short_press_advances_to_next_effect() {
        let mapping = mapping();
        let mut memory = FakeMemory(None);
        let mut garland = Garland::new(REGISTRY, mapping, 0, &mut memory);
        let (mut frame, mut outgoing) = frame_pair();

        garland.press(ButtonEvent::ShortPress, 200);
        let end = 200 + CROSSFADE_MS;
        garland.render(&mut frame, &mut outgoing, end);

        assert_eq!(
            frame.pixels(),
            registry_frame(1, &mapping, end - 200).as_ref(),
            "после кроссфейда Лента — новый эффект"
        );
    }

    /// Имя Текущего эффекта идёт за номером в Реестре; Покой номер не меняет.
    #[test]
    fn effect_name_follows_the_registry() {
        let mut memory = FakeMemory(None);
        let mut garland = Garland::new(REGISTRY, mapping(), 0, &mut memory);

        assert_eq!(garland.effect_name(), "Огонь");
        for expected in ["Снегопад", "Матрица", "Звездопад"] {
            garland.press(ButtonEvent::ShortPress, 100);
            assert_eq!(garland.effect_name(), expected);
        }
        garland.press(ButtonEvent::LongPress, 200);
        assert_eq!(
            garland.effect_name(),
            "Звездопад",
            "Покой не меняет Текущий эффект"
        );
    }

    /// Короткие нажатия листают Реестр по кругу: полный обход возвращает
    /// первый Аддон (отложенное переключение применяется на такте снятия
    /// кроссфейда), который разгорается по своему эталону.
    #[test]
    fn short_press_wraps_around_registry() {
        let mut memory = FakeMemory(None);
        let mut garland = Garland::new(REGISTRY, mapping(), 0, &mut memory);
        let (mut frame, mut outgoing) = frame_pair();

        for _ in 0..REGISTRY.len() {
            garland.press(ButtonEvent::ShortPress, 200);
        }
        garland.render(&mut frame, &mut outgoing, 200 + CROSSFADE_MS);
        assert_eq!(
            garland.effect_name(),
            "Огонь",
            "полный обход — снова первый"
        );

        garland.render(&mut frame, &mut outgoing, 200 + CROSSFADE_MS + 1_000);
        assert_eq!(
            frame.pixels(),
            registry_frame(0, &mapping(), 1_000).as_ref(),
            "огонь после обхода совпадает с эталоном"
        );
    }

    /// Длинное нажатие — Покой: Лента полностью чёрная в любой момент;
    /// повторное длинное ничего не меняет.
    #[test]
    fn long_press_darkens_strip() {
        let mut memory = FakeMemory(None);
        let mut garland = Garland::new(REGISTRY, mapping(), 0, &mut memory);
        let (mut frame, mut outgoing) = frame_pair();

        garland.press(ButtonEvent::LongPress, 500);
        garland.press(ButtonEvent::LongPress, 600);
        garland.render(&mut frame, &mut outgoing, 700);

        assert_eq!(frame.pixels(), &[Rgb8::BLACK; 12]);
    }

    /// Короткое нажатие в Покое возвращает Текущий эффект: после радуги
    /// Лента снова горит, и фаза радуги продолжает идти с момента её
    /// включения, а не сбрасывается пробуждением.
    #[test]
    fn short_press_in_idle_restores_current_effect() {
        let mapping = mapping();
        let mut memory = FakeMemory(None);
        let mut garland = Garland::new(REGISTRY, mapping, 0, &mut memory);
        garland.press(ButtonEvent::ShortPress, 200);
        garland.press(ButtonEvent::LongPress, 1_000);

        let (mut idle, mut outgoing) = frame_pair();
        garland.render(&mut idle, &mut outgoing, 1_100);
        assert_eq!(idle.pixels(), &[Rgb8::BLACK; 12], "в Покое Лента погашена");

        garland.press(ButtonEvent::ShortPress, 5_000);
        let (mut restored, mut outgoing) = frame_pair();
        garland.render(&mut restored, &mut outgoing, 5_000);

        // Эталон: та же радуга без Покоя, прожившая те же 4 800 мс со своего
        // включения.
        let mut reference_memory = FakeMemory(None);
        let mut reference = Garland::new(REGISTRY, mapping, 0, &mut reference_memory);
        reference.press(ButtonEvent::ShortPress, 200);
        let (mut expected, mut reference_outgoing) = frame_pair();
        reference.render(&mut expected, &mut reference_outgoing, 5_000);

        assert_ne!(
            restored.pixels(),
            &[Rgb8::BLACK; 12],
            "после пробуждения Лента горит"
        );
        assert_eq!(
            restored.pixels(),
            expected.pixels(),
            "фаза радуги продолжается"
        );
    }

    /// Смена эффектов идёт кроссфейдом: в начале Лента — уходящий эффект,
    /// в середине — смесь без выбросов, в конце — чистый Текущий эффект.
    #[test]
    fn effect_switch_fades_from_old_to_new() {
        let mapping = mapping();
        let mut memory = FakeMemory(None);
        let mut garland = Garland::new(REGISTRY, mapping, 0, &mut memory);
        garland.press(ButtonEvent::ShortPress, 200); // статичный → радуга

        let (mut frame, mut outgoing) = frame_pair();

        // Начало кроссфейда: Лента ещё показывает уходящий статичный цвет.
        garland.render(&mut frame, &mut outgoing, 200);
        assert_eq!(
            frame.pixels(),
            registry_frame(0, &mapping, 200).as_ref(),
            "в начале кроссфейда Лента — уходящий эффект"
        );

        // Середина: каждый Пиксель между уходящим и новым — рывка нет;
        // и это настоящая смесь, а не проход насквозь.
        let mid = 200 + CROSSFADE_MS / 2;
        garland.render(&mut frame, &mut outgoing, mid);
        let old = registry_frame(0, &mapping, mid);
        let new = registry_frame(1, &mapping, mid - 200);
        for (i, pixel) in frame.pixels().iter().enumerate() {
            assert!(
                between(old[i], new[i], *pixel),
                "Пиксель {i} вне кроссфейда"
            );
        }
        assert_ne!(frame.pixels(), old.as_ref(), "середина — не чистый старый");
        assert_ne!(frame.pixels(), new.as_ref(), "середина — не чистый новый");

        // Конец и после конца: Лента — чистый новый эффект на его шкале.
        let end = 200 + CROSSFADE_MS;
        garland.render(&mut frame, &mut outgoing, end);
        assert_eq!(
            frame.pixels(),
            registry_frame(1, &mapping, end - 200).as_ref()
        );
        garland.render(&mut frame, &mut outgoing, end + 33); // такт Прошивки
        assert_eq!(
            frame.pixels(),
            registry_frame(1, &mapping, end + 33 - 200).as_ref()
        );
    }

    /// Переключение эффекта сохраняет его номер в хранилище.
    #[test]
    fn switch_saves_index_to_memory() {
        let mut memory = FakeMemory(None);
        {
            let mut garland = Garland::new(REGISTRY, mapping(), 0, &mut memory);
            garland.press(ButtonEvent::ShortPress, 200);
        }
        assert_eq!(memory.0, Some(1), "переключение сохранило индекс");

        {
            let mut garland = Garland::new(REGISTRY, mapping(), 0, &mut memory);
            // Долистываем по кругу до первого: реестр замкнулся.
            for _ in 1..REGISTRY.len() {
                garland.press(ButtonEvent::ShortPress, 400);
            }
        }
        assert_eq!(memory.0, Some(0));
    }

    /// Переподача питания: новая машина с тем же хранилищем включается
    /// с сохранённого эффекта, сразу и на его собственной шкале.
    #[test]
    fn power_restore_starts_with_saved_effect() {
        let mapping = mapping();
        let mut memory = FakeMemory(None);
        {
            let mut garland = Garland::new(REGISTRY, mapping, 0, &mut memory);
            garland.press(ButtonEvent::ShortPress, 200); // переключили на радугу
        }
        assert_eq!(memory.0, Some(1), "перед «отключением» индекс сохранён");

        let mut garland = Garland::new(REGISTRY, mapping, 5_000, &mut memory);
        let (mut frame, mut outgoing) = frame_pair();
        garland.render(&mut frame, &mut outgoing, 5_000 + 33);

        assert_eq!(
            frame.pixels(),
            registry_frame(1, &mapping, 33).as_ref(),
            "после переподачи питания включён сохранённый эффект"
        );
    }

    /// Сохранённый номер вне Реестра (Реестр сузился после обновления
    /// Прошивки) — не падение: Гирлянда включается с первого эффекта.
    #[test]
    fn saved_index_out_of_registry_falls_back_to_first() {
        let mapping = mapping();
        let mut memory = FakeMemory(Some(REGISTRY.len()));
        let mut garland = Garland::new(REGISTRY, mapping, 0, &mut memory);
        let (mut frame, mut outgoing) = frame_pair();

        garland.render(&mut frame, &mut outgoing, 100);

        assert_eq!(frame.pixels(), registry_frame(0, &mapping, 100).as_ref());
    }

    /// Без сохранённого номера (первый запуск) Гирлянда включается
    /// с первого эффекта Реестра.
    #[test]
    fn no_saved_index_starts_with_first() {
        let mapping = mapping();
        let mut memory = FakeMemory(None);
        let mut garland = Garland::new(REGISTRY, mapping, 0, &mut memory);
        let (mut frame, mut outgoing) = frame_pair();

        garland.render(&mut frame, &mut outgoing, 100);

        assert_eq!(frame.pixels(), registry_frame(0, &mapping, 100).as_ref());
    }

    /// Покой и пробуждение не трогают хранилище: сохраняется только смена
    /// эффекта.
    #[test]
    fn idle_and_wake_do_not_touch_memory() {
        let mut memory = FakeMemory(None);
        let mut garland = Garland::new(REGISTRY, mapping(), 0, &mut memory);

        garland.press(ButtonEvent::LongPress, 100); // Покой
        garland.press(ButtonEvent::ShortPress, 200); // пробуждение

        assert_eq!(memory.0, None, "Покой и пробуждение не пишут хранилище");
    }

    /// Переключение во время кроссфейда без рывка: новый кроссфейд стартует
    /// от снимка текущего вывода, затем снимок плавно уходит в новый эффект.
    #[test]
    fn switch_during_fade_continues_from_current_output() {
        let mapping = mapping();
        let mut memory = FakeMemory(None);
        let mut garland = Garland::new(REGISTRY, mapping, 0, &mut memory);
        garland.press(ButtonEvent::ShortPress, 200); // огонь → снегопад

        let (mut frame, mut outgoing) = frame_pair();

        // Нажатие посреди кроссфейда (400 из 800 мс): такт нажатия — смесь
        // уходящего и Текущего без выбросов.
        garland.render(&mut frame, &mut outgoing, 630);
        garland.press(ButtonEvent::ShortPress, 640); // посреди кроссфейда
        garland.render(&mut frame, &mut outgoing, 640);
        let old = registry_frame(0, &mapping, 640);
        let new = registry_frame(1, &mapping, 640 - 200);
        for (i, pixel) in frame.pixels().iter().enumerate() {
            assert!(
                between(old[i], new[i], *pixel),
                "Пиксель {i} вне кроссфейда в такт нажатия"
            );
        }

        // Источник нового кроссфейда — снимок: смесь на момент нажатия
        // (альфа прошла половину пути: 440 из 800 мс).
        let snapshot: Vec<Rgb8> = registry_frame(0, &mapping, 640)
            .iter()
            .zip(registry_frame(1, &mapping, 640 - 200).iter())
            .map(|(old, new)| blend(*old, *new, MAX_ALPHA / 2))
            .collect();

        // Середина нового кроссфейда: пиксели между снимком и волнами
        // (третьим Аддоном Реестра).
        let mid = 640 + CROSSFADE_MS / 2;
        garland.render(&mut frame, &mut outgoing, mid);
        let target = registry_frame(2, &mapping, mid - 640);
        for (i, pixel) in frame.pixels().iter().enumerate() {
            assert!(
                between(snapshot[i], target[i], *pixel),
                "Пиксель {i} вне кроссфейда"
            );
        }

        // Конец: чистые волны на их собственной шкале.
        garland.render(&mut frame, &mut outgoing, 640 + CROSSFADE_MS);
        assert_eq!(
            frame.pixels(),
            registry_frame(2, &mapping, CROSSFADE_MS).as_ref()
        );
    }

    /// Длинное нажатие посреди кроссфейда гасит Ленту сразу, а пробуждение
    /// возвращает чистый Текущий эффект — без остатка кроссфейда.
    #[test]
    fn idle_during_fade_gives_black_then_current() {
        let mapping = mapping();
        let mut memory = FakeMemory(None);
        let mut garland = Garland::new(REGISTRY, mapping, 0, &mut memory);
        garland.press(ButtonEvent::ShortPress, 200); // статичный → радуга
        garland.press(ButtonEvent::LongPress, 500); // Покой среди кроссфейда

        let (mut frame, mut outgoing) = frame_pair();
        garland.render(&mut frame, &mut outgoing, 700);
        assert_eq!(frame.pixels(), &[Rgb8::BLACK; 12], "Покой гасит сразу");

        garland.press(ButtonEvent::ShortPress, 2_000); // пробуждение
        garland.render(&mut frame, &mut outgoing, 2_000);
        assert_eq!(
            frame.pixels(),
            registry_frame(1, &mapping, 2_000 - 200).as_ref(),
            "после пробуждения — чистый Текущий эффект"
        );
    }

    /// Нажатие в такте, где кроссфейд истёк по времени, но ещё не снят
    /// рендером: машина начинает свежий кроссфейд (на Ленте — уходящий
    /// эффект), а не исполняет отложенное мгновенное переключение.
    #[test]
    fn press_on_expired_fade_starts_fresh_fade() {
        let mapping = mapping();
        let mut memory = FakeMemory(None);
        let mut garland = Garland::new(REGISTRY, mapping, 0, &mut memory);
        garland.press(ButtonEvent::ShortPress, 200); // статичный → радуга
        // Такт ровно на границе: 200 + 800 − 200 = 800 — кроссфейд истёк,
        // рендер его ещё не снимал.
        garland.press(ButtonEvent::ShortPress, 200 + CROSSFADE_MS); // радуга → кольца

        let (mut frame, mut outgoing) = frame_pair();
        garland.render(&mut frame, &mut outgoing, 200 + CROSSFADE_MS);
        assert_eq!(
            frame.pixels(),
            registry_frame(1, &mapping, CROSSFADE_MS).as_ref(),
            "истёкший кроссфейд стартует новый: на Ленте — уходящий Эффект"
        );
    }

    /// Кроссфейд через заворот счётчика времени `u32` (49 суток работы):
    /// огонь стартует незадолго до границы, его шкала честно пересекает
    /// заворот; смешение посреди кроссфейда и его завершение не ломаются.
    #[test]
    fn crossfade_survives_u32_wrap() {
        let mapping = mapping();
        let mut memory = FakeMemory(None);
        let origin = u32::MAX - 1_199; // огонь стартует за 800 мс до заворота
        let mut garland = Garland::new(REGISTRY, mapping, origin, &mut memory);
        let (mut frame, mut outgoing) = frame_pair();
        garland.render(&mut frame, &mut outgoing, origin);

        let press = origin + 800; // 399 мс до заворота: огонь → снегопад
        garland.press(ButtonEvent::ShortPress, press);
        garland.render(&mut frame, &mut outgoing, press);

        // 100 мс после заворота: кроссфейд идёт (501 из 800) — поканальная
        // смесь уходящего и Текущего без выбросов; шкалы обеихEffects —
        // через границу u32.
        garland.render(&mut frame, &mut outgoing, 100);
        let old = registry_frame(0, &mapping, 100u32.wrapping_sub(origin)); // 1300
        let new = registry_frame(1, &mapping, 100u32.wrapping_sub(press)); // 501
        for (i, pixel) in frame.pixels().iter().enumerate() {
            assert!(
                between(old[i], new[i], *pixel),
                "Пиксель {i} вне кроссфейда после заворота счётчика"
            );
        }

        // Кроссфейд завершается: на Ленте — чистый Текущий эффект.
        garland.render(&mut frame, &mut outgoing, 401);
        assert_eq!(
            frame.pixels(),
            registry_frame(1, &mapping, 401u32.wrapping_sub(press)).as_ref(), // 801
            "кроссфейд завершается после заворота счётчика"
        );
    }
}
