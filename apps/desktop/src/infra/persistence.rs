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
#[derive(Debug, Clone)]
pub struct SessionRecord {
    pub id: Option<u64>,              // ID autoincremental (anonimo)
    pub start_time: DateTime<Utc>,    // Timestamp ISO 8601 (sin usuario)
    pub duration: f32,                // Duración total planificada (segundos)
    pub state: String,                // "focus", "break", "completed"
    pub category: String,             // v1.3 — "Deep work", "Coding", etc.
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
                category TEXT NOT NULL DEFAULT 'Focus',
                created_at TEXT DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;

        // v1.3 Wave A2 — additive migration for v1.2 DBs that pre-date the
        // category column. PRAGMA table_info() lists the columns; if
        // `category` is missing we ALTER it in. Safe and idempotent.
        let has_category: bool = {
            let mut stmt = conn.prepare("PRAGMA table_info(sessions)")?;
            let mut rows = stmt.query([])?;
            let mut found = false;
            while let Some(r) = rows.next()? {
                let name: String = r.get(1)?;
                if name == "category" {
                    found = true;
                    break;
                }
            }
            found
        };
        if !has_category {
            conn.execute(
                "ALTER TABLE sessions ADD COLUMN category TEXT NOT NULL DEFAULT 'Focus'",
                [],
            )?;
            log::info!("v1.3 migration: sessions.category column added");
        }

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

