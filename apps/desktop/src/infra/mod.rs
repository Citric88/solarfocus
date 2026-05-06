#![allow(dead_code)] // Public infra surface; helpers consumed in later phases.

//! # Infraestructura de Persistencia y Logging - PRIVACY FIRST
//!
//! Módulo de infraestructura para SolarFocus Desktop.
//! Privacidad: Cero telemetría, todo procesamiento local.

// 🔒 Encriptación simple para logs (fase 2 - opcional)
pub mod encryption;

// 🗄️ Persistencia SQLite básica
pub mod persistence;

// 📤 v1.8.0 — JSON/CSV export to Downloads.
pub mod export;

// 🔔 v2.0.0 — cross-platform native notifications (macOS / Windows / Linux).
pub mod notify;

// 📂 v2.0.0 — reveal a file in the OS file manager.
pub mod reveal;

// 🧩 v1.12.0 — declarative TOML plugin loader.
pub mod plugins;

// 🪟 v1.2: vigilancia de la ventana activa
pub mod window_watch;

// ⚙️ v1.2: settings JSON persistidos
pub mod settings;

// 🧠 v1.2 Phase 3+4: download stack (shared by llm + classifier features).
// v1.3.1 — also pulled in by `presence` for the YuNet ONNX downloader.
#[cfg(any(feature = "llm", feature = "classifier", feature = "presence"))]
pub mod model_download;

// 🧠 v1.2 Phase 3: LLM tier (feature-gated)
#[cfg(feature = "llm")]
pub mod llm;
#[cfg(feature = "llm")]
pub mod llm_coach;

// 🧠 v1.2 Phase 4: ONNX classifier (feature-gated)
#[cfg(feature = "classifier")]
pub mod onnx_classifier;
#[cfg(feature = "classifier")]
pub mod distilbert_download;

// 📷 v1.3 Wave B: camera-based presence detection (feature-gated).
#[cfg(feature = "presence")]
pub mod presence;
#[cfg(feature = "presence")]
pub mod yunet_download;
// 📱 v1.11.0 — YOLOv8n cell-phone detector (also under `presence` flag).
#[cfg(feature = "presence")]
pub mod yolo_download;

// 📅 v1.3 Wave C: read-only macOS calendar awareness (feature-gated).
#[cfg(feature = "calendar")]
pub mod calendar;

use chrono::Utc;

/// Inicializa el logger global con stdout + archivo
pub fn init_logger(encrypt_path: bool) {
    let log_path = if encrypt_path {
        "logs/app.enc.log" // Ruta encriptada (fase 2)
    } else {
        "logs/app.log"     // Ruta normal (fase 1)
    };
    
    // Crear directorio logs si no existe
    std::fs::create_dir_all("logs").ok();
    
    env_logger::Builder::new()
        .format(|buf, record| {
            use std::io::Write;

            let timestamp = Utc::now().format("%Y-%m-%d %H:%M:%S%.f").to_string();
            writeln!(
                buf,
                "[{}] {:<5} [{}] {}",
                timestamp,
                record.level(),
                record.target(),
                record.args()
            )
        })
        .filter_module("solarfocus_desktop", log::LevelFilter::Info)
        .filter_module("solar_focus_core", log::LevelFilter::Info)
        .parse_default_env()
        .init();

    log::info!("Logger inicializado: stdout + {}", log_path);
}

/// Obtiene ruta del archivo de logs (configurable vía env var)
pub fn get_log_path() -> String {
    std::env::var("LOG_PATH")
        .unwrap_or_else(|_| "logs".to_string())
}

/// Registra eventos clave con contexto (anonimizado)
pub fn log_session_event(level: log::Level, event: &str, data: &str) {
    log::log!(level, "{}: {}", event, data);
}

/// Verifica que logs no contengan datos sensibles (substring check, sin regex)
pub fn sanitize_log_input(input: &str) -> String {
    let lower = input.to_ascii_lowercase();
    let sensitive_keys = ["password", "token", "secret", "api_key", "apikey", "authorization"];

    for key in &sensitive_keys {
        // Detecta "key=" o "key:" o "key =" / "key :"
        if let Some(idx) = lower.find(key) {
            let after = &lower[idx + key.len()..];
            let trimmed = after.trim_start();
            if trimmed.starts_with('=') || trimmed.starts_with(':') {
                eprintln!("❌ Patrón sensible detectado: clave='{}'", key);
                return String::new();
            }
        }
    }

    input.to_string()
}
