//! # Motor Pomodoro Principal
//! 
//! Implementación de cuenta regresiva precisa y robusta.
//! Uso de `std::time::Instant` para precisión sin depender del loop de UI.

use crate::state::{AppState, PomodoroSession};

/// Estrategia de pausa automática entre sesiones (en segundos)
pub const DEFAULT_BREAK_DURATION: f32 = 5.0 * 60.0;   // 5 min descanso corto
pub const LONG_BREAK_DURATION: f32 = 15.0 * 60.0;     // 15 min descanso largo

/// Configuración del motor Pomodoro (todas las duraciones en SEGUNDOS)
#[derive(Debug, Clone)]
pub struct PomodoroConfig {
    /// Duración de sesión Focus (segundos)
    pub focus_duration: f32,

    /// Duración de descanso corto (segundos) - después de 1 sesión
    pub short_break_duration: f32,

    /// Duración de descanso largo (segundos) - después de N sesiones
    pub long_break_duration: f32,

    /// Número de sesiones antes de descanso largo (pomodoros)
    pub sessions_before_long_break: u8,
}

impl Default for PomodoroConfig {
    fn default() -> Self {
        Self {
            focus_duration: 25.0 * 60.0,        // 25 min en segundos
            short_break_duration: 5.0 * 60.0,   // 5 min en segundos
            long_break_duration: 15.0 * 60.0,   // 15 min en segundos
            sessions_before_long_break: 4,
        }
    }
}

/// Motor principal del Pomodoro.
/// Este componente es el "corazón del dominio" porque se encarga puramente 
/// del avance numérico del tiempo, el control de la racha (sesiones seguidas)
/// y las reglas de cambio automático entre "Trabajo" y "Descanso", aislando 
/// las reglas de negocio de cualquier interfaz gráfica o sistema operativo.
pub struct PomodoroEngine {
    /// Configuración actual
    config: PomodoroConfig,

    /// Sesión actual
    session: PomodoroSession,

    /// Contador de sesiones completadas en esta racha
    sessions_completed: u8,

    /// Si la sesión está pausada (no decrementa en tick)
    paused: bool,

