//! End-to-end integration test for the Pomodoro engine via the public crate API.
//!
//! Validates the full lifecycle that v1.1.21 enabled:
//!   start_focus → tick → auto-transition to Break → next focus → … → long break.
//!
//! This test exists as a regression guard for the 22 fixes shipped in v1.1.21.
//! Adding it here (instead of inline #[cfg(test)] mod) means it only sees
//! the *public* API — anything we accidentally make crate-private will break it.

use solar_focus_core::pomodoro::{PomodoroConfig, PomodoroEngine};
use solar_focus_core::state::AppState;

fn fast_engine() -> PomodoroEngine {
    PomodoroEngine::with_config(PomodoroConfig {
        focus_duration: 5.0,        // 5 s "focus" for fast tests
        short_break_duration: 2.0,  // 2 s short break
        long_break_duration: 8.0,   // 8 s long break
        sessions_before_long_break: 3,
    })
}

#[test]
fn full_session_completes_and_writes_progress() {
    let mut engine = fast_engine();

    engine.start_focus();
    assert!(matches!(engine.state(), AppState::Focusing(_)));
    assert_eq!(engine.progress(), 0.0);

    // Halfway through
    engine.tick(2.5);
    assert!((engine.progress() - 0.5).abs() < 0.05, "progress at half: {}", engine.progress());

    // Finish the focus
    engine.tick(3.0);
    assert_eq!(engine.state(), &AppState::Break);
    assert_eq!(engine.sessions_completed(), 1);
    // Progress should re-baseline against the (now active) break duration
    assert!(engine.progress() < 0.05, "break just started, progress should be ~0");
}

#[test]
fn three_sessions_trigger_long_break() {
    let mut engine = fast_engine();

    // Session 1 → short break
    engine.start_focus();
    engine.tick(5.0);
    assert_eq!(engine.state(), &AppState::Break);
    assert_eq!(engine.sessions_completed(), 1);

    // Session 2 → short break
    engine.transition_to_focus();
    assert!(matches!(engine.state(), AppState::Focusing(_)));
    engine.tick(5.0);
    assert_eq!(engine.state(), &AppState::Break);
    assert_eq!(engine.sessions_completed(), 2);

    // Session 3 → LONG break (8 s, not 2 s)
    engine.transition_to_focus();
    engine.tick(5.0);
    assert_eq!(engine.state(), &AppState::Break);
    assert_eq!(engine.sessions_completed(), 3);
    // Long break is 8.0; remaining at start should be near 8.0
    assert!(
        engine.remaining_time_formatted() == "00:08",
        "expected 00:08 long break, got {}",
        engine.remaining_time_formatted()
    );

    // Session 4 starts with a fresh streak counter
    engine.transition_to_focus();
    assert_eq!(engine.sessions_completed(), 0, "counter resets after long break");
}

#[test]
fn pause_freezes_remaining_time() {
    let mut engine = fast_engine();
    engine.start_focus();
    engine.tick(1.0);

    // Pause and tick — nothing should advance
    engine.pause(0.0);
    assert!(engine.is_paused());

    engine.tick(3.0);
    engine.tick(3.0);
    // Still in focus, still ~4 s remaining
    assert!(matches!(engine.state(), AppState::Focusing(_)));
    let formatted = engine.remaining_time_formatted();
    assert!(
        formatted == "00:04",
        "expected 00:04 after pause, got {formatted}"
    );

    // Resume and finish
    engine.resume();
    assert!(!engine.is_paused());
    engine.tick(4.0);
    assert_eq!(engine.state(), &AppState::Break);
}

#[test]
fn config_changes_via_with_config_take_effect() {
    let custom = PomodoroConfig {
        focus_duration: 12.0,
        short_break_duration: 3.0,
        long_break_duration: 9.0,
        sessions_before_long_break: 4,
    };
    let mut engine = PomodoroEngine::with_config(custom);
    engine.start_focus();
    assert_eq!(engine.remaining_time_formatted(), "00:12");
}

#[test]
fn idle_progress_is_zero() {
    let engine = PomodoroEngine::new();
    assert_eq!(engine.progress(), 0.0);
}
