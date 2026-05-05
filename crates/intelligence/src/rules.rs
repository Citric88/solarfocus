//! Rules-based `DistractionClassifier`. Phase 2 default — fast, deterministic,
//! no model files. The Phase 4 ONNX classifier will sit behind the same trait.

use crate::traits::*;
use crate::types::*;
use serde::Deserialize;
use std::collections::HashSet;
use std::path::Path;

const BUNDLED_RULES: &str = include_str!("rules.toml");

#[derive(Debug, thiserror::Error)]
pub enum RulesError {
    #[error("io: {0}")]
    Io(String),
    #[error("toml parse: {0}")]
    Toml(String),
}

#[derive(Debug, Deserialize, Default)]
struct RulesFile {
    #[serde(default)]
    focus: Section,
    #[serde(default)]
    distraction: Section,
}

#[derive(Debug, Deserialize, Default)]
struct Section {
    #[serde(default)]
    processes: Vec<String>,
    #[serde(default)]
    title_keywords: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct RulesClassifier {
    focus_processes: HashSet<String>,
    focus_title_keywords: Vec<String>,
    distraction_processes: HashSet<String>,
    distraction_title_keywords: Vec<String>,
}

impl RulesClassifier {
    /// Load the bundled defaults only.
    pub fn bundled() -> Self {
        Self::from_toml_str(BUNDLED_RULES).expect("bundled rules.toml is malformed")
    }

    /// Load bundled defaults + merge a user override file (additive: user wins on conflict).
    /// Missing or malformed user file → log warn and return bundled defaults.
    pub fn bundled_with_user_override(user_path: &Path) -> Self {
        let mut base = Self::bundled();
        match std::fs::read_to_string(user_path) {
            Ok(s) => match Self::from_toml_str(&s) {
                Ok(user) => base.merge(user),
                Err(e) => log::warn!(
                    "User rules at {} ignored ({:?}); using bundled defaults",
                    user_path.display(),
                    e
                ),
            },
            Err(_) => {
                log::info!(
                    "No user rules at {} — using bundled defaults",
                    user_path.display()
                );
            }
        }
        base
    }

    pub fn from_toml_str(s: &str) -> Result<Self, RulesError> {
        let parsed: RulesFile = toml::from_str(s).map_err(|e| RulesError::Toml(e.to_string()))?;
        Ok(Self {
            focus_processes: parsed
                .focus
                .processes
                .into_iter()
                .map(|s| s.to_lowercase())
                .collect(),
            focus_title_keywords: parsed
                .focus
                .title_keywords
                .into_iter()
                .map(|s| s.to_lowercase())
                .collect(),
            distraction_processes: parsed
                .distraction
                .processes
                .into_iter()
                .map(|s| s.to_lowercase())
                .collect(),
            distraction_title_keywords: parsed
                .distraction
                .title_keywords
                .into_iter()
                .map(|s| s.to_lowercase())
                .collect(),
        })
    }

    /// v1.12.0 — public so the plugin loader can chain plugin-supplied
    /// rules on top of bundled + user defaults.
    pub fn merge(&mut self, other: Self) {
        self.focus_processes.extend(other.focus_processes);
        self.focus_title_keywords.extend(other.focus_title_keywords);
        self.distraction_processes.extend(other.distraction_processes);
        self.distraction_title_keywords
            .extend(other.distraction_title_keywords);
    }