    /// Duración total de la fase actual (para cálculo de progreso)
    current_total: f32,

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
        Self::with_config(PomodoroConfig::default())
    }

    /// Crea un nuevo motor con configuración personalizada
    pub fn with_config(config: PomodoroConfig) -> Self {
        let duration = config.focus_duration;
        Self {
            current_total: duration,
            session: PomodoroSession::new(duration),
            config,
            sessions_completed: 0,
            paused: false,
            engine_state: EngineState::Idle,
        }
    }

    /// Comienza una nueva sesión de Focus
    pub fn start_focus(&mut self) {
        // Reconstruir la sesión a partir de la config actual (#1)
        self.session = PomodoroSession::new(self.config.focus_duration);
        self.session.start();
        self.current_total = self.config.focus_duration;
        self.paused = false;
        self.engine_state = EngineState::Focusing;
    }

    /// Aplica un delta de tiempo (ej: desde frame anterior de UI)
    pub fn tick(&mut self, delta: f32) {
        if self.paused {
            return;
        }
        match self.engine_state {
            EngineState::Focusing => {
                // Delegar a session.tick para mantener self.remaining y el enum sincronizados (#2, #4)
                self.session.tick(delta);
                if matches!(self.session.state, AppState::Completed) {
                    self.transition_to_break();
                }
            },
            EngineState::Break => {
                if self.session.remaining > 0.0 {
                    self.session.remaining -= delta;
                    if self.session.remaining <= 0.0 {
                        self.session.remaining = 0.0;
                        self.session.state = AppState::Completed;
                    }
                }
            },
            EngineState::Idle => {}
        }
    }

    /// Transiciona a modo Break (después de completar Focus)
    pub fn transition_to_break(&mut self) {
        // Incrementar contador antes de calcular tipo de descanso (#10)
        self.sessions_completed = self.sessions_completed.saturating_add(1);

        let break_duration = if self.sessions_completed % self.config.sessions_before_long_break == 0 {
            self.config.long_break_duration
        } else {
            self.config.short_break_duration
        };

        self.session.duration = break_duration;
        self.session.remaining = break_duration;
        self.session.state = AppState::Break;
        self.current_total = break_duration;
        self.engine_state = EngineState::Break;
        self.paused = false;
    }

    /// Transiciona de vuelta a Focus (después de Break)
    pub fn transition_to_focus(&mut self) {
        // Si acabamos de tomar un descanso largo, resetear contador (#10)
        if self.sessions_completed > 0
            && self.sessions_completed % self.config.sessions_before_long_break == 0
        {
            self.sessions_completed = 0;
        }
        self.start_focus();
    }

    /// Pausa la sesión actual (#7) — congela el countdown sin perder tiempo
    pub fn pause(&mut self, _duration_seconds: f32) {
        if matches!(self.engine_state, EngineState::Focusing | EngineState::Break) {
            self.paused = true;
        }
    }

    /// Resume después de una pausa
    pub fn resume(&mut self) {
        self.paused = false;
    }

    /// Verifica si la sesión está pausada
    pub fn is_paused(&self) -> bool {
        self.paused
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

    /// Obtiene el progreso como porcentaje (0.0 a 1.0) — basado en current_total (#12)
    pub fn progress(&self) -> f32 {
        if matches!(self.engine_state, EngineState::Idle) || self.current_total == 0.0 {
            return 0.0;
        }
        let elapsed = self.current_total - self.session.remaining;
        (elapsed / self.current_total).clamp(0.0, 1.0)
    }

    /// Obtiene la configuración actual
    pub fn config(&self) -> &PomodoroConfig {
        &self.config
    }

    /// Obtiene mutable a la configuración (para que la UI pueda modificar)
    pub fn config_mut(&mut self) -> &mut PomodoroConfig {
        &mut self.config
    }

    /// Sesiones completadas en la racha actual
    pub fn sessions_completed(&self) -> u8 {
        self.sessions_completed
    }

    /// FEAT-STOP — abort whatever session/break is running and return to Idle.
    /// Does NOT increment sessions_completed (this is a cancel, not a finish).
    /// The user can immediately start a fresh session afterwards.
    pub fn reset(&mut self) {
        self.session = PomodoroSession::new(self.config.focus_duration);
        self.current_total = self.config.focus_duration;
        self.paused = false;
        self.engine_state = EngineState::Idle;
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

        assert!(matches!(engine.state(), AppState::Focusing(_)));
        assert!((engine.session.remaining - 30.0).abs() < 0.01);
        
        // Tick for 25 seconds
        engine.tick(25.0);
        
        // Should have ~5 seconds remaining
        assert!((engine.session.remaining - 5.0).abs() < 0.01);
        
        // Complete the session — auto-transition to Break
        engine.tick(6.0);

        assert_eq!(engine.state(), &AppState::Break);
        assert_eq!(engine.sessions_completed(), 1);
    }
    
    #[test]
    fn test_auto_break_transition() {
        let mut engine = PomodoroEngine::new();
        engine.config.focus_duration = 10.0;
        engine.config.short_break_duration = 5.0;
        engine.start_focus();
        assert!(matches!(engine.state(), AppState::Focusing(_)));

        // Complete focus session in one tick
        engine.tick(10.0);

        // Should automatically transition to break
        assert_eq!(engine.state(), &AppState::Break);
        assert!((engine.session.remaining - 5.0).abs() < 0.01);
    }

    #[test]
    fn test_manual_pause_resume() {
        let mut engine = PomodoroEngine::new();
        engine.config.focus_duration = 20.0;

        engine.start_focus();
        assert!((engine.session.remaining - 20.0).abs() < 0.01);

        // Pause: tick should NOT decrement
        engine.pause(0.0);
        assert!(engine.is_paused());
        engine.tick(7.0);
        assert!((engine.session.remaining - 20.0).abs() < 0.01);

        // Resume and tick
        engine.resume();
        assert!(!engine.is_paused());
        engine.tick(15.0);
        assert!((engine.session.remaining - 5.0).abs() < 0.01);
    }

    #[test]
    fn test_long_break_after_n_sessions() {
        let mut engine = PomodoroEngine::new();
        engine.config.focus_duration = 5.0;
        engine.config.short_break_duration = 2.0;
        engine.config.long_break_duration = 10.0;
        engine.config.sessions_before_long_break = 2;

        // Sesión 1: focus → short break
        engine.start_focus();
        engine.tick(5.0);
        assert_eq!(engine.state(), &AppState::Break);
        assert!((engine.session.remaining - 2.0).abs() < 0.01);

        // Sesión 2: focus → long break (cada 2)
        engine.transition_to_focus();
        assert!(matches!(engine.state(), AppState::Focusing(_)));
        engine.tick(5.0);
        assert_eq!(engine.state(), &AppState::Break);
        assert!((engine.session.remaining - 10.0).abs() < 0.01);
    }
}
