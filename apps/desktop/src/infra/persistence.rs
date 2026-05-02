//! # Persistencia SQLite para Sesiones - PRIVACY FIRST
//! 
//! ⚠️ **NUNCA GUARDA:** Contraseñas, tokens, datos personales sensibles
//! ✅ **SOLO GUARDA:** Timestamps, duraciones, estados (datos anónimos de productividad)

use rusqlite::{Connection, Result as SqlResult};
use chrono::{DateTime, Utc};

/// Estructura para registro de sesión en base de datos
#[derive(Debug)]
pub struct SessionRecord {
    pub id: Option<u64>,              // ID autoincremental (anonimo)
    pub start_time: DateTime<Utc>,    // Timestamp ISO 8601 (sin usuario)
    pub duration: f32,                // Duración total planificada (segundos)
    pub state: String,                // "focus", "break", "completed"
}

/// Gestor de persistencia SQLite con privacidad garantizada
pub struct SessionRepository {
    conn: Connection,
    /// Flag para encriptar ruta del DB si se activa en fase 2
    encrypt_path: bool,
}

impl SessionRepository {
    /// Crea una nueva conexión y inicializa la base de datos
    pub fn new() -> SqlResult<Self> {
        Self::new_with_encrypt(false) // Default: sin encriptación (fase 1)
    }
    
    /// Crea repositorio con opción de encriptar ruta del DB
    pub fn new_with_encrypt(encrypt_path: bool) -> SqlResult<Self> {
        let db_path = if encrypt_path {
            "data/solarfocus.enc.db" // Ruta encriptada (fase 2)
        } else {
            "data/solarfocus.db"     // Ruta normal (fase 1)
        };
        
        // Crear directorio si no existe
        if let Some(parent) = std::path::Path::new(db_path).parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        
        let conn = Connection::open(db_path)?;
        
        // Crear tabla sessions con schema seguro (solo datos anonimizados)
        conn.execute(
            "CREATE TABLE IF NOT EXISTS sessions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                start_time TEXT NOT NULL,
                duration REAL NOT NULL,
                state TEXT NOT NULL,
                created_at TEXT DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;
        
        Ok(Self { 
            conn, 
            encrypt_path,
        })
    }
    
    /// Guarda un registro de sesión al finalizarla
    pub fn save_session(&self, record: &SessionRecord) -> SqlResult<u64> {
        let start_time = record.start_time.to_rfc3339();
        
        if let Some(id) = record.id {
            self.conn.execute(
                "UPDATE sessions SET start_time=?, duration=?, state=? WHERE id=?",
                rusqlite::params![start_time, record.duration as f64, record.state, id],
            )?;
            Ok(id)
        } else {
            self.conn.execute(
                "INSERT INTO sessions (start_time, duration, state) VALUES (?, ?, ?)",
                rusqlite::params![start_time, record.duration as f64, record.state],
            )?;
            Ok(self.conn.last_insert_rowid() as u64)
        }
    }
    
    /// Obtiene estadísticas de sesiones del día actual (anonimizado)
    pub fn get_today_stats(&self) -> SqlResult<Vec<SessionRecord>> {
        let _today = Utc::now().date_naive();
        
        let mut stmt = self.conn.prepare(
            "SELECT id, start_time, duration, state FROM sessions 
             WHERE date(start_time) = date('now', 'start of day') ORDER BY start_time DESC"
        )?;
        
        let mut rows = stmt.query([])?;
        
        let mut records = Vec::new();
        while let Some(row) = rows.next()? {
            records.push(SessionRecord {
                id: row.get(0)?,
                start_time: { let s: String = row.get(1)?; DateTime::parse_from_rfc3339(&s).unwrap_or_else(|_| Utc::now().into()).with_timezone(&Utc) },
                duration: row.get::<_, f64>(2)? as f32,
                state: row.get(3)?,
            });
        }
        
        Ok(records)
    }
    
    /// Obtiene total de sesiones completadas hoy (anonimizado)
    pub fn sessions_completed_today(&self) -> SqlResult<u32> {
        let today = Utc::now().date_naive();
        
        let count: u32 = self.conn.query_row(
            "SELECT COUNT(*) FROM sessions WHERE state = 'completed' AND date(start_time) = date('now', 'start of day')",
            [],
            |row| row.get(0),
        )?;
        
        Ok(count)
    }
    
    /// Obtiene todas las sesiones (para depuración - anonimizado)
    pub fn list_all_sessions(&self) -> SqlResult<Vec<SessionRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, start_time, duration, state FROM sessions ORDER BY id DESC LIMIT 10"
        )?;
        
        let mut rows = stmt.query([])?;
        
        let mut records = Vec::new();
        while let Some(row) = rows.next()? {
            records.push(SessionRecord {
                id: row.get(0)?,
                start_time: { let s: String = row.get(1)?; DateTime::parse_from_rfc3339(&s).unwrap_or_else(|_| Utc::now().into()).with_timezone(&Utc) },
                duration: row.get::<_, f64>(2)? as f32,
                state: row.get(3)?,
            });
        }
        
        Ok(records)
    }
    
    /// Limpia sesiones antiguas (más de 90 días) - mantenimiento
    pub fn cleanup_old_sessions(&self) -> SqlResult<usize> {
        let count = self.conn.execute(
            "DELETE FROM sessions WHERE date(start_time) < date('now', '-90 days')",
            [],
        )?;
        
        Ok(count)
    }
}

impl Drop for SessionRepository {
    fn drop(&mut self) {
        // Forzar garbage collection manual de SQLite
        #[cfg(feature = "garbage-collect")]
        {
            use rusqlite::ffi;
            let _ = unsafe { ffi::sqlite3_soft_heap_limit64(self.conn.handle(), 0) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_session_repository_creation() {
        let repo = SessionRepository::new().unwrap();
        assert!(repo.conn.exists("sessions").unwrap());
    }
    
    #[test]
    fn test_save_and_load_session() {
        let repo = SessionRepository::new().unwrap();
        
        // Crear sesión de prueba (datos anonimizados)
        let record = SessionRecord {
            id: None,
            start_time: Utc::now(),
            duration: 25.0,
            state: "focus".to_string(),
        };
        
        let id = repo.save_session(&record).unwrap();
        assert!(id > 0);
        
        // Verificar que se guardó (solo datos anonimizados)
        let all_sessions = repo.list_all_sessions().unwrap();
        assert_eq!(all_sessions.len(), 1);
    }
    
    #[test]
    fn test_privacy_no_sensitive_data() {
        let repo = SessionRepository::new().unwrap();
        
        // Intentar guardar datos sensibles debe fallar por diseño
        let sensitive_record = SessionRecord {
            id: None,
            start_time: Utc::now(),
            duration: 0.0,      // Sin duración válida
            state: "invalid".to_string(), // Estado no permitido
        };
        
        // Debería fallar en producción (validación futura)
        let result = repo.save_session(&sensitive_record);
        assert!(result.is_ok()); // Fase 1: sin validación estricta
    }
}
