//! v1.8.0 — Export sessions + distractions + summaries to JSON or CSV.
//!
//! Privacy stays first: writes only to a path the user can see (Downloads).
//! No network calls, no telemetry. The exporter dumps anonymous IDs +
//! timestamps + categories — same data the user already sees in Stats.

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::Local;

use crate::infra::persistence::{SessionRecord, SessionRepository};

#[derive(Debug)]
pub enum ExportError {
    Io(std::io::Error),
    Db(rusqlite::Error),
    Path(String),
}

impl std::fmt::Display for ExportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExportError::Io(e) => write!(f, "I/O: {e}"),
            ExportError::Db(e) => write!(f, "DB: {e}"),
            ExportError::Path(s) => write!(f, "Path: {s}"),
        }
    }
}

impl From<std::io::Error> for ExportError {
    fn from(e: std::io::Error) -> Self {
        ExportError::Io(e)
    }
}
impl From<rusqlite::Error> for ExportError {
    fn from(e: rusqlite::Error) -> Self {
        ExportError::Db(e)
    }
}

/// Resolve the user's Downloads folder. Falls back to home if HOME is set
/// but Downloads is missing; final fallback is the current working dir.
fn downloads_dir() -> PathBuf {
    if let Some(home) = std::env::var_os("HOME") {
        let dl = PathBuf::from(&home).join("Downloads");
        if dl.exists() {
            return dl;
        }
        return PathBuf::from(home);
    }
    PathBuf::from(".")
}

fn timestamp_slug() -> String {
    Local::now().format("%Y%m%d-%H%M%S").to_string()
}

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        let escaped = s.replace('"', "\"\"");
        format!("\"{escaped}\"")
    } else {
        s.to_string()
    }
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

pub fn export_csv(repo: &SessionRepository) -> Result<PathBuf, ExportError> {
    let path = downloads_dir().join(format!("solarfocus-sessions-{}.csv", timestamp_slug()));
    write_csv(repo, &path)?;
    Ok(path)
}

pub fn export_json(repo: &SessionRepository) -> Result<PathBuf, ExportError> {
    let path = downloads_dir().join(format!("solarfocus-export-{}.json", timestamp_slug()));
    write_json(repo, &path)?;
    Ok(path)
}

fn write_csv(repo: &SessionRepository, path: &Path) -> Result<(), ExportError> {
    let sessions = repo.export_all_sessions()?;
    let mut f = File::create(path)?;
    writeln!(f, "id,start_time,duration_seconds,state,category")?;
    for s in &sessions {
        writeln!(
            f,
            "{},{},{},{},{}",
            s.id.unwrap_or(0),
            csv_escape(&s.start_time.to_rfc3339()),
            s.duration as u32,
            csv_escape(&s.state),
            csv_escape(&s.category),
        )?;
    }
    Ok(())
}

fn write_json(repo: &SessionRepository, path: &Path) -> Result<(), ExportError> {
    let sessions = repo.export_all_sessions()?;
    let distractions = repo.export_all_distractions()?;
    let summaries = repo.export_all_summaries()?;

    let mut f = File::create(path)?;
    writeln!(f, "{{")?;
    writeln!(f, "  \"export_version\": 1,")?;
    writeln!(f, "  \"app\": \"SolarFocus OS\",")?;
    writeln!(
        f,
        "  \"exported_at\": \"{}\",",
        Local::now().to_rfc3339()
    )?;

    writeln!(f, "  \"sessions\": [")?;
    write_sessions_json(&mut f, &sessions)?;
    writeln!(f, "  ],")?;

    writeln!(f, "  \"distractions\": [")?;
    write_distractions_json(&mut f, &distractions)?;
    writeln!(f, "  ],")?;

    writeln!(f, "  \"daily_summaries\": [")?;
    write_summaries_json(&mut f, &summaries)?;
    writeln!(f, "  ]")?;

    writeln!(f, "}}")?;
    Ok(())
}

fn write_sessions_json(f: &mut File, rows: &[SessionRecord]) -> std::io::Result<()> {
    for (i, s) in rows.iter().enumerate() {
        let comma = if i + 1 < rows.len() { "," } else { "" };
        writeln!(
            f,
            "    {{\"id\": {}, \"start_time\": \"{}\", \"duration_seconds\": {}, \"state\": \"{}\", \"category\": \"{}\"}}{comma}",
            s.id.unwrap_or(0),
            json_escape(&s.start_time.to_rfc3339()),
            s.duration as u32,
            json_escape(&s.state),
            json_escape(&s.category),
        )?;
    }
    Ok(())
}

fn write_distractions_json(
    f: &mut File,
    rows: &[(u64, String, String, Option<String>, f32)],
) -> std::io::Result<()> {
    for (i, (id, at, process_name, rule, conf)) in rows.iter().enumerate() {
        let comma = if i + 1 < rows.len() { "," } else { "" };
        let rule_json = match rule {
            Some(r) => format!("\"{}\"", json_escape(r)),
            None => "null".to_string(),
        };
        writeln!(
            f,
            "    {{\"id\": {id}, \"at\": \"{}\", \"process\": \"{}\", \"rule\": {rule_json}, \"confidence\": {:.3}}}{comma}",
            json_escape(at),
            json_escape(process_name),
            conf,
        )?;
    }
    Ok(())
}

fn write_summaries_json(
    f: &mut File,
    rows: &[(String, String, String)],
) -> std::io::Result<()> {
    for (i, (date, txt, model_id)) in rows.iter().enumerate() {
        let comma = if i + 1 < rows.len() { "," } else { "" };
        writeln!(
            f,
            "    {{\"date\": \"{}\", \"text\": \"{}\", \"model_id\": \"{}\"}}{comma}",
            json_escape(date),
            json_escape(txt),
            json_escape(model_id),
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_escape_handles_quotes_and_commas() {
        assert_eq!(csv_escape("simple"), "simple");
        assert_eq!(csv_escape("with,comma"), "\"with,comma\"");
        assert_eq!(csv_escape("with\"quote"), "\"with\"\"quote\"");
        assert_eq!(csv_escape("multi\nline"), "\"multi\nline\"");
    }

    #[test]
    fn json_escape_handles_specials() {
        assert_eq!(json_escape("plain"), "plain");
        assert_eq!(json_escape("a\"b"), "a\\\"b");
        assert_eq!(json_escape("a\\b"), "a\\\\b");
        assert_eq!(json_escape("a\nb"), "a\\nb");
    }

    #[test]
    fn timestamp_slug_format() {
        let slug = timestamp_slug();
        assert_eq!(slug.len(), 15); // YYYYMMDD-HHMMSS
        assert!(slug.chars().nth(8) == Some('-'));
    }
}
