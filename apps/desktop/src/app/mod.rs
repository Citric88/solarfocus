//! v1.6.0 — App-layer non-UI helpers.
//!
//! Holds the state struct, lifecycle methods, message-handler
//! submodules, and small pure helpers extracted from main.rs. Imports
//! are kept tight — this module never touches `iced::Element`; UI
//! lives under `ui::*`.

pub mod helpers;
