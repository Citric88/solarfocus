//! v1.7.0 — AI / classifier builders + permission probe.
//!
//! Pure functions; no App state held. Used by App::new and update
//! handlers when the user changes settings (ToggleAi, SetRamMode,
//! SetModelChoice, SetClassifierMode).

use std::sync::Arc;

use solar_focus_intelligence::{
    Coach, DistractionClassifier, MockClassifier, MockCoach, MockSummarizer, RulesClassifier,
    Summarizer,
};

use crate::infra;
use crate::infra::settings::{ClassifierMode, Settings};
use crate::PermissionStatus;

/// Synchronous one-off probe of the foreground-window API. Cheap (~ms);
/// safe to call from update() since iced's update is on the main thread.
pub fn probe_permission_now() -> PermissionStatus {
    match infra::window_watch::WindowWatcher::poll(0) {
        Some(s) => match s.window_title {
            Some(t) if !t.trim().is_empty() => PermissionStatus::Granted,
            _ => PermissionStatus::NameOnly,
        },
        None => PermissionStatus::Denied,
    }
}

/// Select the active Coach based on settings + compile-time `llm` feature.
/// PERF-1: returns MockCoach immediately; the App hot-swaps a real
/// LlmCoach later via Message::LlmEngineLoaded.
pub fn build_coach(settings: &Settings) -> Arc<dyn Coach> {
    if !settings.ai_enabled {
        log::info!("AI disabled → using MockCoach");
        return Arc::new(MockCoach);
    }
    Arc::new(MockCoach)
}

pub fn build_summarizer(settings: &Settings) -> Arc<dyn Summarizer> {
    if !settings.ai_enabled {
        return Arc::new(MockSummarizer);
    }
    Arc::new(MockSummarizer)
}

/// PERF-1: returns true if we should bother hot-loading a real LLM at boot.
pub fn should_attempt_llm_load(settings: &Settings) -> bool {
    if !settings.ai_enabled {
        return false;
    }
    #[cfg(feature = "llm")]
    {
        use crate::infra::model_download::{manifest_for, model_present};
        if let Some(m) = manifest_for(settings.model_choice) {
            return model_present(m);
        }
    }
    false
}

pub fn build_classifier(settings: &Settings) -> Arc<dyn DistractionClassifier> {
    match settings.classifier_mode {
        ClassifierMode::Mock => Arc::new(MockClassifier),
        ClassifierMode::Rules => {
            let path = settings.effective_rules_path();
            Arc::new(RulesClassifier::bundled_with_user_override(&path))
        }
        ClassifierMode::Distilbert => {
            #[cfg(feature = "classifier")]
            {
                use crate::infra::onnx_classifier::OnnxClassifier;
                match OnnxClassifier::try_load() {
                    Ok(c) => return Arc::new(c),
                    Err(e) => {
                        log::warn!("DistilBERT unavailable ({e}) — falling back to rules");
                    }
                }
            }
            #[cfg(not(feature = "classifier"))]
            {
                log::warn!(
                    "ClassifierMode::Distilbert requested but binary built without `classifier` feature — falling back to rules"
                );
            }
            let path = settings.effective_rules_path();
            Arc::new(RulesClassifier::bundled_with_user_override(&path))
        }
    }
}
