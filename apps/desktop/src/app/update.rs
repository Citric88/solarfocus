//! v1.8.0 — `App::update` lifted out of main.rs.
//!
//! Single 1,224-line match by domain. main.rs keeps just the
//! Message enum, App::new, App::view, free helpers, and `fn main`.
//! No behaviour changes vs v1.7.0 — this is a pure positional move.

#[cfg(feature = "llm")]
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use iced::Task;

use solar_focus_intelligence::{
    ClassificationLabel, ClassificationResult, CoachingTrigger, Language,
};

use chrono::Utc;

use crate::app::builders::{build_coach, build_summarizer, probe_permission_now};
use crate::app::helpers::{digits_only, parse_minutes, sanitize_for_display, wipe_all_local_data};
use crate::infra::persistence::SessionRepository;
use crate::infra::settings::Settings;
use crate::infra::window_watch::WindowWatcher;
use crate::ui::sidebar::Route;
use crate::{
    App, Message, PermissionStatus, SetupTab, SolarFocusCore, Toast, WizardStep, infra,
    today_iso_local, yesterday_iso_local,
};


impl App {
    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::StartFocus => {
                log::info!("Iniciando sesión de enfoque");
                self.pomodoro_engine.start_focus();
                self.last_state_was_completed = false;
                self.last_classification = None;
                self.session_started_at = Some(std::time::Instant::now());
                self.session_started_at_utc = Some(chrono::Utc::now());

                if self.settings.ai_enabled {
                    if self.is_coach_in_cooldown() {
                        let msg = solar_focus_intelligence::prompts::coaching_curated(
                            CoachingTrigger::SessionStart,
                            &self.focus_context(),
                        );
                        self.coach_in_curated_cooldown = true;
                        return Task::done(Message::CoachingReady(msg));
                    }
                    self.coach_in_curated_cooldown = false;
                    let fut =
                        self.coach
                            .coaching_message(CoachingTrigger::SessionStart, &self.focus_context());
                    Task::perform(fut, |result| match result {
                        Ok(s) => Message::CoachingReady(s),
                        Err(e) => {
                            log::warn!("Coach error: {e}");
                            Message::CoachingReady(String::new())
                        }
                    })
                } else {
                    Task::none()
                }
            }
            Message::Pause => {
                self.pomodoro_engine.pause(0.0);
                log::info!("Sesión pausada");
                Task::none()
            }
            Message::Resume => {
                self.pomodoro_engine.resume();
                log::info!("Sesión reanudada");
                Task::none()
            }
            Message::EndSession => {
                log::info!("Sesión terminada por el usuario");
                self.pomodoro_engine.reset();
                self.last_state_was_completed = false;
                self.session_started_at = None;
                self.session_started_at_utc = None;
                self.toast = Some(Toast {
                    text: match self.settings.language {
                        Language::Es => "Sesión terminada.".to_string(),
                        Language::En => "Session ended.".to_string(),
                    },
                    expires_at: Instant::now() + Duration::from_secs(2),
                });
                Task::none()
            }
            Message::TakeBreak => {
                self.pomodoro_engine.transition_to_break();
                log::info!("Tomando descanso manual");
                if self.settings.ai_enabled {
                    let fut =
                        self.coach
                            .coaching_message(CoachingTrigger::BreakStart, &self.focus_context());
                    Task::perform(fut, |r| match r {
                        Ok(s) => Message::CoachingReady(s),
                        Err(_) => Message::CoachingReady(String::new()),
                    })
                } else {
                    Task::none()
                }
            }
            Message::TimerTick(delta) => {
                let was_focusing = matches!(
                    self.pomodoro_engine.state(),
                    SolarFocusCore::AppState::Focusing(_)
                );

                self.pomodoro_engine.tick(delta);

                let now_break = matches!(
                    self.pomodoro_engine.state(),
                    SolarFocusCore::AppState::Break
                );
                if was_focusing && now_break && !self.last_state_was_completed {
                    self.last_state_was_completed = true;
                    return Task::done(Message::SessionCompleted);
                }
                Task::none()
            }
            Message::SessionCompleted => {
                log::info!("Sesión completada — guardando en SQLite");
                self.sessions_today = self.sessions_today.saturating_add(1);
                if let Some(ref repo) = self.session_repo {
                    let duration = self.pomodoro_engine.config().focus_duration;
                    // v1.4.1 — record the actual session start, not
                    // the completion timestamp. The old code wrote
                    // Utc::now() here, which broke attention-score
                    // window queries.
                    let start_time = self
                        .session_started_at_utc
                        .unwrap_or_else(|| Utc::now() - chrono::Duration::seconds(duration as i64));

                    // v1.8.0 — compute attention score and is_valid against
                    // the user's threshold. Score = 100 - 20*distractions
                    // (matches the Stats badge formula). is_valid=false flags
                    // the session as not-counting-for-streaks; nothing
                    // destructive — the row still persists.
                    let confirmed = repo
                        .distractions_in_session_window(&start_time, duration)
                        .unwrap_or(0);
                    let score = 100i32.saturating_sub(20 * confirmed as i32).max(0) as u8;
                    let is_valid =
                        score >= self.settings.min_attention_for_valid_session;

                    let record = infra::persistence::SessionRecord {
                        id: None,
                        start_time,
                        duration,
                        state: "completed".to_string(),
                        category: self.settings.last_category.clone(),
                        is_valid,
                    };
                    let saved_id = match repo.save_session(&record) {
                        Ok(id) => {
                            log::info!(
                                "Sesión #{id} guardada (atención={score}%, válida={is_valid})"
                            );
                            Some(id)
                        }
                        Err(e) => {
                            log::error!("Fallo guardando sesión: {}", e);
                            None
                        }
                    };

                    // v1.9.0 — seed earn rules. Only valid sessions count;
                    // invalid ones get nothing (the whole point of the
                    // threshold). Three earn paths can stack:
                    //   1. base — +1 seed per valid session
                    //   2. attention bonus — +1 if score >= 80
                    //   3. streak bonus — +1 every 4th completed session
                    //      (PomodoroEngine::sessions_completed)
                    if is_valid {
                        let _ = repo.save_seeds("session", 1, saved_id);
                        self.seeds_awarded_last = 1;
                        if score >= 80 {
                            let _ = repo.save_seeds("attention_bonus", 1, saved_id);
                            self.seeds_awarded_last += 1;
                        }
                        let streak = self.pomodoro_engine.sessions_completed();
                        if streak > 0 && streak % 4 == 0 {
                            let _ = repo.save_seeds("streak_bonus", 1, saved_id);
                            self.seeds_awarded_last += 1;
                        }
                        // v1.12.0 — plugin category bonus. Sums across
                        // every enabled plugin that names this category.
                        let plugin_bonus = crate::infra::plugins::seed_bonus_for_category(
                            &self.plugins,
                            &self.settings.last_category,
                        );
                        if plugin_bonus > 0 {
                            let _ = repo.save_seeds(
                                "plugin_bonus",
                                plugin_bonus,
                                saved_id,
                            );
                            self.seeds_awarded_last += plugin_bonus;
                        }
                        // Refresh cached totals for hot UI reads.
                        self.seeds_total_cache = repo.total_seeds().unwrap_or(0);
                    } else {
                        self.seeds_awarded_last = 0;
                    }
                }

                // v1.9.0 — surface the harvest as a toast. Coaching message
                // will follow if AI is enabled but it's slow; the toast is
                // immediate and unambiguous so the user sees the reward.
                let toast_text = if self.seeds_awarded_last > 0 {
                    match self.settings.language {
                        Language::Es => format!(
                            "[+] +{} semilla{} cosechada{} (total {})",
                            self.seeds_awarded_last,
                            if self.seeds_awarded_last == 1 { "" } else { "s" },
                            if self.seeds_awarded_last == 1 { "" } else { "s" },
                            self.seeds_total_cache,
                        ),
                        Language::En => format!(
                            "[+] +{} seed{} harvested (total {})",
                            self.seeds_awarded_last,
                            if self.seeds_awarded_last == 1 { "" } else { "s" },
                            self.seeds_total_cache,
                        ),
                    }
                } else {
                    match self.settings.language {
                        Language::Es =>
                            "Sesión guardada — Atención bajo el umbral, no hubo cosecha.".to_string(),
                        Language::En =>
                            "Session saved — focus below threshold, no harvest.".to_string(),
                    }
                };
                // v1.10.0 — Deep mode: chain straight into the next focus
                // session, skipping Break entirely. The seeds toast was
                // already set above; layer a brief "encadenando" line and
                // dispatch StartFocus, which re-fires SessionStart coaching
                // for free.
                if self.settings.deep_mode_enabled {
                    let chain_text = match self.settings.language {
                        Language::Es => format!(
                            "{} · (Deep)Encadenando otra sesión profunda…",
                            toast_text
                        ),
                        Language::En => format!(
                            "{} · (Deep)Chaining another deep session…",
                            toast_text
                        ),
                    };
                    self.toast = Some(Toast {
                        text: chain_text,
                        expires_at: Instant::now() + Duration::from_secs(4),
                    });
                    return Task::done(Message::StartFocus);
                }

                self.toast = Some(Toast {
                    text: toast_text,
                    expires_at: Instant::now() + Duration::from_secs(6),
                });

                if self.settings.ai_enabled {
                    if self.is_coach_in_cooldown() {
                        let msg = solar_focus_intelligence::prompts::coaching_curated(
                            CoachingTrigger::SessionComplete,
                            &self.focus_context(),
                        );
                        self.coach_in_curated_cooldown = true;
                        return Task::done(Message::CoachingReady(msg));
                    }
                    self.coach_in_curated_cooldown = false;
                    let fut = self
                        .coach
                        .coaching_message(CoachingTrigger::SessionComplete, &self.focus_context());
                    Task::perform(fut, |r| match r {
                        Ok(s) => Message::CoachingReady(s),
                        Err(_) => Message::CoachingReady(String::new()),
                    })
                } else {
                    Task::none()
                }
            }
            Message::WindowProbe => {
                let elapsed = self
                    .session_started_at
                    .map(|i| i.elapsed().as_secs() as u32)
                    .unwrap_or(0);
                if let Some(sample) = WindowWatcher::poll(elapsed) {
                    log::info!(
                        "Window probe: process='{}' title={:?}",
                        sample.process_name,
                        sample.window_title
                    );
                    let fut = self.classifier.classify(&sample);
                    return Task::perform(fut, |r| match r {
                        Ok(c) => Message::ClassificationReady(c),
                        Err(e) => {
                            log::warn!("Classifier error: {e}");
                            Message::ClassificationReady(ClassificationResult::neutral())
                        }
                    });
                }
                Task::none()
            }
            Message::ClassificationReady(c) => {
                log::info!(
                    "Classification: {:?} conf={:.2} rule={:?}",
                    c.label,
                    c.confidence,
                    c.matched_rule
                );
                let mut tasks: Vec<Task<Message>> = Vec::new();

                if c.label == ClassificationLabel::Distraction
                    && c.confidence >= self.settings.min_confidence
                {
                    self.consecutive_distraction_samples =
                        self.consecutive_distraction_samples.saturating_add(1);
                    if self.consecutive_distraction_samples >= self.settings.min_consecutive_samples
                    {
                        self.focus_rules.record_distraction();
                        self.distractions_today =
                            self.distractions_today.saturating_add(1);
                        // v1.4.0 — persist confirmed distraction so the
                        // Stats canvas can show recent top offenders.
                        // The "process" we record is the rule's keyword
                        // (e.g. "deny:tiktok" → "tiktok") because rule
                        // names already cluster equivalent surfaces
                        // (e.g. tiktok web vs app vs URL match).
                        if let Some(repo) = self.session_repo.as_ref() {
                            let display_name: String = c
                                .matched_rule
                                .as_deref()
                                .map(|r| {
                                    r.splitn(2, ':')
                                        .nth(1)
                                        .unwrap_or(r)
                                        .to_string()
                                })
                                .unwrap_or_else(|| "(sin nombre)".to_string());
                            let _ = repo.save_distraction(
                                &display_name,
                                c.matched_rule.as_deref(),
                                c.confidence,
                            );
                        }
                        log::warn!(
                            "Distraction confirmed (consecutive={}, today={}, rule={:?})",
                            self.consecutive_distraction_samples,
                            self.distractions_today,
                            c.matched_rule
                        );
                        // v2.0.0 — cross-platform native notification
                        // (was macOS osascript-only in v1.x). The alert
                        // reaches the user even when they're on the
                        // distracting app — toast alone misses the
                        // moment because the user is by definition not
                        // looking at SolarFocus.
                        {
                            let rule = c.matched_rule.clone()
                                .unwrap_or_else(|| "?".to_string());
                            let body = match self.settings.language {
                                Language::Es => format!("Distracción: {}. Vuelve al foco.", rule),
                                Language::En => format!("Distraction: {}. Refocus.", rule),
                            };
                            crate::infra::notify::send("SolarFocus OS", &body);
                        }
                        // v1.4.1 — auto-pause the focus session on a
                        // confirmed window distraction. Live test of
                        // v1.4.0 surfaced the gap: a notification fired
                        // and the row was logged, but the timer kept
                        // counting as if the user was focused. That's
                        // the same bug we fixed for camera absence in
                        // v1.3.x; fix it here for window distractions
                        // too. Auto-pause only when actually focusing.
                        let auto_paused = matches!(
                            self.pomodoro_engine.state(),
                            SolarFocusCore::AppState::Focusing(_)
                        ) && !self.pomodoro_engine.is_paused();
                        if auto_paused {
                            self.pomodoro_engine.pause(0.0);
                            log::warn!(
                                "Auto-paused: window distraction confirmed (rule={:?})",
                                c.matched_rule
                            );
                        }
                        let toast_text = match (self.settings.language, auto_paused) {
                            (Language::Es, true) => match &c.matched_rule {
                                Some(r) => format!("Sesión pausada por distracción ({}).", r),
                                None => "Sesión pausada por distracción.".to_string(),
                            },
                            (Language::Es, false) => match &c.matched_rule {
                                Some(r) => format!("Distracción detectada ({}).", r),
                                None => "Distracción detectada.".to_string(),
                            },
                            (Language::En, true) => match &c.matched_rule {
                                Some(r) => format!("Session paused — distraction ({}).", r),
                                None => "Session paused — distraction.".to_string(),
                            },
                            (Language::En, false) => match &c.matched_rule {
                                Some(r) => format!("Distraction detected ({}).", r),
                                None => "Distraction detected.".to_string(),
                            },
                        };
                        tasks.push(Task::done(Message::ShowToast {
                            text: toast_text,
                            expires_in_secs: 5,
                        }));
                        self.consecutive_distraction_samples = 0;
                    }
                } else {
                    // Streak broken — reset the gate
                    self.consecutive_distraction_samples = 0;
                }

                self.last_classification = Some(c);
                Task::batch(tasks)
            }
            Message::CoachingReady(s) => {
                if !s.is_empty() {
                    log::info!("Coach: {}", s);
                    self.last_coaching = Some(sanitize_for_display(&s));
                }
                Task::none()
            }
            Message::OpenSettings => {
                self.settings_open = true;
                Task::none()
            }
            Message::CloseSettings => {
                self.settings_open = false;
                self.settings.save();
                Task::none()
            }
            Message::ToggleAi(v) => {
                self.settings.ai_enabled = v;
                Task::none()
            }
            Message::ToggleWindowWatch(v) => {
                self.settings.window_watch_enabled = v;
                Task::none()
            }
            Message::SetLanguage(lang) => {
                self.settings.language = lang;
                Task::none()
            }
            Message::SetClassifierMode(mode) => {
                self.settings.classifier_mode = mode;
                self.rebuild_classifier();
                Task::none()
            }
            Message::ShowToast { text, expires_in_secs } => {
                let expires_at = Instant::now() + Duration::from_secs(expires_in_secs);
                log::info!("Toast: {} (TTL {}s)", text, expires_in_secs);
                self.toast = Some(Toast { text, expires_at });
                Task::none()
            }
            Message::DismissToast => {
                self.toast = None;
                Task::none()
            }
            Message::ToastTick => {
                if let Some(t) = &self.toast {
                    if Instant::now() >= t.expires_at {
                        self.toast = None;
                    }
                }
                Task::none()
            }

