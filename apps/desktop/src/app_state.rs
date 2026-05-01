use rand::Rng;

/// Enum de estados de la aplicación (Idle -> Focus -> Break)
#[derive(Debug, Clone)]
pub enum AppState {
    Idle,
    Focusing(f32), // Tiempo restante en segundos (25 min = 1500s)
    Break,
}

impl AppState {
    /// Inicia una sesión de enfoque (25 minutos estándar Pomodoro)
    pub fn start_focus(&mut self) {
        *self = AppState::Focusing(1500.0); // 25 min en segundos
    }

    /// Avanza el tiempo (llamado desde el loop principal o tick)
    pub fn tick(&mut self, delta: f32) {
        if let AppState::Focusing(seconds_left) = self {
            if *seconds_left > 0.0 {
                *seconds_left -= delta;
            } else {
                // Timeout alcanzado -> Ir a Break
                *self = AppState::Break;
            }
        }
    }

    /// Finaliza sesión y vuelve a Idle
    pub fn finish(&mut self) {
        *self = AppState::Idle;
    }

    /// Mock de Sistema: Simula si hay una distracción o presencia (Fase 0)
    /// En Fase 3 se reemplazará con Vision Engine real.
    pub fn mock_check_distraction() -> bool {
        // Retornar true aleatoriamente cada ~15 segundos para simular evento
        let mut rng = rand::thread_rng();
        rng.gen_bool(0.02) // 2% de probabilidad por tick (ajustable)
    }

    /// Formato para mostrar en UI
    pub fn display(&self) -> String {
        match self {
            AppState::Idle => "Estado: Idle".to_string(),
            AppState::Focusing(seconds) => {
                let mins = (*seconds / 60.0) as i32;
                let secs = (*seconds % 60.0) as i32;
                format!("⏱️ FOCUS: {}m {:02}s", mins, secs)
            },
            AppState::Break => "🌿 DESCANSO".to_string(),
        }
    }
}
