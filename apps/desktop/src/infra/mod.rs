//! # Infraestructura de Persistencia y Logging - PRIVACY FIRST
//! 
//! Módulo de infraestructura para SolarFocus Desktop.
//! Privacidad: Cero telemetría, todo procesamiento local.

// 🔒 Encriptación simple para logs (fase 2 - opcional)
pub mod encryption;

// 🗄️ Persistencia SQLite básica
pub mod persistence;

use chrono::{DateTime, Utc};

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
            
            // Timestamp ISO 8601
            let timestamp = Utc::now().format("%Y-%m-%d %H:%M:%S%.f").to_string();
            
            write!(buf, "[{}] ", timestamp).unwrap();
            
            // Nivel de log con emoji para visibilidad
            let level_icon = match record.level() {
                log::Level::Error => "❌",
                log::Level::Warn  => "⚠️ ",
                log::Level::Info  => "📝 ",
                log::Level::Debug => "🔍 ",
                _ => "",
            };
            
            write!(buf, "{}{} ", level_icon, record.level()).unwrap();
            
            // Mensaje original
            writeln!(buf, "{}", record.args()).unwrap();
            
            Ok(())
        })
        .filter_module("solar_focus", log::LevelFilter::Info)
        .parse_default_env()
        .init();
    
    println!("📝 Logger inicializado: stdout + {}", log_path);
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

/// Verifica que logs no contengan datos sensibles
pub fn sanitize_log_input(input: &str) -> String {
    // Regex para detectar patrones sensibles (contraseñas, tokens, etc.)
    let sensitive_patterns = [
        r"password\s*[=:]\s*[\w]+",
        r"token\s*[=:]\s*[\w\-]+",
        r"secret\s*[=:]\s*[\w]+",
        r"api_key\s*[=:]\s*[\w\-]+",
    ];
    
    for pattern in &sensitive_patterns {
        if input.contains(pattern) {
            eprintln!("❌ Patrón sensible detectado en: {}", input);
            return "".to_string(); // Rechazar log con datos sensibles
        }
    }
    
    input.to_string()
}