            Message::StartModelDownload => {
                self.download_error = None;
                self.spawn_download()
            }
            Message::SkipModelDownload => {
                log::info!("User skipped first-run model download");
                self.settings.model_download_skipped = true;
                self.settings.save();
                Task::none()
            }
            Message::DownloadPoll => {
                // Force a re-render so the progress bar refreshes from the
                // shared `download_progress` state. No state change needed.
                Task::none()
            }
            Message::DownloadFinished(result) => {
                match result {
                    Ok(p) => {
                        log::info!("Model downloaded: {}", p);
                        self.download_error = None;
                        self.refresh_setup_caches();
                        self.toast = Some(Toast {
                            text: match self.settings.language {
                                Language::Es => "Modelo descargado. Cargando coach IA…".to_string(),
                                Language::En => "Model downloaded. Loading AI coach…".to_string(),
                            },
                            expires_at: Instant::now() + Duration::from_secs(8),
                        });
                        return self.spawn_engine_load();
                    }
                    Err(e) => {
                        log::error!("Model download failed: {}", e);
                        let user_facing = if e.contains("cancelled") || e.contains("Cancelled") {
                            match self.settings.language {
                                Language::Es => "Descarga cancelada (puedes reanudar desde donde quedaste).".to_string(),
                                Language::En => "Download cancelled (you can resume from where you left off).".to_string(),
                            }
                        } else if e.contains("disk space") || e.contains("space") {
                            match self.settings.language {
                                Language::Es => format!("Sin espacio: {}", e),
                                Language::En => format!("Out of space: {}", e),
                            }
                        } else {
                            e
                        };
                        self.download_error = Some(user_facing);
                    }
                }
                Task::none()
            }
            Message::SpawnEngineLoad => {
                // v1.4.0 rc10 — no toast. LLM hot-swap completes in
                // <200 ms on M-series, so the "Cargando coach IA…"
                // toast (with its 20 s expiry) was nothing but visual
                // noise on every boot. Slow loads will be re-flagged
                // via a dedicated UI surface, not a transient bar.
                log::info!("Spawning background LLM load…");
                self.spawn_engine_load()
            }
            Message::LlmEngineLoaded(result) => {
                match result {
                    Ok(_loaded) => {
                        #[cfg(feature = "llm")]
                        {
                            use infra::llm_coach::{LlmCoach, LlmSummarizer};
                            let runtime = _loaded.0;
                            self.coach = Arc::new(LlmCoach::new(runtime.clone()));
                            self.summarizer = Arc::new(LlmSummarizer::new(runtime));
                            log::info!(
                                "Hot-swap complete: coach_ready={}, summarizer_ready={}",
                                self.coach.is_ready(),
                                self.summarizer.is_ready()
                            );
                            // v1.4.0 rc10 — no toast on completion;
                            // user can verify Coach state in Setup.
                        }
                    }
                    Err(e) => {
                        log::warn!("LLM load failed: {e}");
                        self.toast = Some(Toast {
                            text: format!("LLM load failed: {e}"),
                            expires_at: Instant::now() + Duration::from_secs(6),
                        });
                    }
                }
                Task::none()
            }
            Message::GenerateRecapNow => {
                let target = today_iso_local().unwrap_or_else(|| "today".into());
                self.dispatch_summary_for(target)
            }
            Message::SetModelChoice(choice) => {
                if self.settings.model_choice != choice {
                    self.settings.model_choice = choice;
                    self.settings.model_download_skipped = false;
                    self.settings.save();
                    log::info!("Model choice → {:?}", choice);

                    // ENH-5: if the chosen model is missing, auto-trigger
                    // the download instead of forcing the user to click again.
                    #[cfg(feature = "llm")]
                    {
                        use infra::model_download::{manifest_for, model_present};
                        if let Some(m) = manifest_for(choice) {
                            if !model_present(m) {
                                self.toast = Some(Toast {
                                    text: match self.settings.language {
                                        Language::Es => format!("Descargando {:?}…", choice),
                                        Language::En => format!("Downloading {:?}…", choice),
                                    },
                                    expires_at: Instant::now() + Duration::from_secs(4),
                                });
                                return self.spawn_download();
                            }
                        }
                    }

                    // Already present (or non-llm build) — just notify.
                    self.toast = Some(Toast {
                        text: match self.settings.language {
                            Language::Es => format!("Modelo: {:?} (reinicia para aplicar)", choice),
                            Language::En => format!("Model: {:?} (restart to apply)", choice),
                        },
                        expires_at: Instant::now() + Duration::from_secs(5),
                    });
                }
                Task::none()
            }
            Message::SetFocusMinutes(mins) => {
                let mins = mins.clamp(1, 180);
                self.settings.focus_minutes = mins;
                self.custom_focus_str = mins.to_string();
                self.settings.save();
                self.pomodoro_engine.config_mut().focus_duration = (mins as f32) * 60.0;
                log::info!("Focus duration → {} min", mins);
                self.toast = Some(Toast {
                    text: match self.settings.language {
                        Language::Es => format!("Foco: {} min", mins),
                        Language::En => format!("Focus: {} min", mins),
                    },
                    expires_at: Instant::now() + Duration::from_secs(2),
                });
                Task::none()
            }
            Message::SetBreakMinutes(mins) => {
                let mins = mins.clamp(1, 180);
                self.settings.break_minutes = mins;
                self.custom_break_str = mins.to_string();
                self.settings.save();
                self.pomodoro_engine.config_mut().short_break_duration = (mins as f32) * 60.0;
                log::info!("Short break → {} min", mins);
                self.toast = Some(Toast {
                    text: match self.settings.language {
                        Language::Es => format!("Pausa: {} min", mins),
                        Language::En => format!("Break: {} min", mins),
                    },
                    expires_at: Instant::now() + Duration::from_secs(2),
                });
                Task::none()
            }
            Message::SetLongBreakMinutes(mins) => {
                let mins = mins.clamp(1, 180);
                self.settings.long_break_minutes = mins;
                self.custom_long_break_str = mins.to_string();
                self.settings.save();
                self.pomodoro_engine.config_mut().long_break_duration = (mins as f32) * 60.0;
                log::info!("Long break → {} min", mins);
                Task::none()
            }
            // v1.3 Wave A1 — accept any keystroke into the buffer; if it
            // parses to a u32 in 1..=180, apply immediately. Empty / out
            // of range / non-numeric stays in the buffer but does not
            // mutate the persisted setting.
            Message::SetFocusMinutesText(s) => {
                self.custom_focus_str = digits_only(&s, 3);
                if let Some(m) = parse_minutes(&self.custom_focus_str, 1, 180) {
                    self.settings.focus_minutes = m;
                    self.settings.save();
                    self.pomodoro_engine.config_mut().focus_duration = (m as f32) * 60.0;
                }
                Task::none()
            }
            Message::SetBreakMinutesText(s) => {
                self.custom_break_str = digits_only(&s, 3);
                if let Some(m) = parse_minutes(&self.custom_break_str, 1, 180) {
                    self.settings.break_minutes = m;
                    self.settings.save();
                    self.pomodoro_engine.config_mut().short_break_duration = (m as f32) * 60.0;
                }
                Task::none()
            }
            Message::SetLongBreakMinutesText(s) => {
                self.custom_long_break_str = digits_only(&s, 3);
                if let Some(m) = parse_minutes(&self.custom_long_break_str, 1, 180) {
                    self.settings.long_break_minutes = m;
                    self.settings.save();
                    self.pomodoro_engine.config_mut().long_break_duration = (m as f32) * 60.0;
                }
                Task::none()
            }
            // v1.3 Wave A2 — set the category for the next focus session.
            // Persisted so chip selection survives restarts.
            Message::SetCategory(c) => {
                self.settings.last_category = c.clone();
                self.custom_category_str = c.clone();
                self.settings.save();
                log::info!("Category → {}", c);
                Task::none()
            }
            Message::SetCategoryText(s) => {
                // Cap to a sane length to keep DB rows bounded.
                let trimmed: String = s.chars().take(40).collect();
                self.custom_category_str = trimmed.clone();
                if !trimmed.trim().is_empty() {
                    self.settings.last_category = trimmed.trim().to_string();
                    self.settings.save();
                }
                Task::none()
            }
            // v1.3 Wave B — toggle the presence probe. First enable opens
            // the camera (triggers macOS Camera permission prompt). On
            // failure we save the error string for the UI to surface.
            #[cfg(feature = "presence")]
            Message::TogglePresence(on) => {
                self.settings.presence_enabled = on;
                self.settings.save();
                self.presence_error = None;
                if on {
                    if self.presence_probe.is_none() {
                        match infra::presence::PresenceProbe::new_with_thresholds(
                            self.settings.face_conf_min,
                        ) {
                            Ok(p) => {
                                self.presence_probe = Some(std::sync::Arc::new(p));
                                log::info!("Presence: probe initialized");
                            }
                            Err(e) => {
                                let msg = e.to_string();
                                log::warn!("Presence: init failed: {}", msg);
                                self.presence_error = Some(msg);
                                self.settings.presence_enabled = false;
                                self.settings.save();
                            }
                        }
                    }
                } else {
                    self.presence_probe = None;
                    self.last_presence = None;
                    self.consecutive_absent_samples = 0;
                }
                Task::none()
            }
            #[cfg(feature = "presence")]
            Message::PresenceProbe => {
                // Capture + brightness on the UI thread (~5 ms total).
                // YuNet + YOLO inference each throttled to once every 3 s
                // and run on tokio::task::spawn_blocking so they never
                // block the UI (each call costs ~200 ms on CPU at 640×640).
                const YUNET_THROTTLE_SECS: u64 = 3;
                const YOLO_THROTTLE_SECS: u64 = 5;
                if let Some(probe) = self.presence_probe.as_ref() {
                    let probe = probe.clone();
                    match probe.poll() {
                        Ok((sample, captured)) => {
                            let captured_at = sample.captured_at;
                            let immediate = Task::done(Message::PresenceReady(Ok(sample)));
                            let now = std::time::Instant::now();
                            let mut tasks: Vec<Task<Message>> = vec![immediate];

                            let yunet_throttle_ok = self
                                .last_yunet_at
                                .map(|t| now.duration_since(t).as_secs() >= YUNET_THROTTLE_SECS)
                                .unwrap_or(true);
                            let yolo_throttle_ok = self
                                .last_yolo_at
                                .map(|t| now.duration_since(t).as_secs() >= YOLO_THROTTLE_SECS)
                                .unwrap_or(true);

                            // Cheap clone of the bytes for YuNet; YOLO
                            // gets the original to avoid a second copy
                            // unless both inferences are scheduled this
                            // tick.
                            let frame_w = captured.width;
                            let frame_h = captured.height;
                            let yunet_bytes = if probe.yunet_engine().is_some() && yunet_throttle_ok {
                                Some(captured.bytes.clone())
                            } else {
                                None
                            };

                            if let Some(engine) = probe.yunet_engine() {
                                if yunet_throttle_ok {
                                    self.last_yunet_at = Some(now);
                                    let bytes = yunet_bytes.unwrap_or_default();
                                    let bg = Task::perform(
                                        async move {
                                            tokio::task::spawn_blocking(move || {
                                                let mut g = match engine.lock() {
                                                    Ok(g) => g,
                                                    Err(_) => return Err("yunet poisoned".to_string()),
                                                };
                                                g.infer(&bytes, frame_w, frame_h)
                                            })
                                            .await
                                            .map_err(|e| e.to_string())
                                            .and_then(|r| r)
                                            .map(|(p, c)| (p, c, captured_at))
                                        },
                                        Message::YunetVerdict,
                                    );
                                    tasks.push(bg);
                                }
                            }

                            if let Some(engine) = probe.yolo_engine() {
                                if yolo_throttle_ok {
                                    self.last_yolo_at = Some(now);
                                    let bytes = captured.bytes;
                                    let bg = Task::perform(
                                        async move {
                                            tokio::task::spawn_blocking(move || {
                                                let mut g = match engine.lock() {
                                                    Ok(g) => g,
                                                    Err(_) => return Err("yolo poisoned".to_string()),
                                                };
                                                g.infer(&bytes, frame_w, frame_h)
                                            })
                                            .await
                                            .map_err(|e| e.to_string())
                                            .and_then(|r| r)
                                            .map(|score| (score, captured_at))
                                        },
                                        Message::YoloVerdict,
                                    );
                                    tasks.push(bg);
                                }
                            }

                            if tasks.len() == 1 {
                                tasks.into_iter().next().unwrap()
                            } else {
                                Task::batch(tasks)
                            }
                        }
                        Err(e) => Task::done(Message::PresenceReady(Err(e.to_string()))),
                    }
                } else {
                    Task::none()
                }
            }
            #[cfg(feature = "presence")]
            Message::YunetVerdict(result) => {
                use infra::presence::Presence;
                match result {
                    Ok((p, conf, captured_at)) => {
                        log::info!(
                            "YuNet verdict: {:?} score={:.3} at {}",
                            p,
                            conf,
                            captured_at.format("%H:%M:%S")
                        );
                        self.last_yunet = Some((p, captured_at));
                        // YuNet is the more reliable signal — when it
                        // disagrees with brightness, prefer YuNet for
                        // the auto-pause counter.
                        match p {
                            Presence::Absent => {
                                self.consecutive_absent_samples =
                                    self.consecutive_absent_samples.saturating_add(1);
                                let threshold = self.settings.presence_absent_threshold.max(1);
                                if self.consecutive_absent_samples >= threshold
                                    && matches!(
                                        self.pomodoro_engine.state(),
                                        SolarFocusCore::AppState::Focusing(_)
                                    )
                                    && !self.pomodoro_engine.is_paused()
                                {
                                    self.pomodoro_engine.pause(0.0);
                                    log::info!(
                                        "Presence (YuNet): auto-paused after {} Absent samples",
                                        self.consecutive_absent_samples
                                    );
                                    // v1.4.0 rc5 — log camera-detected
                                    // absence as a distraction so the
                                    // attention score reflects it.
                                    if let Some(repo) = self.session_repo.as_ref() {
                                        let _ = repo.save_distraction(
                                            "ausencia (cámara)",
                                            Some("presence:absent"),
                                            1.0,
                                        );
                                    }
                                    self.toast = Some(Toast {
                                        text: match self.settings.language {
                                            Language::Es =>
                                                "Pausado: te alejaste del escritorio.".to_string(),
                                            Language::En =>
                                                "Paused: you stepped away.".to_string(),
                                        },
                                        expires_at: Instant::now() + Duration::from_secs(4),
                                    });
                                }
                            }
                            Presence::Present | Presence::Unknown => {
                                self.consecutive_absent_samples = 0;
                            }
                        }
                    }
                    Err(e) => {
                        log::warn!("YuNet inference error (background): {}", e);
                    }
                }
                Task::none()
            }
            // v1.3 Wave C — manual next-deadline input handlers.
            #[cfg(feature = "calendar")]
            Message::SetDeadlineLabel(s) => {
                let trimmed: String = s.chars().take(60).collect();
                self.settings.next_deadline_label = trimmed;
                self.settings.save();
                Task::none()
            }
            #[cfg(feature = "calendar")]
            Message::SetDeadlineTime(s) => {
                use chrono::{NaiveTime, TimeZone, Local};
                // Accept "HH:MM" only. Anything else just buffers without
                // applying.
                let cleaned: String = s
                    .chars()
                    .filter(|c| c.is_ascii_digit() || *c == ':')
                    .take(5)
                    .collect();
                self.deadline_time_str = cleaned.clone();
                if let Ok(t) = NaiveTime::parse_from_str(&cleaned, "%H:%M") {
                    let today = Local::now().date_naive();
                    if let Some(dt) = Local
                        .from_local_datetime(&today.and_time(t))
                        .single()
                    {
                        self.settings.next_deadline_at = Some(dt.to_rfc3339());
                        self.settings.save();
                    }
                }
                Task::none()
            }
            #[cfg(feature = "calendar")]
            Message::ClearDeadline => {
                self.settings.next_deadline_at = None;
                self.settings.next_deadline_label.clear();
                self.deadline_time_str.clear();
                self.settings.save();
                Task::none()
            }
            // v2.1.0 — cross-platform ICS file path.
            #[cfg(feature = "calendar")]
            Message::SetCalendarIcsPath(s) => {
                self.settings.calendar_ics_path = s.trim().to_string();
                self.settings.save();
                // Trigger a refresh so the new file is read immediately.
                if !self.settings.calendar_ics_path.is_empty() {
                    return Task::done(Message::CalendarRefresh);
                }
                Task::none()
            }
            #[cfg(feature = "calendar")]
            Message::ClearCalendarIcsPath => {
                self.settings.calendar_ics_path.clear();
                self.settings.save();
                self.calendar_events.clear();
                Task::none()
            }
            // v1.3.1 — live EventKit toggle. macOS Calendar permission
            // prompt is synchronous via EventKit's barrier-based wait;
            // the first call typically returns within ~100 ms (or
            // longer if the user is reading the prompt). Run on UI
            // thread to avoid Send bounds on Retained<EKEventStore>.
            #[cfg(feature = "calendar")]
            Message::ToggleCalendarLive(on) => {
                self.settings.calendar_live_enabled = on;
                self.settings.save();
                self.calendar_error = None;
                if on {
                    if self.calendar_reader.is_none() {
                        self.calendar_reader = Some(std::sync::Arc::new(
                            infra::calendar::ek::CalendarReader::new(),
                        ));
                    }
                    let reader = self.calendar_reader.as_ref().unwrap().clone();
                    let result = reader.request_access().map_err(|e| e.to_string());
                    Task::done(Message::CalendarAccessResult(result))
                } else {
                    self.calendar_events.clear();
                    Task::none()
                }
            }
            #[cfg(feature = "calendar")]
            Message::CalendarAccessResult(result) => match result {
                Ok(true) => {
                    log::info!("Calendar: access granted");
                    Task::done(Message::CalendarRefresh)
                }
                Ok(false) => {
                    log::info!("Calendar: access denied");
                    self.settings.calendar_live_enabled = false;
                    self.settings.save();
                    self.calendar_error = Some(match self.settings.language {
                        Language::Es => "Permiso de calendario denegado.".into(),
                        Language::En => "Calendar permission denied.".into(),
                    });
                    Task::none()
                }
                Err(e) => {
                    log::warn!("Calendar: access error: {}", e);
                    self.calendar_error = Some(e);
                    self.settings.calendar_live_enabled = false;
                    self.settings.save();
                    Task::none()
                }
            },
            #[cfg(feature = "calendar")]
            Message::CalendarRefresh => {
                use infra::calendar::CalendarSource;
                // v2.1.0 — combine events from both sources (live EventKit
                // reader on macOS + ICS file on any OS), dedup by
                // (title, start), sort by start.
                let mut combined: Vec<infra::calendar::CalendarEvent> = Vec::new();
                let mut errs: Vec<String> = Vec::new();

                if let Some(reader) = self.calendar_reader.as_ref() {
                    match reader.events_today() {
                        Ok(mut events) => combined.append(&mut events),
                        Err(e) => errs.push(format!("EventKit: {e}")),
                    }
                }

                let ics_path = self.settings.calendar_ics_path.trim();
                if !ics_path.is_empty() {
                    let src = infra::calendar::IcsFileSource::new(ics_path);
                    match src.events_today() {
                        Ok(mut events) => combined.append(&mut events),
                        Err(e) => errs.push(format!("ICS: {e}")),
                    }
                }

                // Dedup by (title, start) — same meeting can appear in
                // both EventKit and an ICS export of the same calendar.
                combined.sort_by(|a, b| a.start.cmp(&b.start).then(a.title.cmp(&b.title)));
                combined.dedup_by(|a, b| a.title == b.title && a.start == b.start);

                let result = if combined.is_empty() && !errs.is_empty() {
                    Err(errs.join(" · "))
                } else {
                    Ok(combined)
                };
                Task::done(Message::CalendarEventsLoaded(result))
            }
            #[cfg(feature = "calendar")]
            Message::CalendarEventsLoaded(result) => {
                match result {
                    Ok(events) => {
                        log::info!("Calendar: {} events loaded today", events.len());
                        self.calendar_events = events;
                        self.calendar_error = None;
                    }
                    Err(e) => {
                        log::warn!("Calendar: load error: {}", e);
                        self.calendar_error = Some(e);
                    }
                }
                Task::none()
            }
            // v1.3.1 — YuNet model download (337 KB). User-triggered
            // from the Setup → IA presence card.
            #[cfg(feature = "presence")]
            Message::DownloadYunet => {
                use infra::yunet_download;
                if yunet_download::is_present() {
                    return Task::done(Message::YunetDownloaded(Ok(())));
                }
                Task::perform(
                    async move { yunet_download::download().await.map_err(|e| e.to_string()) },
                    Message::YunetDownloaded,
                )
            }
            #[cfg(feature = "presence")]
            Message::YunetDownloaded(result) => {
                match result {
                    Ok(()) => {
                        log::info!("YuNet: download complete");
                        // Force probe re-init so it picks up the new model.
                        if self.settings.presence_enabled {
                            self.presence_probe = None;
                            return Task::done(Message::TogglePresence(true));
                        }
                    }
                    Err(e) => {
                        log::warn!("YuNet: download failed: {}", e);
                        self.presence_error = Some(e);
                    }
                }
                Task::none()
            }
            #[cfg(feature = "presence")]
            Message::DownloadYolo => {
                use crate::infra::yolo_download;
                if yolo_download::is_present() {
                    return Task::done(Message::YoloDownloaded(Ok(())));
                }
                Task::perform(
                    async move { yolo_download::download().await.map_err(|e| e.to_string()) },
                    Message::YoloDownloaded,
                )
            }
            #[cfg(feature = "presence")]
            Message::YoloDownloaded(result) => {
                match result {
                    Ok(()) => {
                        log::info!("YOLOv8n: download complete");
                        if self.settings.presence_enabled {
                            self.presence_probe = None;
                            return Task::done(Message::TogglePresence(true));
                        }
                    }
                    Err(e) => {
                        log::warn!("YOLOv8n: download failed: {}", e);
                        self.presence_error = Some(e);
                    }
                }
                Task::none()
            }
            #[cfg(feature = "presence")]
            Message::YoloVerdict(result) => {
                let phone_conf_min = self.settings.phone_conf_min;
                match result {
                    Ok((score, captured_at)) => {
                        log::info!(
                            "YOLO verdict: cell-phone score={:.3} at {}",
                            score,
                            captured_at.format("%H:%M:%S")
                        );
                        self.last_yolo_score = Some(score);
                        // Confirmation gate: 2 consecutive ≥ threshold
                        // samples (~10 s at the 5 s YOLO throttle) before
                        // we count it as a real distraction.
                        if score >= phone_conf_min {
                            self.consecutive_phone_samples =
                                self.consecutive_phone_samples.saturating_add(1);
                        } else {
                            self.consecutive_phone_samples = 0;
                        }
                        const PHONE_CONFIRM: u8 = 2;
                        if self.consecutive_phone_samples >= PHONE_CONFIRM
                            && matches!(
                                self.pomodoro_engine.state(),
                                SolarFocusCore::AppState::Focusing(_)
                            )
                        {
                            // Reset so we don't refire every tick.
                            self.consecutive_phone_samples = 0;
                            // Always log + auto-pause; same contract as
                            // window distraction handler in v1.4.1.
                            self.distractions_today =
                                self.distractions_today.saturating_add(1);
                            if let Some(repo) = self.session_repo.as_ref() {
                                let _ = repo.save_distraction(
                                    "celular (cámara)",
                                    Some("presence:phone"),
                                    score,
                                );
                            }
                            let was_already_paused =
                                self.pomodoro_engine.is_paused();
                            if !was_already_paused {
                                self.pomodoro_engine.pause(0.0);
                            }
                            self.toast = Some(Toast {
                                text: match self.settings.language {
                                    Language::Es => {
                                        if was_already_paused {
                                            "[Phone]Celular detectado en cámara.".to_string()
                                        } else {
                                            "[Phone]Sesión pausada: celular detectado en cámara.".to_string()
                                        }
                                    }
                                    Language::En => {
                                        if was_already_paused {
                                            "[Phone]Phone detected by camera.".to_string()
                                        } else {
                                            "[Phone]Session paused — phone detected by camera.".to_string()
                                        }
                                    }
                                },
                                expires_at: Instant::now() + Duration::from_secs(5),
                            });
                            // v2.0.0 — cross-platform native notification.
                            {
                                let body = match self.settings.language {
                                    Language::Es => "Celular detectado por la cámara",
                                    Language::En => "Phone detected by camera",
                                };
                                crate::infra::notify::send("SolarFocus OS", body);
                            }
                        }
                    }
                    Err(e) => {
                        log::warn!("YOLO inference failed: {}", e);
                    }
                }
                Task::none()
            }
            #[cfg(feature = "presence")]
            Message::PresenceReady(result) => {
                use infra::presence::{DetectionMode, Presence};
                match result {
                    Ok(sample) => {
                        self.last_presence = Some(sample.presence);
                        self.presence_error = None;
                        // When YuNet is the active mode, the brightness
                        // path is informational only — YuNet drives the
                        // auto-pause counter via Message::YunetVerdict
                        // because brightness is too noisy to act on.
                        let yunet_active = self
                            .presence_probe
                            .as_ref()
                            .map(|p| matches!(
                                p.mode(),
                                DetectionMode::YunetFace | DetectionMode::YunetAndYoloPhone,
                            ))
                            .unwrap_or(false);
                        if yunet_active {
                            return Task::none();
                        }
                        match sample.presence {
                            Presence::Absent => {
                                self.consecutive_absent_samples =
                                    self.consecutive_absent_samples.saturating_add(1);
                                let threshold = self.settings.presence_absent_threshold.max(1);
                                if self.consecutive_absent_samples >= threshold
                                    && matches!(
                                        self.pomodoro_engine.state(),
                                        SolarFocusCore::AppState::Focusing(_)
                                    )
                                    && !self.pomodoro_engine.is_paused()
                                {
                                    self.pomodoro_engine.pause(0.0);
                                    log::info!(
                                        "Presence: auto-paused after {} Absent samples",
                                        self.consecutive_absent_samples
                                    );
                                    // v1.4.0 rc5 — log brightness-detected
                                    // absence as a distraction event too.
                                    if let Some(repo) = self.session_repo.as_ref() {
                                        let _ = repo.save_distraction(
                                            "ausencia (luminancia)",
                                            Some("presence:absent"),
                                            sample.confidence.max(0.5),
                                        );
                                    }
                                    self.toast = Some(Toast {
                                        text: match self.settings.language {
                                            Language::Es =>
                                                "Pausado: te alejaste del escritorio.".to_string(),
                                            Language::En =>
                                                "Paused: you stepped away.".to_string(),
                                        },
                                        expires_at: Instant::now() + Duration::from_secs(4),
                                    });
                                }
                            }
                            Presence::Present | Presence::Unknown => {
                                self.consecutive_absent_samples = 0;
                            }
                        }
                    }
                    Err(e) => {
                        log::warn!("Presence: probe error: {}", e);
                        self.presence_error = Some(e);
                    }
                }
                Task::none()
            }
            Message::SetRamMode(mode) => {
                self.settings.ram_mode = mode;
                self.settings.apply_ram_mode();
                self.settings.save();
                self.rebuild_classifier();
                self.coach = build_coach(&self.settings);
                self.summarizer = build_summarizer(&self.settings);
                log::info!("RAM mode → {:?} (applied)", mode);
                Task::none()
            }
            Message::SwitchSetupTab(t) => {
                self.setup_tab = t;
                self.refresh_setup_caches();
                Task::none()
            }
            Message::ToggleSetupAdvanced => {
                self.setup_show_advanced = !self.setup_show_advanced;
                Task::none()
            }
            Message::ClearFeedbackHistory => {
                let removed = self
                    .session_repo
                    .as_ref()
                    .and_then(|r| r.clear_feedback().ok())
                    .unwrap_or(0);
                log::info!("Cleared {} coaching_feedback rows", removed);
                self.refresh_setup_caches();
                self.toast = Some(Toast {
                    text: match self.settings.language {
                        Language::Es => format!("Historial limpiado ({}).", removed),
                        Language::En => format!("History cleared ({}).", removed),
                    },
                    expires_at: Instant::now() + Duration::from_secs(3),
                });
                Task::none()
            }
            Message::WizardNext => {
                self.wizard_step = match self.wizard_step {
                    WizardStep::Welcome => WizardStep::Profile,
                    WizardStep::Profile => WizardStep::Download,
                    WizardStep::Download | WizardStep::Done => WizardStep::Done,
                };
                Task::none()
            }
            Message::WizardBack => {
                self.wizard_step = match self.wizard_step {
                    WizardStep::Welcome => WizardStep::Welcome,
                    WizardStep::Profile => WizardStep::Welcome,
                    WizardStep::Download => WizardStep::Profile,
                    WizardStep::Done => WizardStep::Done,
                };
                Task::none()
            }
            Message::WizardFinish => {
                self.settings.first_run = false;
                self.settings.save();
                self.wizard_step = WizardStep::Done;

                // FIX-3: if user picked Full mode but never triggered the
                // download in the wizard (and the model isn't already on
                // disk), land them on Setup → AI so the next visible step
                // is obvious. Otherwise → Focus.
                let needs_model_setup = self.settings.ram_mode
                    == infra::settings::RamMode::Full
                    && {
                        #[cfg(feature = "llm")]
                        {
                            use infra::model_download::{manifest_for, model_present};
                            manifest_for(self.settings.model_choice)
                                .map(|m| !model_present(m))
                                .unwrap_or(false)
                        }
                        #[cfg(not(feature = "llm"))]
                        {
                            false
                        }
                    };
                if needs_model_setup {
                    self.route = Route::Setup;
                    self.setup_tab = SetupTab::Ai;
                    self.toast = Some(Toast {
                        text: match self.settings.language {
                            Language::Es => "Descarga el modelo IA cuando estés listo.".to_string(),
                            Language::En => "Download the AI model when you're ready.".to_string(),
                        },
                        expires_at: Instant::now() + Duration::from_secs(6),
                    });
                } else {
                    self.route = Route::Focus;
                }
                Task::none()
            }
            Message::SwitchRoute(r) => {
                if r != self.route {
                    log::info!("Route → {:?}", r);
                    self.route = r;
                    if self.route != Route::Setup {
                        self.settings_open = false;
                    }
                    // BUG-B: refresh caches when entering Setup or Stats.
                    if matches!(self.route, Route::Setup | Route::Stats) {
                        self.refresh_setup_caches();
                    }
                    // v1.12.2 — re-probe permission on EVERY entry to
                    // Stats/Setup, not only when status is Unknown. The
                    // user may have granted permission in System Settings
                    // since boot; without this re-check the card was
                    // permanently stuck reporting "Sin permiso" until the
                    // user manually pressed "Re-verificar". The probe is
                    // sub-millisecond so the cost is negligible.
                    if matches!(self.route, Route::Stats | Route::Setup) {
                        return Task::done(Message::ProbePermission);
                    }
                }
                Task::none()
            }
            Message::ProbePermission => {
                // PERF-2: never block the UI thread on the OS window query.
                // active-win-pos-rs can be slow under contention (LLM load
                // running, many windows open). spawn_blocking offloads it.
                Task::perform(
                    async {
                        tokio::task::spawn_blocking(probe_permission_now)
                            .await
                            .unwrap_or(PermissionStatus::Unknown)
                    },
                    Message::PermissionProbed,
                )
            }
            Message::PermissionProbed(status) => {
                if status != self.permission_status {
                    log::info!("Permission → {:?}", status);
                    self.permission_status = status;
                }
                Task::none()
            }
            Message::OpenSystemSettings => {
                // v2.0.0 — cross-platform privacy deep-link.
                #[cfg(target_os = "macos")]
                {
                    let _ = std::process::Command::new("open")
                        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture")
                        .spawn();
                    log::info!("Opened macOS Privacy → Screen Recording");
                }
                #[cfg(target_os = "windows")]
                {
                    // Windows 10/11: ms-settings:privacy is the closest
                    // analogue. Win32 EnumWindows doesn't gate the
                    // window title behind a permission like macOS Screen
                    // Recording does, so this is mostly informational.
                    let _ = std::process::Command::new("explorer")
                        .arg("ms-settings:privacy")
                        .spawn();
                    log::info!("Opened Windows Settings → Privacy");
                }
                #[cfg(target_os = "linux")]
                {
                    log::info!("Open System Settings is not implemented on Linux");
                }
                Task::none()
            }
            Message::RequestClearData => {
                self.confirming_clear = true;
                Task::none()
            }
            Message::CancelClearData => {
                self.confirming_clear = false;
                Task::none()
            }
            Message::ConfirmClearData => {
                self.confirming_clear = false;
                let removed = wipe_all_local_data();
                log::warn!("Cleared local data: {} files/dirs removed", removed);
                self.toast = Some(Toast {
                    text: match self.settings.language {
                        Language::Es => format!("Datos borrados ({} entradas).", removed),
                        Language::En => format!("Data cleared ({} entries).", removed),
                    },
                    expires_at: Instant::now() + Duration::from_secs(5),
                });
                // Re-init persistence + settings to defaults so the running app
                // doesn't crash on stale handles.
                self.session_repo = SessionRepository::new().ok();
                self.settings = Settings::default();
                self.settings.save();
                self.recap = None;
                self.last_coaching = None;
                self.distractions_today = 0;
                self.sessions_today = 0;
                Task::none()
            }
            Message::ThumbsUp | Message::ThumbsDown => {
                // After persisting we'll refresh the cache so AI tab counts stay current.
                let rating = if matches!(message, Message::ThumbsUp) { 1 } else { -1 };
                let trigger = "session"; // generic — could differentiate per CoachingTrigger later
                let msg_text = self.last_coaching.clone().unwrap_or_default();
                if msg_text.is_empty() {
                    return Task::none();
                }
                let model_id = if cfg!(feature = "llm") && self.settings.ai_enabled {
                    match self.settings.model_choice {
                        infra::settings::ModelChoice::SmolLM2 => "smollm2-1.7b-instruct-q4_k_m",
                        infra::settings::ModelChoice::Llama1B => "llama-3.2-1b-instruct-q4_k_m",
                        infra::settings::ModelChoice::Qwen15 => "qwen2.5-1.5b-instruct-q4_k_m",
                    }
                } else {
                    "mock"
                };
                if let Some(repo) = self.session_repo.as_ref() {
                    if let Err(e) = repo.save_feedback(trigger, &msg_text, rating, model_id) {
                        log::warn!("save_feedback failed: {e}");
                    }
                }
                self.refresh_setup_caches();
                self.toast = Some(Toast {
                    text: match (rating, self.settings.language) {
                        (1, Language::Es) => "Gracias 👍".to_string(),
                        (1, Language::En) => "Thanks 👍".to_string(),
                        (_, Language::Es) => "Anotado 👎".to_string(),
                        (_, Language::En) => "Noted 👎".to_string(),
                    },
                    expires_at: Instant::now() + Duration::from_secs(2),
                });
                Task::none()
            }
            Message::StartDistilbertDownload => {
                #[cfg(feature = "classifier")]
                {
                    use infra::distilbert_download::download_distilbert;
                    let lang = self.settings.language;
                    let fut = async move {
                        match download_distilbert().await {
                            Ok(()) => Ok(()),
                            Err(e) => Err(format!("{}", e)),
                        }
                    };
                    let _ = lang; // placate compiler if classifier disabled
                    return Task::perform(fut, Message::DistilbertDownloadFinished);
                }
                #[cfg(not(feature = "classifier"))]
                {
                    log::warn!("DistilBERT download requested without classifier feature");
                    Task::none()
                }
            }
            Message::DistilbertDownloadFinished(result) => {
                let msg = match result {
                    Ok(()) => match self.settings.language {
                        Language::Es => "DistilBERT descargado.".to_string(),
                        Language::En => "DistilBERT downloaded.".to_string(),
                    },
                    Err(e) => format!("Error: {}", e),
                };
                log::info!("DistilBERT download → {msg}");
                self.toast = Some(Toast {
                    text: msg,
                    expires_at: Instant::now() + Duration::from_secs(6),
                });
                // Rebuild classifier so Distilbert mode tries the new file.
                if matches!(
                    self.settings.classifier_mode,
                    infra::settings::ClassifierMode::Distilbert
                ) {
                    self.rebuild_classifier();
                }
                Task::none()
            }
            Message::CancelDownload => {
                self.download_cancel.store(true, Ordering::Relaxed);
                log::info!("Download cancellation requested");
                Task::none()
            }
            Message::DeleteModel => {
                #[cfg(feature = "llm")]
                {
                    use infra::model_download::{manifest_for, model_path};
                    if let Some(m) = manifest_for(self.settings.model_choice) {
                        let p = model_path(m);
                        match std::fs::remove_file(&p) {
                            Ok(()) => {
                                log::info!("Deleted model file: {}", p.display());
                                self.coach = build_coach(&self.settings);
                                self.summarizer = build_summarizer(&self.settings);
                                self.refresh_setup_caches();
                                self.toast = Some(Toast {
                                    text: match self.settings.language {
                                        Language::Es => "Modelo eliminado.".to_string(),
                                        Language::En => "Model deleted.".to_string(),
                                    },
                                    expires_at: Instant::now() + Duration::from_secs(3),
                                });
                            }
                            Err(e) => log::warn!("Could not delete model file: {e}"),
                        }
                    }
                }
                Task::none()
            }

