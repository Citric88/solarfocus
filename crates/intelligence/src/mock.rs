//! Phase 1 stand-in implementations. Wire-compatible with the real LLM/classifier
//! that arrive in Phases 3 and 4.

use crate::prompts::{coaching_curated, summary_canned};
use crate::traits::*;
use crate::types::*;

pub struct MockCoach;

impl Coach for MockCoach {
    fn coaching_message(&self, trigger: CoachingTrigger, ctx: &FocusContext) -> AiFuture<String> {
        // FIX-COACH — curated bank, varied + grammatically correct, no LLM dependency.
        let s = coaching_curated(trigger, ctx);
        Box::pin(async move { Ok(s) })
    }
    fn is_ready(&self) -> bool {
        true
    }
}

pub struct MockClassifier;

impl DistractionClassifier for MockClassifier {
    fn classify(&self, _sample: &WindowSample) -> AiFuture<ClassificationResult> {
        Box::pin(async move { Ok(ClassificationResult::neutral()) })
    }
    fn is_ready(&self) -> bool {
        true
    }
}

pub struct MockSummarizer;

impl Summarizer for MockSummarizer {
    fn daily_summary(&self, ctx: &DaySummaryContext) -> AiFuture<String> {
        let s = summary_canned(ctx);
        Box::pin(async move { Ok(s) })
    }
    fn is_ready(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block_on<F: std::future::Future>(f: F) -> F::Output {
        // Tiny ad-hoc executor — avoids pulling tokio into core crate tests.
        use std::pin::pin;
        use std::sync::Arc;
        use std::task::{Context, Poll, Wake, Waker};

        struct NoopWaker;
        impl Wake for NoopWaker {
            fn wake(self: Arc<Self>) {}
        }

        let waker = Waker::from(Arc::new(NoopWaker));
        let mut ctx = Context::from_waker(&waker);
        let mut fut = pin!(f);
        loop {
            if let Poll::Ready(v) = fut.as_mut().poll(&mut ctx) {
                return v;
            }
        }
    }

    #[test]
    fn mock_coach_returns_spanish_session_start() {
        let coach = MockCoach;
        let ctx = FocusContext::empty(Language::Es, 1500);
        let msg = block_on(coach.coaching_message(CoachingTrigger::SessionStart, &ctx)).unwrap();
        // After FIX-COACH, the message comes from the curated bank — no
        // longer mentions duration verbatim. Just assert non-empty Spanish.
        assert!(!msg.is_empty(), "expected non-empty curated message");
        assert!(msg.len() > 8, "message too short: {}", msg);
        // Sanity: should not contain the broken phrases that surfaced in live testing.
        let lower = msg.to_lowercase();
        for bad in ["te has felicitado", "estoy feliz", "querido estudiante"] {
            assert!(!lower.contains(bad), "curated string contains bad phrase '{}': {}", bad, msg);
        }
    }

    #[test]
    fn mock_classifier_returns_neutral() {
        let c = MockClassifier;
        let sample = WindowSample {
            process_name: "Anything".into(),
            window_title: None,
            elapsed_in_session_secs: 0,
        };
        let r = block_on(c.classify(&sample)).unwrap();
        assert_eq!(r.label, ClassificationLabel::Neutral);
    }

    #[test]
    fn summary_includes_xp_and_level() {
        let s = MockSummarizer;
        let ctx = DaySummaryContext {
            date: "2026-05-02".into(),
            sessions_completed: 4,
            total_focus_secs: 6000,
            longest_streak: 4,
            level: 3,
            xp_gained: 240,
            language: Language::Es,
        };
        let msg = block_on(s.daily_summary(&ctx)).unwrap();
        assert!(msg.contains("4 sesiones"));
        assert!(msg.contains("Nivel 3"));
        assert!(msg.contains("240 XP"));
    }
}
