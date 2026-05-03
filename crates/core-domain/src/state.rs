//! # Estado del Pomodoro
//! 
//! Estructuras de datos puros para representar el estado del sistema de productividad.
//! Sin dependencias externas - solo tipos Rust estándar.

use serde::{Deserialize, Serialize};

/// Enum que representa los estados posibles de una sesión de Pomodoro.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AppState {
    /// Estado inicial: esperando inicio de sesión
    Idle,
    
    /// En modo enfoque activo (Focus)
    Focusing(f32), // Tiempo restante en segundos
    
    /// En modo descanso activo (Break)
    Break,
    
    /// Sesión terminada - espera reset o nueva configuración
    Completed,
}

/// Estructura principal de sesión de Pomodoro
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PomodoroSession {
    /// Duración total de la sesión (segundos)
    pub duration: f32,
    
    /// Tiempo restante en la sesión (segundos)
    pub remaining: f32,
    
    /// Estado actual de la sesión
    pub state: AppState,
    
    /// Inicio de la sesión (para cálculos precisos de delta)
    #[serde(skip)]
    pub start_time: Option<std::time::Instant>,
    
    /// Pauses acumuladas en esta sesión (segundos)
    pub total_paused: f32,
}

impl PomodoroSession {
    /// Crea una nueva sesión en estado Idle con duración especificada
    pub fn new(duration_seconds: f32) -> Self {
        Self {
            duration: duration_seconds,
            remaining: duration_seconds,
            state: AppState::Idle,
            start_time: None,
            total_paused: 0.0,
        }
    }
    
    /// Inicia una sesión de enfoque
    pub fn start(&mut self) {
        self.state = AppState::Focusing(self.duration);
        self.start_time = Some(std::time::Instant::now());
        self.remaining = self.duration;
    }
    
    /// Aplica un delta de tiempo (ej: desde frame anterior)
    pub fn tick(&mut self, delta: f32) {
        if let AppState::Focusing(_) = self.state {
            if self.remaining > 0.0 {
                self.remaining -= delta;
                if self.remaining <= 0.0 {
                    self.remaining = 0.0;
                    self.state = AppState::Completed;
                } else {
                    self.state = AppState::Focusing(self.remaining);
                }
            } else {
                self.state = AppState::Completed;
            }
        }
    }
    
    /// Finaliza la sesión y la pone a Idle
    pub fn finish(&mut self) {
        match self.state {
            AppState::Completed | AppState::Focusing(_) => {
                let elapsed = self.duration - self.remaining;
                self.total_paused += elapsed;
                self.state = AppState::Idle;
                self.remaining = 0.0;
                self.start_time = None;
            },
            _ => {} // Ya está en Idle o Completed
        }
    }
    
    /// Calcula el progreso como porcentaje (0.0 a 1.0)
    pub fn progress(&self) -> f32 {
        if self.state == AppState::Idle {
            return 0.0;
        }
        
        let elapsed = self.duration - self.remaining;
        (elapsed / self.duration).min(1.0)
    }
    
    /// Formatea el tiempo restante como "mm:ss"
    pub fn format_time(&self) -> String {
        let total_secs = if self.state == AppState::Idle {
            0
        } else {
            let remaining_int = (self.remaining).round() as i32;
            remaining_int.abs()
        };
        
        let mins = total_secs / 60;
        let secs = total_secs % 60;
        format!("{:02}:{:02}", mins, secs)
    }
    
    /// Determina si la sesión está activa
    pub fn is_active(&self) -> bool {
        matches!(self.state, AppState::Focusing(_) | AppState::Break)
    }
}

impl Default for PomodoroSession {
    fn default() -> Self {
        Self::new(1500.0) // 25 minutos por defecto
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_session_lifecycle() {
        let mut session = PomodoroSession::new(30.0); // 30 segundos para pruebas
        
        assert_eq!(session.state, AppState::Idle);
        assert!(!session.is_active());
        
        session.start();
        assert_eq!(session.state, AppState::Focusing(30.0));
        assert!(session.is_active());
        
        session.tick(5.0);
        assert!((session.remaining - 25.0).abs() < 0.01);

        session.tick(25.0);
        assert_eq!(session.state, AppState::Completed);
        assert!(session.remaining.abs() < 0.01);
    }

    #[test]
    fn test_progress_calculation() {
        let mut session = PomodoroSession::new(60.0); // 60 segundos

        assert_eq!(session.progress(), 0.0); // Idle

        session.start();
        assert!((session.progress() - 0.0).abs() < 0.01); // Al inicio es 0% completado

        session.tick(15.0);
        assert!((session.progress() - 0.25).abs() < 0.01); // 25% completado

        session.tick(45.0);
        assert!((session.progress() - 1.0).abs() < 0.01); // 100% completado
    }
}