            Message::DailyRollCheck => {
                let today = match today_iso_local() {
                    Some(s) => s,
                    None => return Task::none(),
                };
                let last = self.last_summary_date.clone();
                if last.as_deref() == Some(today.as_str()) {
                    return Task::none();
                }
                // Date rollover: reset per-day counters BEFORE summarizing.
                self.distractions_today = 0;
                self.sessions_today = 0;
                self.last_summary_date = Some(today.clone());

                let yesterday = match yesterday_iso_local() {
                    Some(s) => s,
                    None => return Task::none(),
                };

                self.dispatch_summary_for(yesterday)
            }
            Message::DailySummaryReady { date, text } => {
                let text = sanitize_for_display(&text);
                if let Some(repo) = self.session_repo.as_ref() {
                    let model_id = if cfg!(feature = "llm") {
                        match self.settings.model_choice {
                            infra::settings::ModelChoice::SmolLM2 => "smollm2-1.7b-instruct-q4_k_m",
                            infra::settings::ModelChoice::Llama1B => "llama-3.2-1b-instruct-q4_k_m",
                            infra::settings::ModelChoice::Qwen15 => "qwen2.5-1.5b-instruct-q4_k_m",
                        }
                    } else {
                        "canned-v1"
                    };
                    let _ = repo.save_summary(&date, &text, model_id);
                }
                self.recap = Some((date, text));
                Task::none()
            }
            Message::DismissRecap => {
                self.recap = None;
                Task::none()
            }

