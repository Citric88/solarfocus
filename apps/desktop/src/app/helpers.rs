//! v1.6.0 — Pure helpers extracted from main.rs.
//!
//! These functions are stateless and don't depend on App or iced.
//! Tests live alongside.

use crate::infra;

/// Strip non-digits and cap length so a numeric input field stays
/// well-behaved even if the user pastes garbage.
pub fn digits_only(s: &str, max_len: usize) -> String {
    s.chars()
        .filter(|c| c.is_ascii_digit())
        .take(max_len)
        .collect()
}

/// Parse a digit string into a u32, returning None if outside [min, max].
pub fn parse_minutes(s: &str, min: u32, max: u32) -> Option<u32> {
    s.parse::<u32>().ok().filter(|m| (min..=max).contains(m))
}

#[allow(dead_code)] // No longer used after FIX-3 AI tab rewrite; kept for compatibility.
pub fn on_off(b: bool) -> &'static str {
    if b {
        "ON"
    } else {
        "OFF"
    }
}

/// Strip codepoints that cosmic-text's default font can't render
/// (emojis, exotic symbols, BOMs). Keeps Latin-1 + accented Spanish
/// characters intact. Also collapses any double whitespace from the
/// removals.
pub fn sanitize_for_display(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        let keep = match c as u32 {
            // Basic Latin + Latin-1 Supplement (covers ¡¿áéíóúñü etc).
            0x0009 | 0x000A => true,           // tab, newline
            0x0020..=0x007E => true,           // printable ASCII
            0x00A0..=0x00FF => true,           // Latin-1 supplement (¡¿ñ accents)
            0x0100..=0x017F => true,           // Latin Extended-A
            0x2010..=0x2027 => true,           // common punctuation (— – ' ' " " …)
            0x2030..=0x203F => true,           // ‰ ‹ › etc
            _ => false,
        };
        if keep {
            out.push(c);
        } else if !out.ends_with(' ') {
            out.push(' ');
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Heuristic hardware recommendation.
/// - Apple Silicon → SmolLM2 (best quality on Metal).
/// - Lower-spec systems (CPU cores < 8) → Llama-1B.
/// - Otherwise → Qwen2.5-1.5B (balanced multilingual).
pub fn recommended_model_choice() -> infra::settings::ModelChoice {
    use infra::settings::ModelChoice;
    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    if cfg!(all(target_arch = "aarch64", target_os = "macos")) {
        ModelChoice::SmolLM2
    } else if cores < 8 {
        ModelChoice::Llama1B
    } else {
        ModelChoice::Qwen15
    }
}

pub fn weekday_short(d: chrono::Weekday) -> String {
    use chrono::Weekday;
    match d {
        Weekday::Mon => "L".to_string(),
        Weekday::Tue => "M".to_string(),
        Weekday::Wed => "X".to_string(),
        Weekday::Thu => "J".to_string(),
        Weekday::Fri => "V".to_string(),
        Weekday::Sat => "S".to_string(),
        Weekday::Sun => "D".to_string(),
    }
}

/// Remove the SQLite DB, settings.json, and any model files. Returns
/// the count of paths actually deleted (best-effort; missing paths
/// skipped).
pub fn wipe_all_local_data() -> u32 {
    let mut n = 0u32;
    let mut try_remove = |p: std::path::PathBuf| {
        if p.is_file() {
            if std::fs::remove_file(&p).is_ok() {
                n += 1;
            }
        } else if p.is_dir() {
            if std::fs::remove_dir_all(&p).is_ok() {
                n += 1;
            }
        }
    };

    if let Some(d) = directories::ProjectDirs::from("os", "SolarFocus", "SolarFocus") {
        try_remove(d.config_dir().join("settings.json"));
        try_remove(d.config_dir().join("rules.toml"));
        try_remove(d.data_dir().join("solarfocus.db"));
        try_remove(d.data_dir().join("models"));
    }
    n
}

#[cfg(test)]
mod custom_duration_tests {
    use super::{digits_only, parse_minutes};

    #[test]
    fn digits_only_strips_letters_and_symbols() {
        assert_eq!(digits_only("a1b2c3!@#", 10), "123");
        assert_eq!(digits_only("", 10), "");
        assert_eq!(digits_only("abc", 10), "");
    }

    #[test]
    fn digits_only_caps_length() {
        assert_eq!(digits_only("12345", 3), "123");
        assert_eq!(digits_only("9999", 4), "9999");
    }

    #[test]
    fn parse_minutes_in_range() {
        assert_eq!(parse_minutes("1", 1, 180), Some(1));
        assert_eq!(parse_minutes("25", 1, 180), Some(25));
        assert_eq!(parse_minutes("180", 1, 180), Some(180));
    }

    #[test]
    fn parse_minutes_rejects_out_of_range() {
        assert_eq!(parse_minutes("0", 1, 180), None);
        assert_eq!(parse_minutes("181", 1, 180), None);
        assert_eq!(parse_minutes("9999", 1, 180), None);
    }

    #[test]
    fn parse_minutes_rejects_garbage() {
        assert_eq!(parse_minutes("", 1, 180), None);
        assert_eq!(parse_minutes("abc", 1, 180), None);
        assert_eq!(parse_minutes("-5", 1, 180), None);
    }
}
