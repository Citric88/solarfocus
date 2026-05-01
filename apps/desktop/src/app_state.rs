// apps/desktop/src/app_state.rs

use rand::Rng;

#[derive(Debug, Clone)]
pub enum AppState {
    Idle,
    Focusing(f32), // Tiempo restante en segundos
    Break,
}

impl AppState {
    pub fn start_focus(&mut self) {
        *self = AppState::Focusing(1500.0); // 25 min en segundos
    }

    pub fn tick(&mut self, delta: f32) {
        if let AppState::Focusing(seconds_left) = self {
            if *seconds_left > 0.0 {
                *seconds_left -= delta;
            } else {
                *self = AppState::Break;
            }
        }
    }

    pub fn finish(&mut self) {
        *self = AppState::Idle;
    }

    // 🔧 CORRECCIÓN: Se eliminó el punto y coma después de gen_bool() para devolver el valor booleano real.
    pub fn mock_check_distraction() -> bool {
        let mut rng = rand::thread_rng();
        rng.gen_bool(0.02) // Sin punto y coma al final, devuelve true/false directamente
    }

    pub fn display(&self) -> String {
        match self {
            AppState::Idle => "Estado: Idle".to_string(),
            AppState::Focusing(seconds) => {
                let mins = (*seconds / 60.0).round() as i32;
                let secs = ((*seconds % 60.0) * 100.0).round() as i32 / 100;
                format!("⏱️ FOCUS: {}m {:02}s", mins, secs)
            },
            AppState::Break => "🌿 DESCANSO".to_string(),
        }
    }
}
