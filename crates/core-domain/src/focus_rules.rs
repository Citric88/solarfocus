//! # Reglas Heurísticas de Foco
//! 
//! Sistema de reglas para detectar patrones de productividad y alertar al usuario.
//! Lógica pura sin dependencias externas.

use crate::state::{AppState, PomodoroSession};

/// Umbral de distracciones antes de alerta (número de sesiones fallidas)
pub const DISTRACTION_THRESHOLD: u8 = 3;

/// Duración de sesión requerida para contar como "completada" (segundos)
pub const MIN_SESSION_DURATION_FOR_XP: f32 = 10.0; // Mínimo 10 segundos para XP

/// Factor de multiplicador de XP por sesión completada
pub const XP_MULTIPLIER: f32 = 10.0;

/// Tipos de alertas que pueden generarse
#[derive(Debug, Clone, PartialEq)]
pub enum AlertType {
    /// Usuario ha tenido muchas distracciones seguidas
    TooManyDistractions(u8),
    
    /// Sesión completada - felicitación
    SessionCompleted(f32), // XP ganado
    
    /// Pausa prolongada detectada
    LongPauseDetected(f32), // Tiempo pausado (segundos)
    
    /// Final de día - resumen
    DaySummary,
}

impl std::fmt::Display for AlertType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AlertType::TooManyDistractions(count) => {
                write!(f, "⚠️ Has tenido {} distracciones seguidas. Considera hacer una pausa.", count)
            },
            AlertType::SessionCompleted(xp) => {
                write!(f, "🎉 ¡Sesión completada! Ganaste {} XP", *xp as u32)
            },
            AlertType::LongPauseDetected(duration) => {
                let mins = (*duration / 60.0).round();
                write!(f, "⏸️ Pausa prolongada detectada: {} minutos", mins as u32)
            },
            AlertType::DaySummary => {
                write!(f, "🌙 Fin del día. ¡Buen trabajo hoy!")
            },
        }
    }
}

/// Regla de productividad simple
#[derive(Debug, Clone)]
pub struct ProductivityRule {
    /// Umbral mínimo de sesiones para considerar "productivo"
    pub min_sessions_per_day: u8,
    
    /// Umbral máximo de pausas por sesión
    pub max_allowed_pauses: u8,
}

impl Default for ProductivityRule {
    fn default() -> Self {
        Self {
            min_sessions_per_day: 3,
            max_allowed_pauses: 2,
        }
    }
}

/// Motor de reglas de productividad
pub struct FocusRulesEngine {
    /// Reglas activas
    rules: Vec<ProductivityRule>,
    
    /// Contador de distracciones consecutivas
    consecutive_failures: u8,
    
    /// Historial de alertas generadas (evita spam)
    last_alert_time: Option<std::time::Instant>,
    last_alert_type: Option<AlertType>,
}

impl FocusRulesEngine {
    /// Crea un nuevo motor de reglas con configuración predeterminada
    pub fn new() -> Self {
        Self {
            rules: vec![ProductivityRule::default()],
            consecutive_failures: 0,
            last_alert_time: None,
            last_alert_type: None,
        }
    }
    
    /// Registra una distracción (ej: usuario dejó la app)
    pub fn record_distraction(&mut self) {
        self.consecutive_failures += 1;
        
        // Verificar si hemos superado el umbral de alertas
        if self.consecutive_failures >= DISTRACTION_THRESHOLD {
            let now = std::time::Instant::now();
            
            // Evitar spam: solo alertar si han pasado 5 minutos desde la última alerta
            if self.last_alert_time.map(|t| now.duration_since(t).as_secs()) 
                .unwrap_or(u64::MAX) >= 300 {
                
                let alert = AlertType::TooManyDistractions(self.consecutive_failures);
                println!("🔔 ALERTA: {}", alert); // En producción, usar logger
                
                self.last_alert_time = Some(now);
                self.last_alert_type = Some(alert.clone());
            }
        }
    }
    
    /// Registra una sesión completada
    pub fn record_session_complete(&mut self, duration_seconds: f32) {
        // Resetear fallos consecutivos al completar sesión
        self.consecutive_failures = 0;
        
        // Generar alerta de éxito si la sesión fue lo suficientemente larga
        if duration_seconds >= MIN_SESSION_DURATION_FOR_XP {
            let xp = (duration_seconds * XP_MULTIPLIER).round() as f32;
            let alert = AlertType::SessionCompleted(xp);
            
            println!("✨ {}", alert); // En producción, usar logger
            
            self.last_alert_time = Some(std::time::Instant::now());
            self.last_alert_type = Some(alert.clone());
        }
    }
    
    /// Verifica si se ha detectado una pausa prolongada (> 5 minutos)
    pub fn check_long_pause(&mut self, paused_duration: f32) -> Option<AlertType> {
        if paused_duration >= 300.0 { // 5 minutos
            let alert = AlertType::LongPauseDetected(paused_duration);
            
            println!("⏸️ {}", alert); // En producción, usar logger
            
            self.last_alert_time = Some(std::time::Instant::now());
            self.last_alert_type = Some(alert.clone());
            
            Some(alert)
        } else {
            None
        }
    }
    
    /// Verifica si el usuario es "productivo" según las reglas
    pub fn is_productive(&self, sessions_today: u8) -> bool {
        self.rules.iter().any(|rule| sessions_today >= rule.min_sessions_per_day)
    }
    
    /// Obtiene el número actual de fallos consecutivos
    pub fn consecutive_failures(&self) -> u8 {
        self.consecutive_failures
    }
    
    /// Verifica si debería generar una alerta (evita spam)
    pub fn should_alert_now(&self) -> bool {
        let now = std::time::Instant::now();
        
        match self.last_alert_time {
            Some(last_time) => {
                // Solo alertar si han pasado al menos 2 minutos desde la última alerta
                now.duration_since(last_time).as_secs() >= 120
            },
            None => true,
        }
    }
}

impl Default for FocusRulesEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_distraction_threshold_alert() {
        let mut engine = FocusRulesEngine::new();
        
        // Registrar 3 distracciones consecutivas
        for _ in 0..3 {
            engine.record_distraction();
        }
        
        assert_eq!(engine.consecutive_failures(), 3);
    }
    
    #[test]
    fn test_session_complete_resets_failures() {
        let mut engine = FocusRulesEngine::new();
        
        // Registrar algunas distracciones
        engine.record_distraction();
        engine.record_distraction();
        assert_eq!(engine.consecutive_failures(), 2);
        
        // Completar una sesión
        engine.record_session_complete(300.0);
        assert_eq!(engine.consecutive_failures(), 0);
    }
    
    #[test]
    fn test_productive_check() {
        let mut engine = FocusRulesEngine::new();
        
        // Usuario con 2 sesiones - no es productivo
        assert!(!engine.is_productive(2));
        
        // Usuario con 3 sesiones - ahora es productivo
        assert!(engine.is_productive(3));
    }
}