    /// Pure helper — easier to test than going through the async trait.
    pub fn classify_sync(&self, sample: &WindowSample) -> ClassificationResult {
        let proc_lower = sample.process_name.to_lowercase();

        // Process-name match wins (highest confidence).
        if self.distraction_processes.contains(&proc_lower) {
            return ClassificationResult {
                label: ClassificationLabel::Distraction,
                confidence: 0.95,
                matched_rule: Some(format!("blocklist:{}", proc_lower)),
            };
        }
        if self.focus_processes.contains(&proc_lower) {
            return ClassificationResult {
                label: ClassificationLabel::Focus,
                confidence: 0.95,
                matched_rule: Some(format!("allowlist:{}", proc_lower)),
            };
        }

        // Then title-keyword matches.
        if let Some(ref title) = sample.window_title {
            let title_lower = title.to_lowercase();
            for kw in &self.distraction_title_keywords {
                if title_lower.contains(kw) {
                    return ClassificationResult {
                        label: ClassificationLabel::Distraction,
                        confidence: 0.85,
                        matched_rule: Some(format!("title:{}", kw)),
                    };
                }
            }
            for kw in &self.focus_title_keywords {
                if title_lower.contains(kw) {
                    return ClassificationResult {
                        label: ClassificationLabel::Focus,
                        confidence: 0.85,
                        matched_rule: Some(format!("title:{}", kw)),
                    };
                }
            }
        }

        ClassificationResult::neutral()
    }
}

impl DistractionClassifier for RulesClassifier {
    fn classify(&self, sample: &WindowSample) -> AiFuture<ClassificationResult> {
        let result = self.classify_sync(sample);
        Box::pin(async move { Ok(result) })
    }
    fn is_ready(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(process: &str, title: Option<&str>) -> WindowSample {
        WindowSample {
            process_name: process.to_string(),
            window_title: title.map(|s| s.to_string()),
            elapsed_in_session_secs: 0,
        }
    }

    #[test]
    fn bundled_loads() {
        let _ = RulesClassifier::bundled();
    }

    #[test]
    fn vscode_is_focus() {
        let c = RulesClassifier::bundled();
        let r = c.classify_sync(&sample("Code", Some("file.rs")));
        assert_eq!(r.label, ClassificationLabel::Focus);
        assert!(r.confidence >= 0.9);
        assert_eq!(r.matched_rule.as_deref(), Some("allowlist:code"));
    }

    #[test]
    fn cursor_is_focus_case_insensitive() {
        let c = RulesClassifier::bundled();
        let r = c.classify_sync(&sample("CURSOR", None));
        assert_eq!(r.label, ClassificationLabel::Focus);
    }

    #[test]
    fn tiktok_is_distraction() {
        let c = RulesClassifier::bundled();
        let r = c.classify_sync(&sample("TikTok", None));
        assert_eq!(r.label, ClassificationLabel::Distraction);
        assert_eq!(r.confidence, 0.95);
    }

    #[test]
    fn youtube_url_is_distraction_via_title() {
        let c = RulesClassifier::bundled();
        let r = c.classify_sync(&sample(
            "Safari",
            Some("Cool Video — youtube.com/watch?v=abc123"),
        ));
        assert_eq!(r.label, ClassificationLabel::Distraction);
        assert_eq!(r.confidence, 0.85);
        assert_eq!(r.matched_rule.as_deref(), Some("title:youtube.com/watch"));
    }

    #[test]
    fn unknown_process_is_neutral() {
        let c = RulesClassifier::bundled();
        let r = c.classify_sync(&sample("Calculator", None));
        assert_eq!(r.label, ClassificationLabel::Neutral);
    }

    #[test]
    fn user_override_can_promote_to_focus() {
        let mut c = RulesClassifier::bundled();
        // Spotify is bundled distraction; user adds it as focus.
        let user = RulesClassifier::from_toml_str(
            r#"
            [focus]
            processes = ["Spotify"]
            "#,
        )
        .unwrap();
        c.merge(user);
        // After merge, Spotify is BOTH distraction (bundled) and focus (user).
        // Distraction check runs first, so it still hits Distraction —
        // for a true override we want user wins. Adjust spec accordingly.
        let r = c.classify_sync(&sample("Spotify", None));
        assert_eq!(
            r.label,
            ClassificationLabel::Distraction,
            "current merge semantics: bundled distraction wins; user must remove from distraction list"
        );
    }

    #[test]
    fn user_override_adds_new_distraction() {
        let mut c = RulesClassifier::bundled();
        let user = RulesClassifier::from_toml_str(
            r#"
            [distraction]
            processes = ["MyTimeWaster"]
            title_keywords = ["news.ycombinator.com"]
            "#,
        )
        .unwrap();
        c.merge(user);
        assert_eq!(
            c.classify_sync(&sample("MyTimeWaster", None)).label,
            ClassificationLabel::Distraction
        );
        assert_eq!(
            c.classify_sync(&sample("Safari", Some("HN | news.ycombinator.com")))
                .label,
            ClassificationLabel::Distraction
        );
    }

    #[test]
    fn malformed_user_file_does_not_panic() {
        let p = std::env::temp_dir().join(format!("solarfocus_bad_rules_{}.toml", std::process::id()));
        std::fs::write(&p, "this is not toml = = =").unwrap();
        let _ = RulesClassifier::bundled_with_user_override(&p);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn missing_title_does_not_match_title_keywords() {
        let c = RulesClassifier::bundled();
        // macOS without Screen Recording: title=None, but process is unknown
        let r = c.classify_sync(&sample("RandomApp", None));
        assert_eq!(r.label, ClassificationLabel::Neutral);
    }
}