            Message::ExportJson => {
                let result = match self.session_repo.as_ref() {
                    Some(repo) => crate::infra::export::export_json(repo),
                    None => return Task::none(),
                };
                self.handle_export_result(result);
                Task::none()
            }

            Message::SetMinConfidence(v) => {
                self.settings.min_confidence = v.clamp(0.30, 1.0);
                self.settings.save();
                Task::none()
            }
            Message::SetMinConsecutiveSamples(v) => {
                self.settings.min_consecutive_samples = v.clamp(1, 5);
                self.settings.save();
                Task::none()
            }
            Message::SetPresenceAbsentThreshold(v) => {
                self.settings.presence_absent_threshold = v.clamp(1, 10);
                self.settings.save();
                Task::none()
            }
            Message::SetPhoneConfMin(v) => {
                self.settings.phone_conf_min = v.clamp(0.30, 0.80);
                self.settings.save();
                Task::none()
            }
            Message::SetFaceConfMin(v) => {
                self.settings.face_conf_min = v.clamp(0.40, 0.90);
                self.settings.save();
                // Re-init the presence probe so YuNet picks up the new
                // threshold without app restart.
                #[cfg(feature = "presence")]
                {
                    if self.settings.presence_enabled {
                        self.presence_probe = None;
                        return Task::done(Message::TogglePresence(true));
                    }
                }
                Task::none()
            }
            Message::SetCoachCooldownMins(v) => {
                self.settings.coach_negative_cooldown_mins = v.min(240);
                self.settings.save();
                Task::none()
            }

