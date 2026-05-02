//! # Persistencia SQLite para Sesiones - PRIVACY FIRST
//! 
//! ⚠️ **NUNCA GUARDA:** Contraseñas, tokens, datos personales sensibles
//! ✅ **SOLO GUARDA:** Timestamps, duraciones, estados (datos anónimos de productividad)

#![allow(dead_code)] // Public infra API; not all consumers wired yet.

use rusqlite::{Connection, Result as SqlResult};
use chrono::{DateTime, Utc};
use directories::ProjectDirs;
use std::path::PathBuf;

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
}

impl SessionRepository {
    /// Crea una nueva conexión y inicializa la base de datos
    pub fn new() -> SqlResult<Self> {
        Self::new_at_path(&Self::default_db_path())
    }

    /// Crea una conexión apuntando a una ruta concreta (útil para tests)
    pub fn new_at_path(db_path: &std::path::Path) -> SqlResult<Self> {
        if let Some(parent) = db_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let conn = Connection::open(db_path)?;
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

        // v1.2 Phase 3 — daily LLM-generated summaries.
        conn.execute(
            "CREATE TABLE IF NOT EXISTS summaries (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                date TEXT NOT NULL UNIQUE,
                text TEXT NOT NULL,
                model_id TEXT NOT NULL,
                generated_at TEXT DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;

        // v1.2 Phase 4 — opt-in coaching feedback (thumbs up/down).
        // Stored locally only — never sent anywhere.
        conn.execute(
            "CREATE TABLE IF NOT EXISTS coaching_feedback (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                created_at TEXT DEFAULT CURRENT_TIMESTAMP,
                trigger TEXT NOT NULL,
                message TEXT NOT NULL,
                rating INTEGER NOT NULL,        -- +1 or -1
                model_id TEXT NOT NULL
            )",
            [],
        )?;

        Ok(Self { conn })
    }

    pub fn save_feedback(
        &self,
        trigger: &str,
        message: &str,
        rating: i32,
        model_id: &str,
    ) -> SqlResult<u64> {
        self.conn.execute(
            "INSERT INTO coaching_feedback (trigger, message, rating, model_id) VALUES (?, ?, ?, ?)",
            rusqlite::params![trigger, message, rating, model_id],
        )?;
        Ok(self.conn.last_insert_rowid() as u64)
    }

    pub fn feedback_counts(&self) -> SqlResult<(u32, u32)> {
        let up: u32 = self.conn.query_row(
            "SELECT COUNT(*) FROM coaching_feedback WHERE rating > 0",
            [],
            |row| row.get(0),
        )?;
        let down: u32 = self.conn.query_row(
            "SELECT COUNT(*) FROM coaching_feedback WHERE rating < 0",
            [],
            |row| row.get(0),
        )?;
        Ok((up, down))
    }

    /// Phase 3 — store / fetch daily summaries.
    pub fn save_summary(&self, date: &str, text: &str, model_id: &str) -> SqlResult<u64> {
        self.conn.execute(
            "INSERT OR REPLACE INTO summaries (date, text, model_id) VALUES (?, ?, ?)",
            rusqlite::params![date, text, model_id],
        )?;
        Ok(self.conn.last_insert_rowid() as u64)
    }

