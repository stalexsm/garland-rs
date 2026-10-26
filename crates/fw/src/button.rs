//! Кнопка Гирлянды: антидребезг и различение короткого и длинного нажатия.
//!
//! Чистая машина без железа: внутрь — выборки уровня и время, наружу —
//! события. Кнопка замыкает Пин на землю (зажата — низкий уровень),
//! подтяжка вверх включается в Прошивке. Машина подтверждает переход
//! уровня только если сырой уровень держится вне окна антидребезга, поэтому
//! пачки дребезга не порождают событий, а шкала времени — wrapping `u32`.

/// Событие кнопки, распознанное из выборок уровня.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonEvent {
    /// Короткое нажатие: отпускание подтверждено раньше порога длинного.
    ShortPress,
    /// Длинное нажатие: срабатывает на пороге удержания, пока кнопка ещё
    /// зажата; отпускание после него события не добавляет.
    LongPress,
}

/// Окно антидребезга, мс: сырой переход подтверждается не раньше, чем через
/// это время после предыдущего подтверждённого; заведомо шире дребезга
/// механического контакта и больше двух тактов опроса Прошивки (33 мс).
const DEBOUNCE_MS: u32 = 50;

/// Порог длинного нажатия, мс: ~0,8 с, как длительность кроссфейда в спеке.
const LONG_PRESS_MS: u32 = 800;

/// Машина кнопки: копит выборки уровня, выдаёт события нажатий.
#[derive(Debug, Default)]
pub struct Button {
    /// Подтверждённое состояние: кнопка зажата.
    pressed: bool,
    /// Момент последнего подтверждённого перехода (мс, wrapping `u32`).
    changed_ms: u32,
    /// Длинное нажатие для текущего нажатия уже выдано.
    long_fired: bool,
}

impl Button {
    /// Машина в состоянии «отпущена».
    pub fn new() -> Self {
        Self::default()
    }

    /// Очередная выборка: `pressed` — кнопка зажата, `now_ms` — момент
    /// выборки. Возвращает событие, если эта выборка его замкнула.
    pub fn update(&mut self, pressed: bool, now_ms: u32) -> Option<ButtonEvent> {
        if pressed == self.pressed {
            // Уровень совпал с подтверждённым: остаётся проверить порог
            // длинного нажатия у удерживаемой кнопки.
            if pressed && !self.long_fired && now_ms.wrapping_sub(self.changed_ms) >= LONG_PRESS_MS
            {
                self.long_fired = true;
                return Some(ButtonEvent::LongPress);
            }
            return None;
        }
        // Сырой уровень разошёлся с подтверждённым: слишком быстро после
        // прошлого перехода — дребезг, подтверждения нет.
        if now_ms.wrapping_sub(self.changed_ms) < DEBOUNCE_MS {
            return None;
        }
        // Переход подтверждён; длинное — уже на пороге, короткое — на
        // отпускании, если длинное не опередило.
        self.pressed = pressed;
        self.changed_ms = now_ms;
        if pressed {
            self.long_fired = false;
            None
        } else if self.long_fired {
            None
        } else {
            Some(ButtonEvent::ShortPress)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Короткое нажатие: зажали на 100-й мс, отпустили на 200-й — ровно одно
    /// событие короткого на отпускании.
    #[test]
    fn quick_tap_gives_one_short_press() {
        let mut button = Button::new();

        assert_eq!(button.update(false, 0), None);
        assert_eq!(button.update(true, 100), None);
        assert_eq!(button.update(true, 150), None);
        assert_eq!(button.update(false, 200), Some(ButtonEvent::ShortPress));
        assert_eq!(button.update(false, 300), None);
    }

    /// Длинное нажатие: ровно на пороге (800 мс удержания) событие приходит,
    /// пока кнопка ещё зажата, ровно один раз; отпускание не добавляет ничего.
    #[test]
    fn long_press_fires_at_threshold_while_held() {
        let mut button = Button::new();

        assert_eq!(button.update(true, 1_000), None);
        assert_eq!(button.update(true, 1_799), None);
        assert_eq!(button.update(true, 1_800), Some(ButtonEvent::LongPress));
        assert_eq!(button.update(true, 3_000), None);
        assert_eq!(button.update(false, 3_100), None);
    }

    /// Удержание на миллисекунду короче порога — ещё короткое нажатие.
    #[test]
    fn just_under_threshold_is_still_short() {
        let mut button = Button::new();

        assert_eq!(button.update(true, 100), None);
        assert_eq!(
            button.update(false, 100 + LONG_PRESS_MS - 1),
            Some(ButtonEvent::ShortPress)
        );
    }

    /// Дребезг при замыкании: сырые дёргания уровня в окне антидребезга после
    /// подтверждённого отпускания игнорируются и не запускают нажатие.
    #[test]
    fn contact_bounce_at_press_is_ignored() {
        let mut button = Button::new();

        assert_eq!(button.update(false, 0), None);
        // Пачка дребезга сразу после старта: переходы в окне не считаются.
        assert_eq!(button.update(true, 20), None);
        assert_eq!(button.update(false, 40), None);
        // Настоящее нажатие распознаётся как обычно.
        assert_eq!(button.update(true, 100), None);
        assert_eq!(button.update(false, 200), Some(ButtonEvent::ShortPress));
    }

    /// Дребезг после отпускания: контакт дёрнул в окне антидребезга —
    /// новое нажатие не началось, следующее короткое приходит от своего
    /// собственного нажатия.
    #[test]
    fn bounce_after_release_does_not_restart_press() {
        let mut button = Button::new();

        assert_eq!(button.update(true, 400), None);
        assert_eq!(button.update(false, 500), Some(ButtonEvent::ShortPress));

        assert_eq!(button.update(true, 520), None);
        assert_eq!(button.update(false, 540), None);

        assert_eq!(button.update(true, 700), None);
        assert_eq!(button.update(false, 800), Some(ButtonEvent::ShortPress));
    }

    /// Дребезг внутри нажатия: короткое размыкание в окне антидребезга не
    /// рвёт нажатие, длинное доходит до порога.
    #[test]
    fn bounce_while_held_does_not_break_press() {
        let mut button = Button::new();

        assert_eq!(button.update(true, 1_000), None);
        assert_eq!(button.update(false, 1_020), None);
        assert_eq!(button.update(true, 1_040), None);
        assert_eq!(button.update(true, 1_800), Some(ButtonEvent::LongPress));
    }

    /// Короткое нажатие короче окна антидребезга не теряется: отпускание
    /// подтверждается, как только окно истечёт.
    #[test]
    fn very_quick_tap_is_confirmed_after_debounce_window() {
        let mut button = Button::new();

        assert_eq!(button.update(true, 1_000), None);
        assert_eq!(button.update(false, 1_020), None);
        assert_eq!(button.update(false, 1_051), Some(ButtonEvent::ShortPress));
    }

    /// Одно удержание — не больше одного длинного события, как бы долго
    /// кнопку ни держали.
    #[test]
    fn long_press_emits_exactly_once() {
        let mut button = Button::new();
        assert_eq!(button.update(true, 0), None);

        let mut longs = 0;
        for now in (0..=5_000).step_by(33) {
            if button.update(true, now) == Some(ButtonEvent::LongPress) {
                longs += 1;
            }
        }
        assert_eq!(longs, 1);
    }
}