            Message::TestWindowDetection => {
                let elapsed = self
                    .session_started_at
                    .map(|i| i.elapsed().as_secs() as u32)
                    .unwrap_or(0);
                let sample_opt = WindowWatcher::poll(elapsed);
                if let Some(sample) = sample_opt {
                    let proc = sample.process_name.clone();
                    let fut = self.classifier.classify(&sample);
                    return Task::perform(fut, move |r| match r {
                        Ok(c) => Message::WindowTestReady(Some((
                            proc.clone(),
                            c.matched_rule.clone(),
                            c.confidence,
                            c.label,
                        ))),
                        Err(_) => Message::WindowTestReady(None),
                    });
                }
                Task::done(Message::WindowTestReady(None))
            }
            Message::WindowTestReady(opt) => {
                self.last_window_test = opt;
                Task::none()
            }

            #[cfg(feature = "presence")]
            Message::TestFaceDetection => {
                if let Some(probe) = self.presence_probe.as_ref() {
                    let probe = probe.clone();
                    if let Some(engine) = probe.yunet_engine() {
                        match probe.poll() {
                            Ok((_sample, captured)) => {
                                let bytes = captured.bytes;
                                let (w, h) = (captured.width, captured.height);
                                return Task::perform(
                                    async move {
                                        tokio::task::spawn_blocking(move || {
                                            let mut g = engine
                                                .lock()
                                                .map_err(|_| "yunet poisoned".to_string())?;
                                            g.infer(&bytes, w, h)
                                        })
                                        .await
                                        .map_err(|e| e.to_string())
                                        .and_then(|r| r)
                                    },
                                    Message::FaceTestReady,
                                );
                            }
                            Err(e) => return Task::done(Message::FaceTestReady(Err(e.to_string()))),
                        }
                    }
                    return Task::done(Message::FaceTestReady(Err("YuNet no descargado".to_string())));
                }
                Task::done(Message::FaceTestReady(Err(
                    "Activa la cámara primero".to_string(),
                )))
            }
            #[cfg(feature = "presence")]
            Message::FaceTestReady(result) => {
                self.last_face_test = result.ok();
                Task::none()
            }

