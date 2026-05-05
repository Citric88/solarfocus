//! v1.12.0 — Declarative plugin loader.
//!
//! Plugins are TOML files dropped into
//! `<data>/SolarFocus OS/plugins/*.toml`. Each one can declare:
//!
//! - **`[metadata]`** — name, version, author, description (free form).
//! - **`[classifier_rules]`** — additional `[focus]` and `[distraction]`
//!   process/title-keyword lists. Merged on top of bundled + user
//!   `rules.toml` via `RulesClassifier::merge` so plugins are purely
//!   additive — they can extend the deny/allow lists but can't remove
//!   things the user already allowed.
//! - **`[seed_rules]`** — per-category seed bonus tables. Sums across
//!   enabled plugins; awarded on top of v1.9.0's base/attention/streak
//!   rules at SessionCompleted.
//!
//! Honest scope: this is a **data-plugin** system, not a code-plugin
//! system. TOML can't execute arbitrary code, so the privacy contract
//! is preserved by design — no sandboxing layer needed. WASM/native
//! code plugins (wasmtime / libloading) are deferred to a major when
//! there's actual demand and budget for the ABI design + security
//! review.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use solar_focus_intelligence::RulesClassifier;

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct PluginMetadata {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub description: String,
}

/// `[seed_rules]` sub-table: category name → extra seeds awarded per
/// completed valid session in that category. Allows users to value some
/// categories more than others.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct SeedRules {
    #[serde(default)]
    pub category_bonus: HashMap<String, u32>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct PluginFile {
    #[serde(default)]
    pub metadata: PluginMetadata,
    /// Raw TOML body of the embedded `[classifier_rules]` section, kept
    /// as a string so we can hand it to `RulesClassifier::from_toml_str`
    /// unchanged. None when the plugin doesn't extend rules.
    #[serde(default, rename = "classifier_rules")]
    pub classifier_rules: Option<ClassifierRulesBody>,
    #[serde(default)]
    pub seed_rules: SeedRules,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ClassifierRulesBody {
    #[serde(default)]
    pub focus: SectionBody,
    #[serde(default)]
    pub distraction: SectionBody,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct SectionBody {
    #[serde(default)]
    pub processes: Vec<String>,
    #[serde(default)]
    pub title_keywords: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Plugin {
    /// Stable id derived from the filename (sans .toml). Used as the key
    /// in `Settings::plugin_overrides` and surfaced in the Setup tab.
    pub id: String,
    pub path: PathBuf,
    pub file: PluginFile,
    pub enabled: bool,
}

/// Resolve `<data>/SolarFocus OS/plugins/`. Computed independently from
/// the model_download module because plugins ship in the default build
/// while model_download is feature-gated. Created on first scan if
/// missing.
pub fn plugins_dir() -> PathBuf {
    use directories::ProjectDirs;
    if let Some(p) = ProjectDirs::from("os", "SolarFocus", "SolarFocus") {
        p.data_dir().join("plugins")
    } else {
        PathBuf::from("plugins")
    }
}

/// Scan the plugins directory and return one Plugin per parseable file.
/// `overrides` is the persisted enable/disable map from `Settings`;
/// missing entries default to **enabled** (opt-out per plugin once
/// dropped in).
pub fn scan(overrides: &HashMap<String, bool>) -> Vec<Plugin> {
    let dir = plugins_dir();
    let _ = std::fs::create_dir_all(&dir);
    let mut out = Vec::new();
    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(e) => {
            log::info!("Plugins dir unreadable ({e}); none loaded");
            return out;
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("toml") {
            continue;
        }
        let id = match path.file_stem().and_then(|s| s.to_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };
        let body = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                log::warn!("Plugin {} unreadable: {e}", path.display());
                continue;
            }
        };
        let file: PluginFile = match toml::from_str(&body) {
            Ok(p) => p,
            Err(e) => {
                log::warn!("Plugin {} malformed: {e}", path.display());
                continue;
            }
        };
        let enabled = overrides.get(&id).copied().unwrap_or(true);
        log::info!(
            "Plugin loaded: id='{id}' name='{}' v{} enabled={enabled}",
            file.metadata.name,
            file.metadata.version
        );
        out.push(Plugin {
            id,
            path,
            file,
            enabled,
        });
    }
    out
}

/// Render an enabled plugin's `[classifier_rules]` table back into TOML
/// matching `RulesClassifier`'s expected shape, then `merge` it onto
/// the supplied classifier. Disabled plugins are silently skipped.
pub fn merge_into_classifier(classifier: &mut RulesClassifier, plugins: &[Plugin]) {
    for p in plugins {
        if !p.enabled {
            continue;
        }
        let rules = match &p.file.classifier_rules {
            Some(r) => r,
            None => continue,
        };
        let toml_body = render_classifier_toml(rules);
        match RulesClassifier::from_toml_str(&toml_body) {
            Ok(extra) => {
                classifier.merge(extra);
                log::info!(
                    "Plugin '{}' classifier_rules merged ({} focus proc, {} focus kw, {} distraction proc, {} distraction kw)",
                    p.id,
                    rules.focus.processes.len(),
                    rules.focus.title_keywords.len(),
                    rules.distraction.processes.len(),
                    rules.distraction.title_keywords.len(),
                );
            }
            Err(e) => log::warn!(
                "Plugin '{}' classifier_rules rejected: {e:?}",
                p.id
            ),
        }
    }
}

/// Sum seed bonuses across enabled plugins for a given category. Caller
/// adds the result to the v1.9.0 base/attention/streak awards.
pub fn seed_bonus_for_category(plugins: &[Plugin], category: &str) -> u32 {
    plugins
        .iter()
        .filter(|p| p.enabled)
        .map(|p| {
            p.file
                .seed_rules
                .category_bonus
                .get(category)
                .copied()
                .unwrap_or(0)
        })
        .sum()
}

fn render_classifier_toml(rules: &ClassifierRulesBody) -> String {
    fn quote_list(xs: &[String]) -> String {
        xs.iter()
            .map(|s| format!("\"{}\"", s.replace('"', "\\\"")))
            .collect::<Vec<_>>()
            .join(", ")
    }
    format!(
        "[focus]\nprocesses = [{}]\ntitle_keywords = [{}]\n\n[distraction]\nprocesses = [{}]\ntitle_keywords = [{}]\n",
        quote_list(&rules.focus.processes),
        quote_list(&rules.focus.title_keywords),
        quote_list(&rules.distraction.processes),
        quote_list(&rules.distraction.title_keywords),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_one(body: &str) -> PluginFile {
        toml::from_str(body).unwrap()
    }

    #[test]
    fn empty_plugin_parses() {
        let p = parse_one("[metadata]\nname = \"test\"");
        assert_eq!(p.metadata.name, "test");
        assert!(p.classifier_rules.is_none());
        assert!(p.seed_rules.category_bonus.is_empty());
    }

    #[test]
    fn classifier_rules_round_trip() {
        let body = r#"
[metadata]
name = "extra-distractions"
version = "0.1"

[classifier_rules.distraction]
processes = ["FortniteLauncher"]
title_keywords = ["fortnite"]
"#;
        let p = parse_one(body);
        let rules = p.classifier_rules.unwrap();
        assert_eq!(rules.distraction.processes, vec!["FortniteLauncher"]);
        // Render + reparse via RulesClassifier round-trip.
        let toml_body = render_classifier_toml(&rules);
        let extra = RulesClassifier::from_toml_str(&toml_body).unwrap();
        let _ = extra; // smoke
    }

    #[test]
    fn seed_bonus_sums_across_enabled_only() {
        let make = |id: &str, cat: &str, n: u32, on: bool| Plugin {
            id: id.to_string(),
            path: PathBuf::new(),
            file: PluginFile {
                seed_rules: SeedRules {
                    category_bonus: {
                        let mut h = HashMap::new();
                        h.insert(cat.to_string(), n);
                        h
                    },
                },
                ..Default::default()
            },
            enabled: on,
        };
        let plugins = vec![
            make("a", "Coding", 2, true),
            make("b", "Coding", 1, true),
            make("c", "Coding", 5, false), // disabled — ignored
            make("d", "Reading", 3, true),  // wrong category — ignored
        ];
        assert_eq!(seed_bonus_for_category(&plugins, "Coding"), 3);
    }
}
