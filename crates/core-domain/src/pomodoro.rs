//! # Motor Pomodoro Principal
//! 
//! Implementación de cuenta regresiva precisa y robusta.
//! Uso de `std::time::Instant` para precisión sin depender del loop de UI.

use crate::state::{AppState, PomodoroSession};

/// Estrategia de pausa automática entre sesiones (en segundos)
pub const DEFAULT_BREAK_DURATION: f32 = 15.0; // 15 min descanso corto
pub const LONG_BREAK_DURATION: f32 = 30.0;     // 30 min descanso largo

/// Configuración del motor Pomodoro
#[derive(Debug, Clone)]
pub struct PomodoroConfig {
    /// Duración de sesión Focus (segundos)
    pub focus_duration: f32,
    
    /// Duración de descanso corto (segundos) - después de 1 sesión
    pub short_break_duration: f32,
    
    /// Duración de descanso largo (segundos) - después de 4 sesiones
    pub long_break_duration: f32,
    
    /// Número de sesiones antes de descanso largo (pomodoros)
    pub sessions_before_long_break: u8,
}

impl Default for PomodoroConfig {
    fn default() -> Self {
        Self {
            focus_duration: 25.0,      // 25 min foco
            short_break_duration: 15.0, // 15 min descanso corto
            long_break_duration: 30.0,  // 30 min descanso largo
            sessions_before_long_break: 4,
        }
    }
}

/// Motor principal del Pomodoro
pub struct PomodoroEngine {
    /// Configuración actual
    config: PomodoroConfig,
    
    /// Sesión actual
    session: PomodoroSession,
    
    /// Contador de sesiones completadas en esta racha
    sessions_completed: u8,
    
    /// Pauses manuales acumulados (segundos)
    manual_paused_time: f32,
    
    /// Estado del motor
    engine_state: EngineState,
}

/// Estados internos del motor
#[derive(Debug, Clone, PartialEq)]
pub enum EngineState {
    /// Esperando inicio de sesión
    Idle,
    
    /// En modo Focus activo
    Focusing,
    
    /// En modo Break (descanso) activo
    Break,
}

impl PomodoroEngine {
    /// Crea un nuevo motor con configuración predeterminada
    pub fn new() -> Self {
        Self {
            config: PomodoroConfig::default(),
            session: PomodoroSession::new(25.0), // 25 min por defecto
            sessions_completed: 0,
            manual_paused_time: 0.0,
            engine_state: EngineState::Idle,
        }
    }
    
    /// Crea un nuevo motor con configuración personalizada
    pub fn with_config(config: PomodoroConfig) -> Self {
        let duration = config.focus_duration;
        Self {
            config,
            session: PomodoroSession::new(duration),
            sessions_completed: 0,
            manual_paused_time: 0.0,
            engine_state: EngineState::Idle,
        }
    }
    
    /// Comienza una nueva sesión de Focus
    pub fn start_focus(&mut self) {
        if self.engine_state == EngineState::Idle {
            self.session.start();
            self.engine_state = EngineState::Focusing;
        }
    }
    
    /// Aplica un delta de tiempo (ej: desde frame anterior de UI)
    pub fn tick(&mut self, delta: f32) {
        match self.engine_state {
            EngineState::Focusing => {
                // Solo decrementa si no hay pausas acumuladas
                if self.manual_paused_time <= 0.0 {
                    let remaining = &mut self.session.remaining;
                    if *remaining > 0.0 {
                        *remaining -= delta;
                        
                        // Si termina, transicionar a Break automáticamente
                        if *remaining <= 0.0 {
                            self.session.state = AppState::Completed;
                            self.transition_to_break();
                        }
                    }
                }
            },
            EngineState::Break => {
                // El descanso también cuenta hacia abajo (opcional)
                let remaining = &mut self.session.remaining;
                if *remaining > 0.0 {
                    *remaining -= delta;
                    
                    if *remaining <= 0.0 {
                        self.session.state = AppState::Completed;
                        self.transition_to_focus();
                    }
                }
            },
            EngineState::Idle => {} // No hacer nada en Idle
        }
    }
    
    /// Transiciona a modo Break (después de completar Focus)
    pub fn transition_to_break(&mut self) {
        self.session.remaining = self.config.short_break_duration;
        self.session.state = AppState::Break;
        self.engine_state = EngineState::Break;
        
        // Incrementar contador de sesiones completadas
        self.sessions_completed += 1;
        
        // Verificar si es descanso largo (cada 4 sesiones)
        if self.sessions_completed % self.config.sessions_before_long_break == 0 {
            self.session.remaining = self.config.long_break_duration;
        }
    }
    
    /// Transiciona de vuelta a Focus (después de Break)
    pub fn transition_to_focus(&mut self) {
        self.session.start();
        self.engine_state = EngineState::Focusing;
        
        // Resetear contador para la siguiente racha
        if self.sessions_completed % self.config.sessions_before_long_break == 0 {
            self.sessions_completed = 0;
        } else {
            self.sessions_completed -= 1;
        }
    }
    
