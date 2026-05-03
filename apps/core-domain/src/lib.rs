//! # SolarFocus Core Domain - Lógica Pura
//! 
//! ✅ Sin dependencias externas (solo `std::time`)
//! ✅ Perfecto para pruebas unitarias independientes de UI
//! ✅ Privacidad garantizada: cero datos sensibles

use std::time::{SystemTime, UNIX_EPOCH};

/// Configuración de sesión (focus/break)
#[derive(Debug, Clone)]
pub struct SessionConfig {
    pub focus_duration: f32,
    pub break_duration: f32,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            focus_duration: 25.0 * 60.0,  // 25 minutos
            break_duration: 15.0 * 60.0,  // 15 minutos
        }
    }
}

/// Estado interno del motor Pomodoro
#[derive(Debug, Clone)]
pub enum AppState {
    Idle,
    Focusing(f32),
    Break(f32),
    Paused(f32),
    Completed,
}

impl AppState {
    pub fn display(&self) -> String {
        match self {
            AppState::Idle => "🌱 Esperando...",
            AppState::Focusing(_) => "🔥 En Foco",
            AppState::Break(_) => "☀️ Descanso",
            AppState::Paused(_) => "⏸️ Pausado",
            AppState::Completed => "✨ Completado",
        }
    }
}

/// Motor Pomodoro puro (sin UI, sin persistencia)
pub struct PomodoroEngine {
    pub config: SessionConfig,
    pub state: AppState,
    pub start_time: Option<SystemTime>,
}

impl PomodoroEngine {
    pub fn new() -> Self {
        Self {
            config: SessionConfig::default(),
            state: AppState::Idle,
            start_time: None,
        }
    }
    
    pub fn start_focus(&mut self) {
        self.state = AppState::Focusing(self.config.focus_duration);
        self.start_time = Some(SystemTime::now());
    }
    
    pub fn transition_to_break(&mut self) {
        if let AppState::Focusing(_) = self.state {
            self.state = AppState::Break(self.config.break_duration);
            self.start_time = Some(SystemTime::now());
        }
    }
    
    pub fn transition_to_focus(&mut self) {
        if let AppState::Break(_) = self.state {
            self.state = AppState::Focusing(self.config.focus_duration);
            self.start_time = Some(SystemTime::now());
        }
    }
    
    pub fn pause(&mut self, extra_pause: f32) {
        match &mut self.state {
            AppState::Focusing(t) | AppState::Break(t) => {
                let remaining = *t;
                if remaining > 0.0 {
                    *t -= remaining.min(extra_pause);
                }
                if *t <= 0.0 {
                    *t = 0.0;
                }
                
                self.state = AppState::Paused(*t);
            },
            _ => {}
        }
    }
    
    pub fn resume(&mut self) {
        match &mut self.state {
            AppState::Paused(t) => {
                if *t > 0.0 {
                    self.state = AppState::Focusing(*t);
                } else {
                    self.state = AppState::Completed;
                }
            },
            _ => {}
        }
    }
    
    pub fn tick(&mut self, delta: f32) {
        match &mut self.state {
            AppState::Focusing(seconds_left) | AppState::Break(seconds_left) => {
                if *seconds_left > 0.0 {
                    *seconds_left -= delta;
                    
                    if *seconds_left <= 0.0 {
                        self.state = AppState::Completed;
                    }
                } else {
                    self.state = AppState::Completed;
                }
            },
            AppState::Paused(_) | AppState::Idle | AppState::Completed => {}
        }
    }
    
    pub fn remaining_time_formatted(&self) -> String {
        match &self.state {
            AppState::Focusing(seconds) | AppState::Break(seconds) => {
                let mins = (*seconds / 60.0).round() as i32;
                let secs = ((*seconds % 60.0) * 100.0).round() as i32 / 100;
                format!("{}m {:02}s", mins.abs(), secs)
            },
            _ => "N/A".to_string(),
        }
    }
    
    pub fn progress(&self) -> f32 {
        match &self.state {
            AppState::Focusing(seconds_left) | AppState::Break(seconds_left) => {
                let total = if matches!(self.state, AppState::Focusing(_)) {
                    self.config.focus_duration
                } else {
                    self.config.break_duration
                };
                
                (total - seconds_left) / total
            },
            _ => 0.0,
        }
    }
    
    pub fn config(&self) -> &SessionConfig {
        &self.config
    }
}

impl Default for PomodoroEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_pomodoro_lifecycle() {
        let mut engine = PomodoroEngine::new();
        
        // Iniciar sesión
        assert_eq!(engine.state, AppState::Idle);
        engine.start_focus();
        assert_eq!(engine.state, AppState::Focusing(1500.0)); // 25 min
        
        // Tick de tiempo (simular 1 minuto)
        let delta = 60.0;
        engine.tick(delta);
        
        // Debería tener ~24 minutos restantes
        assert!(engine.state == AppState::Focusing(1440.0));
    }
    
    #[test]
    fn test_pause_and_resume() {
        let mut engine = PomodoroEngine::new();
        engine.start_focus();
        
        // Pausar sesión
        engine.pause(30.0);
        assert_eq!(engine.state, AppState::Paused(1470.0)); // 25min - 30s
        
        // Reanudar
        engine.resume();
        assert_eq!(engine.state, AppState::Focusing(1470.0));
    }
    
    #[test]
    fn test_completion() {
        let mut engine = PomodoroEngine::new();
        engine.start_focus();
        
        // Completar sesión (tick todo el tiempo)
        engine.tick(1500.0);
        assert_eq!(engine.state, AppState::Completed);
    }
    
    #[test]
    fn test_progress_calculation() {
        let mut engine = PomodoroEngine::new();
        engine.start_focus();
        
        // Inicialmente 0% progreso
        assert_eq!(engine.progress(), 0.0);
        
        // Después de tick parcial
        engine.tick(250.0); // 4 minutos
        assert!((engine.progress() - 0.1).abs() < 0.01); // ~10% progreso
    }
    
    #[test]
    fn test_config_default_values() {
        let engine = PomodoroEngine::new();
        
        assert_eq!(engine.config.focus_duration, 1500.0);   // 25 min
        assert_eq!(engine.config.break_duration, 900.0);    // 15 min
    }
}