    pub fn get_summary(&self, date: &str) -> SqlResult<Option<String>> {
        self.conn
            .query_row(
                "SELECT text FROM summaries WHERE date = ?",
                rusqlite::params![date],
                |row| row.get::<_, String>(0),
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })
    }

    /// Resuelve la ruta del DB según OS (#21)
    /// macOS: ~/Library/Application Support/SolarFocus/solarfocus.db
    /// Linux: ~/.local/share/solarfocus/solarfocus.db
    /// Windows: %APPDATA%\SolarFocus\solarfocus.db
    fn default_db_path() -> PathBuf {
        if let Some(proj_dirs) = ProjectDirs::from("os", "SolarFocus", "SolarFocus") {
            proj_dirs.data_dir().join("solarfocus.db")
        } else {
            PathBuf::from("data/solarfocus.db")
        }
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
        let mut stmt = self.conn.prepare(
            "SELECT id, start_time, duration, state FROM sessions
             WHERE date(start_time) = date('now', 'start of day') ORDER BY start_time DESC"
        )?;

        let mut rows = stmt.query([])?;
        let mut records = Vec::new();
        while let Some(row) = rows.next()? {
            records.push(row_to_record(row)?);
        }
        Ok(records)
    }

    /// Sesiones para una fecha concreta (ISO YYYY-MM-DD). Usado por el
    /// scheduler de resumen diario para procesar el día anterior.
    pub fn sessions_for_date(&self, date: &str) -> SqlResult<Vec<SessionRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, start_time, duration, state FROM sessions
             WHERE date(start_time) = ? ORDER BY start_time ASC",
        )?;
        let mut rows = stmt.query(rusqlite::params![date])?;
        let mut records = Vec::new();
        while let Some(row) = rows.next()? {
            records.push(row_to_record(row)?);
        }
        Ok(records)
    }

    /// Returns 7 entries (oldest first), one per day in the last 7 days
    /// including today. Each entry: (ISO date, total focus seconds).
    /// Days with no data come back as 0.
    pub fn weekly_focus_seconds(&self) -> SqlResult<Vec<(String, u32)>> {
        use chrono::{Datelike, Local, Duration as CDur};
        let today = Local::now().date_naive();
        let mut days: Vec<(String, u32)> = (0..7)
            .rev()
            .map(|n| {
                let d = today - CDur::days(n);
                (d.format("%Y-%m-%d").to_string(), 0)
            })
            .collect();

        let mut stmt = self.conn.prepare(
            "SELECT date(start_time), SUM(duration) FROM sessions
             WHERE state = 'completed' AND date(start_time) >= date('now', '-6 days')
             GROUP BY date(start_time)",
        )?;
        let mut rows = stmt.query([])?;
        while let Some(r) = rows.next()? {
            let day: String = r.get(0)?;
            let secs: f64 = r.get(1)?;
            if let Some(slot) = days.iter_mut().find(|(d, _)| d == &day) {
                slot.1 = secs as u32;
            }
        }
        // Suppress unused-import lint
        let _ = today.weekday();
        Ok(days)
    }

    /// All-time totals: (sessions completed, total focus seconds).
    pub fn lifetime_totals(&self) -> SqlResult<(u32, u32)> {
        let count: u32 = self.conn.query_row(
            "SELECT COUNT(*) FROM sessions WHERE state = 'completed'",
            [],
            |r| r.get(0),
        )?;
        let secs: f64 = self
            .conn
            .query_row(
                "SELECT COALESCE(SUM(duration), 0) FROM sessions WHERE state = 'completed'",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0.0);
        Ok((count, secs as u32))
    }

    /// Last N coaching feedback entries, newest first.
    pub fn recent_feedback(&self, limit: u32) -> SqlResult<Vec<(String, i32, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT created_at, rating, message FROM coaching_feedback
             ORDER BY id DESC LIMIT ?",
        )?;
        let mut rows = stmt.query(rusqlite::params![limit])?;
        let mut out = Vec::new();
        while let Some(r) = rows.next()? {
            out.push((
                r.get::<_, String>(0)?,
                r.get::<_, i32>(1)?,
                r.get::<_, String>(2)?,
            ));
        }
        Ok(out)
    }

    /// Resumen más reciente (para mostrar al iniciar la app).
    pub fn latest_summary(&self) -> SqlResult<Option<(String, String)>> {
        self.conn
            .query_row(
                "SELECT date, text FROM summaries ORDER BY date DESC LIMIT 1",
                [],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })
    }

    /// Obtiene total de sesiones completadas hoy (anonimizado)
    pub fn sessions_completed_today(&self) -> SqlResult<u32> {
        self.conn.query_row(
            "SELECT COUNT(*) FROM sessions WHERE state = 'completed' AND date(start_time) = date('now', 'start of day')",
            [],
            |row| row.get(0),
        )
    }

    /// Obtiene todas las sesiones (para depuración - anonimizado)
    pub fn list_all_sessions(&self) -> SqlResult<Vec<SessionRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, start_time, duration, state FROM sessions ORDER BY id DESC LIMIT 10"
        )?;

        let mut rows = stmt.query([])?;
        let mut records = Vec::new();
        while let Some(row) = rows.next()? {
            records.push(row_to_record(row)?);
        }
        Ok(records)
    }

    /// Limpia sesiones antiguas (más de 90 días) - mantenimiento
    pub fn cleanup_old_sessions(&self) -> SqlResult<usize> {
        self.conn.execute(
            "DELETE FROM sessions WHERE date(start_time) < date('now', '-90 days')",
            [],
        )
    }
}

fn row_to_record(row: &rusqlite::Row<'_>) -> SqlResult<SessionRecord> {
    let s: String = row.get(1)?;
    let start_time = DateTime::parse_from_rfc3339(&s)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now());
    Ok(SessionRecord {
        id: row.get(0)?,
        start_time,
        duration: row.get::<_, f64>(2)? as f32,
        state: row.get(3)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    fn fresh_repo() -> SessionRepository {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!("solarfocus_test_{}_{}.db", std::process::id(), n));
        let _ = std::fs::remove_file(&path);
        SessionRepository::new_at_path(&path).unwrap()
    }

    #[test]
    fn test_session_repository_creation() {
        let repo = fresh_repo();
        let count: i64 = repo
            .conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='sessions'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_save_and_load_session() {
        let repo = fresh_repo();

        let record = SessionRecord {
            id: None,
            start_time: Utc::now(),
            duration: 25.0,
            state: "focus".to_string(),
        };

        let id = repo.save_session(&record).unwrap();
        assert!(id > 0);

        let all_sessions = repo.list_all_sessions().unwrap();
        assert_eq!(all_sessions.len(), 1);
    }

    /// Phase 1: validación de datos no implementada — registra la limitación actual.
    /// Cuando se añada validación, este test debe convertirse en assert!(result.is_err()).
    #[test]
    fn test_phase1_no_strict_validation() {
        let repo = fresh_repo();

        let invalid_record = SessionRecord {
            id: None,
            start_time: Utc::now(),
            duration: 0.0,
            state: "invalid".to_string(),
        };

        let result = repo.save_session(&invalid_record);
        assert!(result.is_ok(), "Phase 1: no strict validation yet (TODO phase 2)");
    }
}