        // v1.4.0 — confirmed distraction events (after the 2-sample
        // gate). Used by Stats canvas to surface "top distractions
        // last 7 days". Process name + matched rule + confidence;
        // window title is NOT persisted (privacy contract).
        conn.execute(
            "CREATE TABLE IF NOT EXISTS distraction_events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                at TEXT NOT NULL,
                process_name TEXT NOT NULL,
                rule TEXT,
                confidence REAL NOT NULL
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

    /// FIX-4 (rc14) — Wipe all coaching_feedback rows. Called from the
    /// Coach canvas "Limpiar historial" button. Returns the number of rows
    /// removed.
    pub fn clear_feedback(&self) -> SqlResult<usize> {
        self.conn.execute("DELETE FROM coaching_feedback", [])
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
    /// macOS: ~/Library/Application Support/SolarFocus OS/solarfocus.db
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
                "UPDATE sessions SET start_time=?, duration=?, state=?, category=? WHERE id=?",
                rusqlite::params![
                    start_time,
                    record.duration as f64,
                    record.state,
                    record.category,
                    id,
                ],
            )?;
            Ok(id)
        } else {
            self.conn.execute(
                "INSERT INTO sessions (start_time, duration, state, category) VALUES (?, ?, ?, ?)",
                rusqlite::params![
                    start_time,
                    record.duration as f64,
                    record.state,
                    record.category,
                ],
            )?;
            Ok(self.conn.last_insert_rowid() as u64)
        }
    }

    /// v1.4.0 — log a confirmed distraction event.
    pub fn save_distraction(
        &self,
        process: &str,
        rule: Option<&str>,
        confidence: f32,
    ) -> SqlResult<u64> {
        let now = chrono::Local::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO distraction_events (at, process_name, rule, confidence) VALUES (?, ?, ?, ?)",
            rusqlite::params![now, process, rule, confidence as f64],
        )?;
        Ok(self.conn.last_insert_rowid() as u64)
    }

    /// v1.4.0 — top processes that confirmed as distractions over the
    /// last N days. Returns Vec<(process, count)> sorted by count desc.
    pub fn top_distractions_last_days(
        &self,
        days: u32,
        limit: u32,
    ) -> SqlResult<Vec<(String, u32)>> {
        let cutoff = format!("-{} days", days.saturating_sub(1));
        let mut stmt = self.conn.prepare(
            "SELECT process_name, COUNT(*) FROM distraction_events
             WHERE date(at) >= date('now', ?)
             GROUP BY process_name
             ORDER BY COUNT(*) DESC
             LIMIT ?",
        )?;
        let mut rows = stmt.query(rusqlite::params![cutoff, limit])?;
        let mut out = Vec::new();
        while let Some(r) = rows.next()? {
            out.push((r.get::<_, String>(0)?, r.get::<_, u32>(1)?));
        }
        Ok(out)
    }

    /// v1.3 Wave A2 — totals grouped by category over the last N days
    /// for completed focus sessions. Returns Vec<(category, total_secs,
    /// session_count)> sorted by total_secs desc.
    pub fn category_totals_last_days(&self, days: u32) -> SqlResult<Vec<(String, u32, u32)>> {
        let cutoff = format!("-{} days", days.saturating_sub(1));
        let mut stmt = self.conn.prepare(
            "SELECT category, COALESCE(SUM(duration), 0), COUNT(*) FROM sessions
             WHERE state = 'completed' AND date(start_time) >= date('now', ?)
             GROUP BY category
             ORDER BY SUM(duration) DESC",
        )?;
        let mut rows = stmt.query(rusqlite::params![cutoff])?;
        let mut out = Vec::new();
        while let Some(r) = rows.next()? {
            let cat: String = r.get(0)?;
            let secs: f64 = r.get(1)?;
            let count: u32 = r.get(2)?;
            out.push((cat, secs as u32, count));
        }
        Ok(out)
    }
    
    /// Obtiene estadísticas de sesiones del día actual (anonimizado)
    pub fn get_today_stats(&self) -> SqlResult<Vec<SessionRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, start_time, duration, state, category FROM sessions
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
            "SELECT id, start_time, duration, state, category FROM sessions
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
            "SELECT id, start_time, duration, state, category FROM sessions ORDER BY id DESC LIMIT 10"
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
        category: row.get(4)?,
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
            category: "Focus".to_string(),
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
            category: "Other".to_string(),
        };

        let result = repo.save_session(&invalid_record);
        assert!(result.is_ok(), "Phase 1: no strict validation yet (TODO phase 2)");
    }

    /// v1.3 Wave A2 — opening a v1.2-shape DB (no `category` column)
    /// runs the additive migration and existing rows default to "Focus".
    #[test]
    fn sessions_schema_migrates_v1_2_to_v1_3() {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!(
            "solarfocus_migrate_{}_{}.db",
            std::process::id(),
            n
        ));
        let _ = std::fs::remove_file(&path);

        // Pre-create a v1.2-shape DB without `category` and seed one row.
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute(
                "CREATE TABLE sessions (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    start_time TEXT NOT NULL,
                    duration REAL NOT NULL,
                    state TEXT NOT NULL,
                    created_at TEXT DEFAULT CURRENT_TIMESTAMP
                )",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO sessions (start_time, duration, state) VALUES (?, ?, ?)",
                rusqlite::params!["2026-04-30T10:00:00Z", 1500.0, "completed"],
            )
            .unwrap();
        }

        // Now open via SessionRepository — should auto-migrate.
        let repo = SessionRepository::new_at_path(&path).unwrap();
        let rows = repo.list_all_sessions().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].category, "Focus",
            "v1.2 row should default to 'Focus' after migration"
        );

        // New inserts now carry the category through.
        let r = SessionRecord {
            id: None,
            start_time: Utc::now(),
            duration: 1500.0,
            state: "completed".to_string(),
            category: "Coding".to_string(),
        };
        repo.save_session(&r).unwrap();
        let totals = repo.category_totals_last_days(7).unwrap();
        // Both legacy "Focus" and new "Coding" appear.
        let cats: Vec<&str> = totals.iter().map(|(c, _, _)| c.as_str()).collect();
        assert!(cats.contains(&"Coding"));
        assert!(cats.contains(&"Focus"));
    }

    /// v1.4.0 — confirmed distraction events persist + aggregate.
    #[test]
    fn distraction_events_persist_and_rank() {
        let repo = fresh_repo();
        repo.save_distraction("tiktok", Some("deny:tiktok"), 0.95).unwrap();
        repo.save_distraction("tiktok", Some("deny:tiktok"), 0.91).unwrap();
        repo.save_distraction("instagram", Some("deny:instagram"), 0.88).unwrap();

        let top = repo.top_distractions_last_days(7, 5).unwrap();
        assert_eq!(top.len(), 2);
        assert_eq!(top[0], ("tiktok".to_string(), 2));
        assert_eq!(top[1], ("instagram".to_string(), 1));
    }

    /// FIX-4 (rc14) — clear_feedback empties the table and zeroes the counts.
    #[test]
    fn clear_feedback_empties_table() {
        let repo = fresh_repo();
        repo.save_feedback("session", "Útil msg", 1, "smollm2").unwrap();
        repo.save_feedback("session", "Mal msg", -1, "smollm2").unwrap();
        let (up, down) = repo.feedback_counts().unwrap();
        assert_eq!((up, down), (1, 1));

        let removed = repo.clear_feedback().unwrap();
        assert_eq!(removed, 2);

        let (up_after, down_after) = repo.feedback_counts().unwrap();
        assert_eq!((up_after, down_after), (0, 0));
        assert!(repo.recent_feedback(10).unwrap().is_empty());
    }
}