            #[cfg(feature = "presence")]
            Message::TestPhoneDetection => {
                if let Some(probe) = self.presence_probe.as_ref() {
                    let probe = probe.clone();
                    if let Some(engine) = probe.yolo_engine() {
                        match probe.poll() {
                            Ok((_sample, captured)) => {
                                let bytes = captured.bytes;
                                let (w, h) = (captured.width, captured.height);
                                return Task::perform(
                                    async move {
                                        tokio::task::spawn_blocking(move || {
                                            let mut g = engine
                                                .lock()
                                                .map_err(|_| "yolo poisoned".to_string())?;
                                            g.infer(&bytes, w, h)
                                        })
                                        .await
                                        .map_err(|e| e.to_string())
                                        .and_then(|r| r)
                                    },
                                    Message::PhoneTestReady,
                                );
                            }
                            Err(e) => return Task::done(Message::PhoneTestReady(Err(e.to_string()))),
                        }
                    }
                    return Task::done(Message::PhoneTestReady(Err(
                        "YOLOv8n no descargado".to_string(),
                    )));
                }
                Task::done(Message::PhoneTestReady(Err(
                    "Activa la cámara primero".to_string(),
                )))
            }
            #[cfg(feature = "presence")]
            Message::PhoneTestReady(result) => {
                self.last_phone_test = result.ok();
                Task::none()
            }

