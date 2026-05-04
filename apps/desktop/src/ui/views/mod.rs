//! v1.6.0 — Per-canvas view modules.
//!
//! Each file declares an `impl App { fn view_… }` block that
//! the iced runtime invokes via `App::view`. App still owns its
//! state in main.rs; these modules just split the rendering code.

pub mod help;
pub mod setup_about;
pub mod setup_tabs;
