//! v1.7.0 — App lifecycle methods: title (window title), focus_context
//! (assembled from settings + repo + engine state), subscription
//! (timer + window probe + presence + calendar + toast + download +
//! daily-roll + keyboard).

use std::sync::atomic::Ordering;
use std::time::Duration;

use iced::Subscription;
use solar_focus_intelligence::FocusContext;

use crate::ui::sidebar::Route;
use crate::{App, Message, SolarFocusCore};

impl App {
    pub fn title(&self) -> String {
        match self.pomodoro_engine.state() {
            SolarFocusCore::AppState::Idle => "SolarFocus OS - Esperando...".to_string(),
            SolarFocusCore::AppState::Focusing(_) => "SolarFocus OS - En Foco".to_string(),
            SolarFocusCore::AppState::Break => "SolarFocus OS - Descanso".to_string(),
            SolarFocusCore::AppState::Completed => "SolarFocus OS - Completado".to_string(),
        }
    }

    pub(crate) fn focus_context(&self) -> FocusContext {
        use chrono::{Datelike, Local, Timelike};
        let now = Local::now();
        let weekday = now.weekday().num_days_from_monday() as u8;
        let focus_minutes_7d = self
            .session_repo
            .as_ref()
            .and_then(|r| r.weekly_focus_seconds().ok())
            .map(|days| days.iter().map(|(_, s)| *s).sum::<u32>() / 60)
            .unwrap_or(0);
        let last_distraction = self
            .last_classification
            .as_ref()
            .and_then(|c| c.matched_rule.clone())
            .and_then(|rule| rule.split(':').nth(1).map(|s| s.to_string()));

        FocusContext {
            sessions_today: self.sessions_today,
            streak: self.pomodoro_engine.sessions_completed(),
            xp_today: 0,
            focus_duration_secs: self.pomodoro_engine.config().focus_duration as u32,
            language: self.settings.language,
            hour_of_day: now.hour() as u8,
            weekday,
            distractions_today: self.distractions_today,
            focus_minutes_7d,
            last_distraction,
            category: Some(self.settings.last_category.clone()),
        }
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let mut subs: Vec<Subscription<Message>> = Vec::new();

        if !self.pomodoro_engine.is_paused()
            && matches!(
                self.pomodoro_engine.state(),
                SolarFocusCore::AppState::Focusing(_) | SolarFocusCore::AppState::Break
            )
        {
            subs.push(
                iced::time::every(Duration::from_millis(100))
                    .map(|_| Message::TimerTick(0.1)),
            );
        }

        if self.settings.window_watch_enabled
            && matches!(
                self.pomodoro_engine.state(),
                SolarFocusCore::AppState::Focusing(_)
            )
        {
            let secs = self.settings.window_poll_secs.max(1) as u64;
            subs.push(
                iced::time::every(Duration::from_secs(secs)).map(|_| Message::WindowProbe),
            );
        }

        #[cfg(feature = "presence")]
        if self.settings.presence_enabled
            && matches!(
                self.pomodoro_engine.state(),
                SolarFocusCore::AppState::Focusing(_)
            )
        {
            subs.push(
                iced::time::every(Duration::from_secs(1)).map(|_| Message::PresenceProbe),
            );
        }

        #[cfg(feature = "calendar")]
        if self.settings.calendar_live_enabled && self.calendar_reader.is_some() {
            subs.push(
                iced::time::every(Duration::from_secs(60)).map(|_| Message::CalendarRefresh),
            );
        }

        if self.toast.is_some() {
            subs.push(iced::time::every(Duration::from_secs(1)).map(|_| Message::ToastTick));
        }

        if self.download_active.load(Ordering::Relaxed) {
            subs.push(
                iced::time::every(Duration::from_millis(250)).map(|_| Message::DownloadPoll),
            );
        }

        subs.push(iced::time::every(Duration::from_secs(60)).map(|_| Message::DailyRollCheck));

        subs.push(iced::keyboard::on_key_press(|key, _modifiers| {
            use iced::keyboard::key::{Key, Named};
            match key.as_ref() {
                Key::Named(Named::Space) => Some(Message::Pause),
                Key::Named(Named::Escape) => Some(Message::EndSession),
                Key::Character("r") | Key::Character("R") => Some(Message::Resume),
                Key::Character("p") | Key::Character("P") => Some(Message::Pause),
                Key::Character("b") | Key::Character("B") => Some(Message::TakeBreak),
                Key::Character("s") | Key::Character("S") => {
                    Some(Message::SwitchRoute(Route::Setup))
                }
                Key::Character("1") => Some(Message::SwitchRoute(Route::Focus)),
                Key::Character("2") => Some(Message::SwitchRoute(Route::Stats)),
                Key::Character("3") => Some(Message::SwitchRoute(Route::Coach)),
                Key::Character("4") => Some(Message::SwitchRoute(Route::Setup)),
                Key::Character("5") | Key::Character("?") => {
                    Some(Message::SwitchRoute(Route::Help))
                }
                _ => None,
            }
        }));

        Subscription::batch(subs)
    }
}
