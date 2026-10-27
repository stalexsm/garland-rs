//! Ядро рендеринга Гирлянды: трейт Эффекта, Фреймбуфер, Привязка координат
//! Матрицы, цветовая математика (RGB/HSV, гамма, альфа-смешение).
//!
//! Крейт — чистый `no_std` без зависимостей от железа: вся математика
//! проверяется тестами на хосте (`cargo test` без устройства).
#![no_std]

pub mod color;
pub mod effect;
pub mod frame;
pub mod fx;
pub mod mapping;

pub use color::{Hsv8, MAX_ALPHA, Rgb8, blend, gamma, gamma8};
pub use effect::Effect;
pub use frame::Frame;
pub use fx::{
    CLOUD_PALETTE, LAVA_PALETTE, NOISE_MAX_SIDE, NoiseField, PARTY_PALETTE, Palette16,
    RAINBOW_PALETTE, Rng16, advance_anchor, dim8_raw, ease8_in_out_quad, fade_pixel, fade_to_black,
    hsv2rgb_rainbow, inoise8, qadd8, qsub8, scale8, scale8_video, sin8, steps_since,
};
pub use mapping::{Corner, Layout, Mapping, ParseError};
