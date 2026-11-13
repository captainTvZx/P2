//! P2 tasarım sistemi.
//!
//! - [`tokens`]: P2 Mavi renk, yarıçap, boşluk ve yazı token'ları (`src/p2-theme.css`'in karşılığı).
//! - [`motion`]: süre/easing token'ları ve giriş animasyonları (`src/motion.css`'in karşılığı).
//! - [`theme`]: token'ları gpui-component temasına uygular.
//! - [`palette`]: theme.ts `paletteFor()` (19 hazır tema + özel tema).
//! - [`fonts`]: gömülü Inter (Electron'daki @fontsource-variable/inter).

pub mod components;
pub mod controls;
pub mod fonts;
pub mod form;
pub mod fx;
pub mod glyphs;
pub mod icons;
mod icons_data;
pub mod motion;
pub mod palette;
pub mod rich;
pub mod text;
pub mod theme;
pub mod tokens;

pub use fx::{P2Fx, P2Transition};
pub use motion::{Entrance, P2Motion};
pub use text::P2Text;
pub use tokens::Palette;
