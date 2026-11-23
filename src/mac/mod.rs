//! macOS: прямые вызовы objc runtime без сторонних крейтов. Основа — порт
//! rnikitin/yamusic-rs (GPL-3.0). Все функции звать из главного потока.

mod objc;
pub mod app;
pub mod media;
pub mod window;