    /// Aplica una pausa manual (pausa temporal)
    pub fn pause(&mut self, duration_seconds: f32) {
        if matches!(self.engine_state, EngineState::Focusing) {
            self.manual_paused_time += duration_seconds;
            
            // Si la pausa es completa, transicionar a Idle
            if duration_seconds >= self.session.remaining {
                self.session.state = AppState::Idle;
                self.manual_paused_time = 0.0;
                self.engine_state = EngineState::Idle;
            } else {
                // Restar el tiempo pausado
                let remaining = &mut self.session.remaining;
                *remaining += duration_seconds; // Aumentamos porque se "pausa" el countdown
                
                // Verificar si la pausa completa la sesión
                if *remaining >= self.config.focus_duration {
                    self.transition_to_break();
                }
            }
        }
    }
    
    /// Resume después de una pausa
    pub fn resume(&mut self) {
        if matches!(self.engine_state, EngineState::Focusing) {
            let remaining = &mut self.session.remaining;
            *remaining -= self.manual_paused_time.min(*remaining);
            
            // Si se completó con la pausa
            if *remaining <= 0.0 {
                self.transition_to_break();
            } else {
                self.manual_paused_time = 0.0;
            }
        }
    }
    
    /// Verifica si la sesión ha terminado
    pub fn is_session_complete(&self) -> bool {
        matches!(self.session.state, AppState::Completed)
    }
    
    /// Obtiene el estado actual del motor
    pub fn state(&self) -> &AppState {
        &self.session.state
    }
    
    /// Obtiene el tiempo restante formateado
    pub fn remaining_time_formatted(&self) -> String {
        self.session.format_time()
    }
    
    /// Obtiene el progreso como porcentaje (0.0 a 1.0)
    pub fn progress(&self) -> f32 {
        if matches!(self.engine_state, EngineState::Idle) {
            return 0.0;
        }
        
        let total_duration = match self.engine_state {
            EngineState::Focusing => self.config.focus_duration,
            EngineState::Break => self.session.remaining + self.manual_paused_time, // Time elapsed in break
            EngineState::Idle => 0.0,
        };
        
        if total_duration == 0.0 {
            return 0.0;
        }
        
        let elapsed = total_duration - self.session.remaining;
        (elapsed / total_duration).min(1.0)
    }
    
    /// Obtiene la configuración actual
    pub fn config(&self) -> &PomodoroConfig {
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
    fn test_focus_session_duration() {
        let mut engine = PomodoroEngine::new();
        
        // Start focus session (25 seconds for test)
        engine.config.focus_duration = 30.0;
        engine.start_focus();
        
        assert_eq!(engine.state(), &AppState::Focusing);
        assert!((engine.session.remaining - 30.0).abs() < 0.01);
        
        // Tick for 25 seconds
        engine.tick(25.0);
        
        // Should have ~5 seconds remaining
        assert!((engine.session.remaining - 5.0).abs() < 0.01);
        
        // Complete the session
        engine.tick(6.0);
        
        assert_eq!(engine.state(), &AppState::Completed);
    }
    
    #[test]
    fn test_auto_break_transition() {
        let mut engine = PomodoroEngine::new();
        engine.config.focus_duration = 10.0; // 10 seconds for test
        
        engine.start_focus();
        assert_eq!(engine.state(), &AppState::Focusing);
        
        // Complete focus session in one tick
        engine.tick(10.0);
        
        // Should automatically transition to break
        assert_eq!(engine.state(), &AppState::Break);
        assert!((engine.session.remaining - 15.0).abs() < 0.01);
    }
    
    #[test]
    fn test_manual_pause_resume() {
        let mut engine = PomodoroEngine::new();
        engine.config.focus_duration = 20.0;
        
        engine.start_focus();
        assert!((engine.session.remaining - 20.0).abs() < 0.01);
        
        // Pause for 5 seconds
        engine.pause(5.0);
        
        // Should have increased remaining time (paused)
        assert!((engine.session.remaining - 15.0).abs() < 0.01);
        
        // Resume and tick
        engine.resume();
        engine.tick(10.0);
        
        // Should have ~5 seconds remaining (20 - 5 pause - 10 tick)
        assert!((engine.session.remaining - 5.0).abs() < 0.01);
    }
    
    #[test]
    fn test_long_break_after_4_sessions() {
        let mut engine = PomodoroEngine::new();
        engine.config.focus_duration = 5.0;      // Short for test
        engine.config.short_break_duration = 2.0; // 2 min short break
        engine.config.long_break_duration = 10.0; // 10 min long break
        engine.config.sessions_before_long_break = 2; // Long break every 2 sessions
        
        engine.start_focus();
        
        // Complete first session
        engine.tick(5.0);
        assert_eq!(engine.state(), &AppState::Break);
        assert!((engine.session.remaining - 2.0).abs() < 0.01);
        
        // Complete second session (should trigger long break)
        engine.tick(2.0);
        assert_eq!(engine.state(), &AppState::Break);
        assert!((engine.session.remaining - 10.0).abs() < 0.01);
    }
}
