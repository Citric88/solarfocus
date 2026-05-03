//! # SolarFocus Core Domain
//! 
//! Motor de Pomodoro puro - Arquitectura Hexagonal (Lógica sin dependencias externas)
//! 
//! ## 🎯 Principios de Diseño:
//! - **Cero dependencias externas**: Solo librerías estándar de Rust (`std`)
//! - **Lógica pura**: Separada completamente de UI e infraestructura
//! - **Determinista**: Resultados predecibles y reproducibles
//! - **Sincrónica API**: Fácil integración con UI sin bloqueos complejos
//! 
//! ## 📦 Módulos:
//! - `pomodoro/`: Motor principal de cuenta regresiva precisa
//! - `focus_rules/`: Reglas heurísticas de productividad
//! - `rewards/`: Sistema de XP y recompensas
//! - `state/`: Máquina de estados del Pomodoro

pub mod pomodoro;
pub mod focus_rules;
pub mod rewards;
pub mod state;

// Re-exportar tipos principales para uso cómodo
pub use pomodoro::PomodoroEngine;
pub use state::{AppState, PomodoroSession};