            Message::StartCalibrationWizard => {
                self.calibration_wizard = Some(crate::app::state::CalibrationWizardState::default());
                Task::none()
            }
            Message::CalibrationWizardCancel => {
                self.calibration_wizard = None;
                Task::none()
            }
            #[cfg(not(feature = "presence"))]
            Message::CalibrationCapture => Task::none(),
            #[cfg(feature = "presence")]
            Message::CalibrationCapture => {
                // The capture loop runs on the UI thread (one frame per
                // tick) because PresenceProbe wraps a non-Send Camera.
                // Each frame: poll on UI (~5ms), inference in
                // spawn_blocking (~200ms), then schedule the next.
                use crate::app::state::CalibrationStage;
                let stage = match self.calibration_wizard.as_ref().map(|w| w.stage) {
                    Some(s) => s,
                    None => return Task::none(),
                };
                // v1.13.0 — "Empezar" on the Welcome stage just
                // transitions to FaceWith and waits for the user to
                // click Capturar there. Without this the click did
                // nothing because the capture path below only handles
                // the four data stages.
                if matches!(stage, CalibrationStage::Welcome) {
                    if let Some(w) = self.calibration_wizard.as_mut() {
                        w.stage = CalibrationStage::FaceWith;
                    }
                    return Task::none();
                }
                let want_face = matches!(
                    stage,
                    CalibrationStage::FaceWith | CalibrationStage::FaceWithout
                );
                let want_phone = matches!(
                    stage,
                    CalibrationStage::PhoneWith | CalibrationStage::PhoneWithout
                );
                if !want_face && !want_phone {
                    return Task::none();
                }
                let probe = match self.presence_probe.as_ref() {
                    Some(p) => p.clone(),
                    None => {
                        self.toast = Some(Toast {
                            text: match self.settings.language {
                                Language::Es => "Activa la cámara primero (Setup → IA).".to_string(),
                                Language::En => "Enable the camera first (Setup → IA).".to_string(),
                            },
                            expires_at: Instant::now() + Duration::from_secs(4),
                        });
                        return Task::none();
                    }
                };
                // Only reset the bucket on the first click of the batch
                // (capturing == false). Subsequent recursive entries via
                // CalibrationFrameReady → CalibrationCapture must keep
                // their accumulated frames, otherwise the batch never
                // reaches the 10-frame target and the UI stays stuck on
                // "Capturando…".
                let already_capturing = self
                    .calibration_wizard
                    .as_ref()
                    .map(|w| w.capturing)
                    .unwrap_or(false);
                if let Some(w) = self.calibration_wizard.as_mut() {
                    if !already_capturing {
                        // Fresh batch (user click): clear bucket AND
                        // dismiss any stale warning from the previous
                        // batch so the UI flips to "Capturando…"
                        // immediately. Without this clear the primary
                        // button stays as "Reintentar este paso" while
                        // the new capture runs silently underneath, and
                        // each extra click spawns a parallel batch.
                        w.stage_warning = None;
                        match stage {
                            CalibrationStage::FaceWith => w.face_with.clear(),
                            CalibrationStage::FaceWithout => w.face_without.clear(),
                            CalibrationStage::PhoneWith => w.phone_with.clear(),
                            CalibrationStage::PhoneWithout => w.phone_without.clear(),
                            _ => {}
                        }
                    }
                    w.capturing = true;
                }
                // Capture frame 1 of 10 right now; the rest schedule
                // themselves via CalibrationFrameReady → CalibrationCaptureNext.
                let captured_opt = probe.poll().ok().map(|(_, c)| c);
                let captured = match captured_opt {
                    Some(c) => c,
                    None => return Task::done(Message::CalibrationFrameReady(stage, 0.0)),
                };
                let bytes = captured.bytes;
                let (w, h) = (captured.width, captured.height);
                if want_face {
                    if let Some(engine) = probe.yunet_engine() {
                        return Task::perform(
                            async move {
                                let res = tokio::task::spawn_blocking(move || {
                                    engine
                                        .lock()
                                        .ok()
                                        .and_then(|mut g| g.infer(&bytes, w, h).ok().map(|(_, s)| s))
                                })
                                .await
                                .ok()
                                .flatten()
                                .unwrap_or(0.0);
                                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                                res
                            },
                            move |s| Message::CalibrationFrameReady(stage, s),
                        );
                    }
                } else if let Some(engine) = probe.yolo_engine() {
                    return Task::perform(
                        async move {
                            let res = tokio::task::spawn_blocking(move || {
                                engine
                                    .lock()
                                    .ok()
                                    .and_then(|mut g| g.infer(&bytes, w, h).ok())
                            })
                            .await
                            .ok()
                            .flatten()
                            .unwrap_or(0.0);
                            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                            res
                        },
                        move |s| Message::CalibrationFrameReady(stage, s),
                    );
                }
                Task::done(Message::CalibrationFrameReady(stage, 0.0))
            }
            #[cfg(feature = "presence")]
            Message::CalibrationFrameReady(stage, score) => {
                use crate::app::state::CalibrationStage;
                const TARGET: usize = 10;
                let mut done_n = 0;
                if let Some(w) = self.calibration_wizard.as_mut() {
                    let bucket = match stage {
                        CalibrationStage::FaceWith => &mut w.face_with,
                        CalibrationStage::FaceWithout => &mut w.face_without,
                        CalibrationStage::PhoneWith => &mut w.phone_with,
                        CalibrationStage::PhoneWithout => &mut w.phone_without,
                        _ => return Task::none(),
                    };
                    bucket.push(score);
                    done_n = bucket.len();
                }
                if done_n < TARGET {
                    // Still need more frames — fire the next capture.
                    return Task::done(Message::CalibrationCapture);
                }
                // 10 frames collected — wrap up this stage.
                let scores: Vec<f32> = match self.calibration_wizard.as_ref() {
                    Some(w) => match stage {
                        CalibrationStage::FaceWith => w.face_with.clone(),
                        CalibrationStage::FaceWithout => w.face_without.clone(),
                        CalibrationStage::PhoneWith => w.phone_with.clone(),
                        CalibrationStage::PhoneWithout => w.phone_without.clone(),
                        _ => return Task::none(),
                    },
                    None => return Task::none(),
                };
                Task::done(Message::CalibrationBatchReady(stage, scores))
            }
            #[cfg(feature = "presence")]
            Message::CalibrationBatchReady(stage, scores) => {
                use crate::app::state::CalibrationStage;
                // v1.13.0 — proactive per-stage diagnostics. Compute
                // statistics on the just-arrived batch and decide
                // whether to (a) advance silently, (b) advance with a
                // warning the Summary will surface, or (c) PAUSE on
                // this stage and ask the user to retry this step
                // specifically.
                let mean: f32 = if scores.is_empty() {
                    0.0
                } else {
                    scores.iter().sum::<f32>() / scores.len() as f32
                };
                let max: f32 = scores.iter().copied().fold(f32::NEG_INFINITY, f32::max);
                let warning: Option<String> = match stage {
                    CalibrationStage::FaceWith => {
                        if max < 0.10 {
                            Some(match self.settings.language {
                                Language::Es => format!(
                                    "🚨 Casi no detecté tu cara (max={:.2}). Verifica que estás frente a la cámara, que hay buena luz, y que la cámara no está bloqueada. Pulsa Reintentar este paso o Continuar de todos modos.",
                                    max
                                ),
                                Language::En => format!(
                                    "🚨 Barely detected your face (max={:.2}). Make sure you're in frame, lighting is decent, and the camera isn't blocked. Press Retry this step or Continue anyway.",
                                    max
                                ),
                            })
                        } else if mean < 0.20 {
                            Some(match self.settings.language {
                                Language::Es => format!(
                                    "⚠ Detección débil (media={:.2}). Para mejor calibración: acerca la cámara a tu cara o mejora la iluminación. ¿Reintentar?",
                                    mean
                                ),
                                Language::En => format!(
                                    "⚠ Weak detection (mean={:.2}). For better calibration: move closer or improve lighting. Retry?",
                                    mean
                                ),
                            })
                        } else {
                            None
                        }
                    }
                    CalibrationStage::FaceWithout => {
                        // Cross-validation: compare against the previous
                        // FaceWith batch. Catches the "low light + dark
                        // background" failure mode where with/without
                        // distributions overlap heavily.
                        let prev_with = self
                            .calibration_wizard
                            .as_ref()
                            .map(|w| w.face_with.clone())
                            .unwrap_or_default();
                        let mean_with: f32 = if prev_with.is_empty() {
                            0.0
                        } else {
                            prev_with.iter().sum::<f32>() / prev_with.len() as f32
                        };
                        let max_without = scores.iter().copied().fold(f32::NEG_INFINITY, f32::max);
                        if mean > 0.30 {
                            Some(match self.settings.language {
                                Language::Es => format!(
                                    "⚠ Aún detecto algo (media={:.2}) cuando se supone que no hay cara. Asegúrate de cubrir la cámara o salir del cuadro completamente. ¿Reintentar?",
                                    mean
                                ),
                                Language::En => format!(
                                    "⚠ Still detecting something (mean={:.2}) when there should be no face. Make sure you cover the camera or fully step out of frame. Retry?",
                                    mean
                                ),
                            })
                        } else if max_without >= mean_with && mean_with > 0.0 {
                            // Overlap detected: noise without face is as
                            // strong as signal with face → no threshold
                            // can separate them.
                            Some(match self.settings.language {
                                Language::Es => format!(
                                    "🚨 Solapamiento total detectado: el modelo ve 'caras' fantasma (max sin cara = {:.2}) tan fuertes como cuando estás presente (media con cara = {:.2}). Causa probable: poca luz o fondo ruidoso. Mejora la iluminación frontal y reintenta el paso. O Continúa de todos modos para ver el resumen.",
                                    max_without, mean_with
                                ),
                                Language::En => format!(
                                    "🚨 Total overlap detected: the model sees ghost 'faces' (max without face = {:.2}) as strongly as your real face (mean with face = {:.2}). Likely cause: low light or noisy background. Improve frontal lighting and retry this step. Or Continue anyway to see the summary.",
                                    max_without, mean_with
                                ),
                            })
                        } else if mean_with > 0.0 && mean_with < 2.5 * mean.max(0.01) {
                            // Weak ratio between with and without.
                            Some(match self.settings.language {
                                Language::Es => format!(
                                    "⚠ Diferencia débil entre con/sin cara (con={:.2} vs sin={:.2}). El umbral resultante será frágil. Considera mejorar la iluminación y reintentar.",
                                    mean_with, mean
                                ),
                                Language::En => format!(
                                    "⚠ Weak ratio between with/without face (with={:.2} vs without={:.2}). The resulting threshold will be fragile. Consider improving lighting and retrying.",
                                    mean_with, mean
                                ),
                            })
                        } else {
                            None
                        }
                    }
                    CalibrationStage::PhoneWith => {
                        if max < 0.10 {
                            Some(match self.settings.language {
                                Language::Es => format!(
                                    "🚨 No detecté tu celular (max={:.2}). Levántalo bien al frente de la cámara, asegúrate que ocupe parte significativa del cuadro, y que esté bien iluminado. ¿Reintentar?",
                                    max
                                ),
                                Language::En => format!(
                                    "🚨 Couldn't detect your phone (max={:.2}). Hold it clearly in front of the camera, make sure it fills a meaningful part of the frame, and is well-lit. Retry?",
                                    max
                                ),
                            })
                        } else if mean < 0.20 {
                            Some(match self.settings.language {
                                Language::Es => format!(
                                    "⚠ Detección débil del celular (media={:.2}). Acércalo más o ajusta el ángulo. ¿Reintentar?",
                                    mean
                                ),
                                Language::En => format!(
                                    "⚠ Weak phone detection (mean={:.2}). Hold it closer or adjust the angle. Retry?",
                                    mean
                                ),
                            })
                        } else {
                            None
                        }
                    }
                    CalibrationStage::PhoneWithout => {
                        let prev_with = self
                            .calibration_wizard
                            .as_ref()
                            .map(|w| w.phone_with.clone())
                            .unwrap_or_default();
                        let mean_with: f32 = if prev_with.is_empty() {
                            0.0
                        } else {
                            prev_with.iter().sum::<f32>() / prev_with.len() as f32
                        };
                        let max_without = scores.iter().copied().fold(f32::NEG_INFINITY, f32::max);
                        if mean > 0.30 {
                            Some(match self.settings.language {
                                Language::Es => format!(
                                    "⚠ Aún detecto un objeto similar a celular (media={:.2}). Quita cualquier teléfono u objeto rectangular del cuadro. ¿Reintentar?",
                                    mean
                                ),
                                Language::En => format!(
                                    "⚠ Still detecting something phone-like (mean={:.2}). Remove any phone or rectangular object from the frame. Retry?",
                                    mean
                                ),
                            })
                        } else if max_without >= mean_with && mean_with > 0.0 {
                            Some(match self.settings.language {
                                Language::Es => format!(
                                    "🚨 Solapamiento total: el modelo confunde el fondo con un celular (max sin = {:.2}) tan fuerte como cuando lo muestras (media con = {:.2}). Probable causa: poca luz o el celular era pequeño en el cuadro. Acércalo más y mejora la iluminación.",
                                    max_without, mean_with
                                ),
                                Language::En => format!(
                                    "🚨 Total overlap: the model confuses the background with a phone (max without = {:.2}) as strongly as when you showed it (mean with = {:.2}). Likely cause: low light or phone too small in frame. Hold it closer and improve lighting.",
                                    max_without, mean_with
                                ),
                            })
                        } else if mean_with > 0.0 && mean_with < 2.5 * mean.max(0.01) {
                            Some(match self.settings.language {
                                Language::Es => format!(
                                    "⚠ Diferencia débil entre con/sin celular (con={:.2} vs sin={:.2}). El umbral resultante será frágil. Considera acercar el celular o mejorar la luz.",
                                    mean_with, mean
                                ),
                                Language::En => format!(
                                    "⚠ Weak ratio between with/without phone (with={:.2} vs without={:.2}). The resulting threshold will be fragile. Hold the phone closer or improve lighting.",
                                    mean_with, mean
                                ),
                            })
                        } else {
                            None
                        }
                    }
                    _ => None,
                };

                if let Some(w) = self.calibration_wizard.as_mut() {
                    w.capturing = false;
                    // Always store the scores even if we pause for warning —
                    // the user might choose to continue with imperfect data.
                    match stage {
                        CalibrationStage::FaceWith => w.face_with = scores.clone(),
                        CalibrationStage::FaceWithout => w.face_without = scores.clone(),
                        CalibrationStage::PhoneWith => w.phone_with = scores.clone(),
                        CalibrationStage::PhoneWithout => w.phone_without = scores.clone(),
                        _ => {}
                    }
                    if let Some(msg) = warning {
                        // PAUSE on this stage; user picks Reintentar or Continuar.
                        w.stage_warning = Some(msg);
                        return Task::none();
                    }
                    w.stage_warning = None;
                    match stage {
                        CalibrationStage::FaceWith => {
                            w.stage = CalibrationStage::FaceWithout;
                        }
                        CalibrationStage::FaceWithout => {
                            w.stage = CalibrationStage::PhoneWith;
                        }
                        CalibrationStage::PhoneWith => {
                            w.stage = CalibrationStage::PhoneWithout;
                        }
                        CalibrationStage::PhoneWithout => {
                            // scores already stored above; compute outcomes.
                            let face = compute_suggested_outcome(
                                &w.face_with,
                                &w.face_without,
                            );
                            let phone = compute_suggested_outcome(
                                &w.phone_with,
                                &w.phone_without,
                            );
                            let q_label = |q| match q {
                                CalibrationQuality::Strong => "strong".to_string(),
                                CalibrationQuality::Marginal => "marginal".to_string(),
                                CalibrationQuality::Unusable => "unusable".to_string(),
                                CalibrationQuality::Insufficient => "insufficient".to_string(),
                            };
                            w.suggested_face = face.threshold;
                            w.face_marginal =
                                matches!(face.quality, CalibrationQuality::Marginal);
                            w.face_quality = Some((
                                q_label(face.quality),
                                (face.error_rate * 100.0).round() as u32,
                                face.overlap,
                                face.mean_with,
                                face.mean_without,
                            ));
                            w.suggested_phone = phone.threshold;
                            w.phone_marginal =
                                matches!(phone.quality, CalibrationQuality::Marginal);
                            w.phone_quality = Some((
                                q_label(phone.quality),
                                (phone.error_rate * 100.0).round() as u32,
                                phone.overlap,
                                phone.mean_with,
                                phone.mean_without,
                            ));
                            w.stage = CalibrationStage::Summary;
                        }
                        _ => {}
                    }
                }
                Task::none()
            }
            Message::CalibrationContinueAnyway => {
                // Clear the warning and force-advance to the next stage,
                // re-using the same dispatch logic as CalibrationBatchReady.
                use crate::app::state::CalibrationStage;
                if let Some(w) = self.calibration_wizard.as_mut() {
                    w.stage_warning = None;
                    w.stage = match w.stage {
                        CalibrationStage::FaceWith => CalibrationStage::FaceWithout,
                        CalibrationStage::FaceWithout => CalibrationStage::PhoneWith,
                        CalibrationStage::PhoneWith => CalibrationStage::PhoneWithout,
                        CalibrationStage::PhoneWithout => CalibrationStage::Summary,
                        other => other,
                    };
                    if matches!(w.stage, CalibrationStage::Summary) {
                        let face = compute_suggested_outcome(
                            &w.face_with,
                            &w.face_without,
                        );
                        let phone = compute_suggested_outcome(
                            &w.phone_with,
                            &w.phone_without,
                        );
                        let q_label = |q| match q {
                            CalibrationQuality::Strong => "strong".to_string(),
                            CalibrationQuality::Marginal => "marginal".to_string(),
                            CalibrationQuality::Unusable => "unusable".to_string(),
                            CalibrationQuality::Insufficient => "insufficient".to_string(),
                        };
                        w.suggested_face = face.threshold;
                        w.face_marginal = matches!(face.quality, CalibrationQuality::Marginal);
                        w.face_quality = Some((
                            q_label(face.quality),
                            (face.error_rate * 100.0).round() as u32,
                            face.overlap,
                            face.mean_with,
                            face.mean_without,
                        ));
                        w.suggested_phone = phone.threshold;
                        w.phone_marginal = matches!(phone.quality, CalibrationQuality::Marginal);
                        w.phone_quality = Some((
                            q_label(phone.quality),
                            (phone.error_rate * 100.0).round() as u32,
                            phone.overlap,
                            phone.mean_with,
                            phone.mean_without,
                        ));
                    }
                }
                Task::none()
            }
            Message::CalibrationApply => {
                // v1.13.0 — only apply per-detector if quality is NOT
                // 'unusable'. A high error rate or zero-mean detection
                // means the threshold would brick the feature (e.g.
                // YOLO=0.00 → every frame fires phone-detected). Skip
                // those silently and toast only what was applied.
                let (face, phone, face_q, phone_q) = match self.calibration_wizard.as_ref() {
                    Some(w) => (
                        w.suggested_face,
                        w.suggested_phone,
                        w.face_quality.clone(),
                        w.phone_quality.clone(),
                    ),
                    None => return Task::none(),
                };
                let mut applied = Vec::new();
                let mut skipped = Vec::new();
                if let Some(v) = face {
                    let unusable = face_q.as_ref().map(|(q, ..)| q == "unusable").unwrap_or(false);
                    if unusable {
                        skipped.push("YuNet".to_string());
                    } else if v < 0.05 {
                        // Floor: anything below 0.05 effectively means
                        // "always present" — refuse silently.
                        skipped.push("YuNet".to_string());
                    } else {
                        self.settings.face_conf_min = v.clamp(0.40, 0.90);
                        applied.push(format!("YuNet → {:.2}", v));
                    }
                }
                if let Some(v) = phone {
                    let unusable = phone_q.as_ref().map(|(q, ..)| q == "unusable").unwrap_or(false);
                    if unusable {
                        skipped.push("YOLO".to_string());
                    } else if v < 0.05 {
                        // Floor: a phone threshold near zero would
                        // auto-pause the session every tick.
                        skipped.push("YOLO".to_string());
                    } else {
                        self.settings.phone_conf_min = v.clamp(0.30, 0.80);
                        applied.push(format!("YOLO → {:.2}", v));
                    }
                }
                self.settings.save();
                self.calibration_wizard = None;
                let toast_text = if applied.is_empty() {
                    match self.settings.language {
                        Language::Es =>
                            "Sin separación suficiente. Defaults preservados.".to_string(),
                        Language::En =>
                            "Insufficient separation. Defaults kept.".to_string(),
                    }
                } else {
                    match self.settings.language {
                        Language::Es =>
                            format!("Aplicado: {}", applied.join(" · ")),
                        Language::En => format!("Applied: {}", applied.join(" · ")),
                    }
                };
                self.toast = Some(Toast {
                    text: toast_text,
                    expires_at: Instant::now() + Duration::from_secs(6),
                });
                #[cfg(feature = "presence")]
                {
                    if self.settings.presence_enabled {
                        self.presence_probe = None;
                        return Task::done(Message::TogglePresence(true));
                    }
                }
                Task::none()
            }

