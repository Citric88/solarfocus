//! # Sistema de Recompensas XP
//! 
//! Implementación simple de sistema de experiencia y niveles.
//! Lógica pura sin dependencias externas.

/// XP base por segundo de sesión (segundos × 0.1 → 30s = 3 XP)
pub const BASE_XP_PER_SECOND: f32 = 0.1;

/// Multiplicador de XP según racha de sesiones
pub const STREAK_BONUS_MULTIPLIER: f32 = 1.5; // +50% XP con racha

/// Duración mínima para contar como sesión válida (segundos)
pub const MIN_VALID_SESSION_DURATION: f32 = 10.0;

/// Umbral de XP para subir de nivel
pub const XP_TO_LEVEL_UP: u32 = 100;

/// Estructura de datos para recompensas
#[derive(Debug, Clone)]
pub struct RewardData {
    /// XP total acumulado
    pub total_xp: u32,
    
    /// Nivel actual
    pub level: u8,
    
    /// Racha actual (sesiones consecutivas)
    pub current_streak: u8,
    
    /// Máximo de racha alcanzado
    pub max_streak_reached: u8,
    
    /// Sesiones completadas hoy
    pub sessions_today: u8,
}

impl RewardData {
    /// Crea nuevos datos de recompensa con valores iniciales
    pub fn new() -> Self {
        Self {
            total_xp: 0,
            level: 1,
            current_streak: 0,
            max_streak_reached: 0,
            sessions_today: 0,
        }
    }
    
    /// Calcula XP ganado por una sesión específica
    pub fn calculate_xp_for_session(&self, duration_seconds: f32) -> u32 {
        if duration_seconds < MIN_VALID_SESSION_DURATION {
            return 0;
        }
        
        let base_xp = (duration_seconds * BASE_XP_PER_SECOND).round() as u32;
        
        // Bonus por racha
        let streak_bonus = if self.current_streak > 0 {
            STREAK_BONUS_MULTIPLIER
        } else {
            1.0
        };
        
        (base_xp as f32 * streak_bonus) as u32
    }
    
    /// Aplica XP ganado por una sesión. Devuelve true si subió al menos un nivel.
    pub fn apply_xp(&mut self, xp_gained: u32) -> bool {
        if xp_gained == 0 {
            return false;
        }
        self.total_xp = self.total_xp.saturating_add(xp_gained);

        // Multi-level up: cada XP_TO_LEVEL_UP cumulativos sube un nivel (#11)
        let target_level = ((self.total_xp / XP_TO_LEVEL_UP) as u8).saturating_add(1);
        if target_level > self.level {
            self.level = target_level;
            println!("🎊 ¡Nivel {} alcanzado! 🎊", self.level);
            true
        } else {
            false
        }
    }
    
    /// Incrementa la racha actual
    pub fn increment_streak(&mut self) {
        self.current_streak += 1;
        if self.current_streak > self.max_streak_reached {
            self.max_streak_reached = self.current_streak;
        }
   }
    
    /// Resetea la racha (al iniciar día nuevo o después de break)
    pub fn reset_streak(&mut self) {
        self.current_streak = 0;
    }
    
    /// Incrementa sesiones de hoy
    pub fn increment_sessions_today(&mut self) {
        self.sessions_today += 1;
    }
    
    /// Obtiene el XP necesario para el siguiente nivel
    pub fn xp_to_next_level(&self) -> u32 {
        let target_xp = XP_TO_LEVEL_UP * (self.level as u32);
        target_xp.saturating_sub(self.total_xp)
    }

    /// Obtiene porcentaje de progreso al siguiente nivel (XP dentro del nivel actual)
    pub fn progress_to_next_level(&self) -> f32 {
        let xp_in_level = self.total_xp % XP_TO_LEVEL_UP;
        (xp_in_level as f32 / XP_TO_LEVEL_UP as f32).clamp(0.0, 1.0)
    }
    
    /// Obtiene nombre del nivel basado en XP
    pub fn level_name(&self) -> &'static str {
        match self.level {
            1 => "Novato",
            2 => "Aprendiz",
            3 => "Practicante",
            4 => "Entrenado",
            5 => "Experto",
            _ => "Maestro"
        }
    }
    
    /// Obtiene estadísticas de progreso
    pub fn progress_stats(&self) -> (u8, u8, f32) {
        (self.current_streak, self.sessions_today, self.progress_to_next_level())
    }
}

impl Default for RewardData {
    fn default() -> Self {
        Self::new()
    }
}

/// Sistema completo de recompensas que integra con PomodoroSession
pub struct RewardsSystem {
    /// Datos actuales de recompensas
    data: RewardData,
}

impl RewardsSystem {
    /// Crea un nuevo sistema de recompensas
    pub fn new() -> Self {
        Self {
            data: RewardData::new(),
        }
    }
    
    /// Registra el inicio de una sesión (para resetear racha si es nuevo día)
    pub fn session_start(&mut self, _duration_seconds: f32) {
        // En producción: verificar si pasó medianoche desde last_day_reset
        // Por ahora: simple increment
        self.data.increment_streak();
        self.data.increment_sessions_today();
    }
    
    /// Registra finalización de sesión y calcula recompensas
    pub fn session_complete(&mut self, duration_seconds: f32) -> u32 {
        let xp_gained = self.data.calculate_xp_for_session(duration_seconds);
        
        if self.data.apply_xp(xp_gained) {
            println!("⭐ Ganaste {} XP", xp_gained); // En producción: usar logger
        }
        
        xp_gained
    }
    
    /// Obtiene los datos actuales de recompensas
    pub fn data(&self) -> &RewardData {
        &self.data
    }
    
    /// Obtiene mutables a los datos
    pub fn data_mut(&mut self) -> &mut RewardData {
        &mut self.data
    }
}

impl Default for RewardsSystem {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_xp_calculation() {
        let data = RewardData::new();
        
        // Sesión de 30 segundos (base XP)
        let xp_30sec = data.calculate_xp_for_session(30.0);
        assert_eq!(xp_30sec, 3); // 30 / 10 = 3
        
        // Sesión de 60 segundos con racha
        let mut data_with_streak = RewardData::new();
        data_with_streak.increment_streak(); // Racha de 1
        let xp_60sec = data_with_streak.calculate_xp_for_session(60.0);
        assert_eq!(xp_60sec, 9); // (60 / 10) * 1.5 = 9
        
        // Sesión muy corta (< 10 segundos) - no da XP
        let xp_short = data.calculate_xp_for_session(5.0);
        assert_eq!(xp_short, 0);
    }
    
    #[test]
    fn test_level_up() {
        let mut data = RewardData::new();

        // 5 sesiones de 200s = 5 × 20 XP = 100 XP → nivel 2
        for _ in 0..5 {
            let xp = data.calculate_xp_for_session(200.0);
            data.apply_xp(xp);
        }

        assert_eq!(data.level, 2);
    }

    #[test]
    fn test_progress_calculation() {
        let mut data = RewardData::new();
        assert!((data.progress_to_next_level() - 0.0).abs() < 0.01);

        // Acumular 50 XP (mitad del camino al siguiente nivel)
        let xp = data.calculate_xp_for_session(500.0); // 500 × 0.1 = 50 XP
        data.apply_xp(xp);

        assert!((data.progress_to_next_level() - 0.5).abs() < 0.01);
    }
}
