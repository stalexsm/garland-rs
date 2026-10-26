//! Ядро рендеринга Гирлянды: Фреймбуфер, Привязка координат Матрицы,
//! цветовая математика (RGB/HSV, гамма, альфа-смешение).
//!
//! Крейт — чистый `no_std` без зависимостей от железа: вся математика
//! проверяется тестами на хосте (`cargo test` без устройства).
#![no_std]

pub mod color;
pub mod frame;
pub mod mapping;

pub use color::{Hsv8, MAX_ALPHA, Rgb8, blend, gamma, gamma8, hsv_to_rgb};
pub use frame::Frame;
pub use mapping::{Corner, Layout, Mapping};