            Message::MarkLastDistractionAsFalsePositive => {
                let process: Option<String> = self
                    .session_repo
                    .as_ref()
                    .and_then(|r| r.recent_distraction_process().ok().flatten());
                let proc_name = match process {
                    Some(p) => p,
                    None => {
                        self.toast = Some(Toast {
                            text: match self.settings.language {
                                Language::Es => "Sin distracciones recientes para marcar.".to_string(),
                                Language::En => "No recent distractions to mark.".to_string(),
                            },
                            expires_at: Instant::now() + Duration::from_secs(4),
                        });
                        return Task::none();
                    }
                };
                let result =
                    crate::infra::plugins::append_to_user_exceptions(&proc_name);
                let toast_text = match (&result, self.settings.language) {
                    (Ok(_), Language::Es) => format!("'{proc_name}' añadido a excepciones."),
                    (Ok(_), Language::En) => format!("'{proc_name}' added to exceptions."),
                    (Err(e), Language::Es) => format!("Error: {e}"),
                    (Err(e), Language::En) => format!("Error: {e}"),
                };
                self.toast = Some(Toast {
                    text: toast_text,
                    expires_at: Instant::now() + Duration::from_secs(5),
                });
                self.plugins =
                    crate::infra::plugins::scan(&self.settings.plugin_overrides);
                self.classifier =
                    crate::app::builders::build_classifier_with_plugins(
                        &self.settings,
                        &self.plugins,
                    );
                Task::none()
            }

            Message::SetMinAttention(value) => {
                let clamped = value.min(100);
                self.settings.min_attention_for_valid_session = clamped;
                self.settings.save();
                Task::none()
            }

            Message::TogglePlugin(id, enabled) => {
                self.settings
                    .plugin_overrides
                    .insert(id.clone(), enabled);
                self.settings.save();
                if let Some(p) = self.plugins.iter_mut().find(|p| p.id == id) {
                    p.enabled = enabled;
                }
                // Rebuild classifier so the rules table reflects the
                // change immediately (no app restart needed).
                self.classifier = crate::app::builders::build_classifier_with_plugins(
                    &self.settings,
                    &self.plugins,
                );
                Task::none()
            }

            Message::ReloadPlugins => {
                self.plugins =
                    crate::infra::plugins::scan(&self.settings.plugin_overrides);
                self.classifier = crate::app::builders::build_classifier_with_plugins(
                    &self.settings,
                    &self.plugins,
                );
                let count = self.plugins.len();
                self.toast = Some(Toast {
                    text: match self.settings.language {
                        Language::Es => format!("{count} plugin(s) recargados."),
                        Language::En => format!("{count} plugin(s) reloaded."),
                    },
                    expires_at: Instant::now() + Duration::from_secs(4),
                });
                Task::none()
            }

            Message::ToggleDeepMode(enabled) => {
                self.settings.deep_mode_enabled = enabled;
                self.settings.save();
                let text = match (enabled, self.settings.language) {
                    (true, Language::Es) =>
                        "Modo profundo activado: las sesiones se encadenan sin descanso.",
                    (true, Language::En) =>
                        "Deep mode on: sessions chain back-to-back without breaks.",
                    (false, Language::Es) => "Modo profundo desactivado.",
                    (false, Language::En) => "Deep mode off.",
                };
                self.toast = Some(Toast {
                    text: text.to_string(),
                    expires_at: Instant::now() + Duration::from_secs(4),
                });
                Task::none()
            }

            Message::ExportCsv => {
                let result = match self.session_repo.as_ref() {
                    Some(repo) => crate::infra::export::export_csv(repo),
                    None => return Task::none(),
                };
                self.handle_export_result(result);
                Task::none()
            }
        }
    }

    /// v1.13.0 — true if the user voted 👎 on a coach message within
    /// `settings.coach_negative_cooldown_mins`. While true, the next
    /// coach call bypasses the LLM and pulls from the curated bank.
    fn is_coach_in_cooldown(&self) -> bool {
        let mins = self.settings.coach_negative_cooldown_mins;
        if mins == 0 {
            return false;
        }
        self.session_repo
            .as_ref()
            .and_then(|r| r.recent_negative_feedback_within(mins).ok())
            .unwrap_or(false)
    }

    /// v1.12.2 — common export feedback. Saves the path on App so the
    /// Privacy card can show it inline (the toast only renders on the
    /// Focus canvas), reveals the file in macOS Finder so the user
    /// gets immediate confirmation, and still sets the toast for the
    /// case where the user returns to Focus afterwards.
    fn handle_export_result(
        &mut self,
        result: Result<std::path::PathBuf, crate::infra::export::ExportError>,
    ) {
        match result {
            Ok(path) => {
                log::info!("Export written: {}", path.display());
                self.last_export_path = Some(path.clone());
                self.last_export_error = None;
                // v2.0.0 — cross-platform reveal (Finder / Explorer / xdg-open).
                crate::infra::reveal::reveal(&path);
                let toast_text = match self.settings.language {
                    Language::Es => format!("Exportado: {}", path.display()),
                    Language::En => format!("Exported: {}", path.display()),
                };
                self.toast = Some(Toast {
                    text: toast_text,
                    expires_at: Instant::now() + Duration::from_secs(6),
                });
            }
            Err(e) => {
                log::warn!("Export failed: {e}");
                let msg = e.to_string();
                self.last_export_path = None;
                self.last_export_error = Some(msg.clone());
                let toast_text = match self.settings.language {
                    Language::Es => format!("Error al exportar: {msg}"),
                    Language::En => format!("Export error: {msg}"),
                };
                self.toast = Some(Toast {
                    text: toast_text,
                    expires_at: Instant::now() + Duration::from_secs(6),
                });
            }
        }
    }
}

/// v1.13.0 — calibration analysis output. Reports the optimal
/// threshold (minimising classification error on the captured
/// samples), the expected error rate at that threshold, and a
/// quality classification so the UI can render an actionable
/// recommendation instead of a vague "marginal" badge.
#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(dead_code)]
pub enum CalibrationQuality {
    /// Distributions cleanly separated; suggested threshold is
    /// confident.
    Strong,
    /// Distributions overlap a little; threshold has measurable
    /// error rate but is still useful.
    Marginal,
    /// Distributions overlap heavily; no threshold can reliably
    /// separate them. Don't apply — recommend disabling presence
    /// detection or repositioning.
    Unusable,
    /// Not enough data (< 3 samples in either set).
    Insufficient,
}

#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub struct CalibrationOutcome {
    pub threshold: Option<f32>,
    pub quality: CalibrationQuality,
    /// Expected misclassification rate at the chosen threshold,
    /// computed against the captured samples themselves. 0..1.
    pub error_rate: f32,
    /// True if max(without) >= min(with) — i.e. the two clouds
    /// overlap, which is the dominant failure mode for the user's
    /// camera angle / lighting / model fit.
    pub overlap: bool,
    pub mean_with: f32,
    pub mean_without: f32,
}

/// v1.13.0 — find the threshold on `[0..1]` that minimises the
/// total classification error against the captured samples:
/// `false_positives = |without ≥ T|` (claims presence when absent)
/// + `false_negatives = |with < T|` (claims absent when present).
/// Returns the best `(threshold, error_rate)` pair.
fn scan_optimal_threshold(with: &[f32], without: &[f32]) -> (f32, f32) {
    // Build a sorted list of candidate thresholds: every observed
    // score acts as a boundary, plus 0.0 and 1.0.
    let mut candidates: Vec<f32> = with.iter().copied().chain(without.iter().copied()).collect();
    candidates.push(0.0);
    candidates.push(1.0);
    candidates.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    candidates.dedup_by(|a, b| (*a - *b).abs() < 1e-4);

    let n = (with.len() + without.len()) as f32;
    let mut best_t = 0.5_f32;
    let mut best_err = f32::INFINITY;

    for t in &candidates {
        let fp = without.iter().filter(|&&s| s >= *t).count() as f32;
        let fn_ = with.iter().filter(|&&s| s < *t).count() as f32;
        let err = (fp + fn_) / n;
        if err < best_err {
            best_err = err;
            best_t = *t;
        }
    }
    (best_t.clamp(0.0, 1.0), best_err)
}

/// v1.13.0 — derive a suggested threshold + quality verdict from two
/// contrastive score samples. Replaces the v1.13.0-pre naive midpoint
/// approach with an error-minimisation scan that produces honest
/// quality + error-rate readouts the UI can render.
#[allow(dead_code)]
pub(crate) fn compute_suggested_outcome(
    with: &[f32],
    without: &[f32],
) -> CalibrationOutcome {
    if with.len() < 3 || without.len() < 3 {
        return CalibrationOutcome {
            threshold: None,
            quality: CalibrationQuality::Insufficient,
            error_rate: 0.0,
            overlap: false,
            mean_with: 0.0,
            mean_without: 0.0,
        };
    }
    let mean = |xs: &[f32]| xs.iter().sum::<f32>() / xs.len() as f32;
    let m_with = mean(with);
    let m_without = mean(without);
    let max_without = without.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let min_with = with.iter().copied().fold(f32::INFINITY, f32::min);
    let overlap = max_without >= min_with;

    let (t, err) = scan_optimal_threshold(with, without);

    let quality = if err <= 0.05 {
        CalibrationQuality::Strong
    } else if err <= 0.20 {
        CalibrationQuality::Marginal
    } else {
        CalibrationQuality::Unusable
    };

    let threshold = match quality {
        CalibrationQuality::Strong | CalibrationQuality::Marginal => Some(t),
        CalibrationQuality::Unusable | CalibrationQuality::Insufficient => None,
    };

    CalibrationOutcome {
        threshold,
        quality,
        error_rate: err,
        overlap,
        mean_with: m_with,
        mean_without: m_without,
    }
}

/// v1.13.0 (legacy) — kept for compile compatibility with older call
/// sites. Returns `(Option<threshold>, marginal_flag)`.
#[allow(dead_code)]
pub(crate) fn compute_suggested(with: &[f32], without: &[f32]) -> (Option<f32>, bool) {
    let outcome = compute_suggested_outcome(with, without);
    let marginal = matches!(outcome.quality, CalibrationQuality::Marginal);
    (outcome.threshold, marginal)
}
