//! Plantillas de prompts para coaching y resúmenes. Phase 1 las usa el `MockCoach`
//! como strings canónicos. Phase 3 las inyectará en el LLM con el chat template
//! correcto (ChatML para SmolLM2, etc.).

use crate::types::{CoachingTrigger, FocusContext, DaySummaryContext, Language};

pub fn coaching_canned(trigger: CoachingTrigger, ctx: &FocusContext) -> String {
    let mins = ctx.focus_duration_secs / 60;
    match (trigger, ctx.language) {
        (CoachingTrigger::SessionStart, Language::Es) => {
            format!("Empezamos. {} minutos de foco.", mins)
        }
        (CoachingTrigger::SessionStart, Language::En) => {
            format!("Let's begin. {} minutes of focus.", mins)
        }
        (CoachingTrigger::BreakStart, Language::Es) => {
            "Buen trabajo. Levantate y respira.".to_string()
        }
        (CoachingTrigger::BreakStart, Language::En) => {
            "Nice work. Stand up and breathe.".to_string()
        }
        (CoachingTrigger::SessionComplete, Language::Es) => {
            format!("Sesion {} completada hoy.", ctx.sessions_today)
        }
        (CoachingTrigger::SessionComplete, Language::En) => {
            format!("Session {} done today.", ctx.sessions_today)
        }
        (CoachingTrigger::LongPauseDetected, Language::Es) => {
            "Pausa larga detectada. ¿Volvemos?".to_string()
        }
        (CoachingTrigger::LongPauseDetected, Language::En) => {
            "Long pause detected. Ready to resume?".to_string()
        }
        (CoachingTrigger::StreakMilestone(n), Language::Es) => {
            format!("Racha de {} sesiones. Sigue asi.", n)
        }
        (CoachingTrigger::StreakMilestone(n), Language::En) => {
            format!("Streak of {} sessions. Keep going.", n)
        }
    }
}

pub fn summary_canned(ctx: &DaySummaryContext) -> String {
    match ctx.language {
        Language::Es => format!(
            "{}: {} sesiones, {} minutos de foco, +{} XP. Nivel {}.",
            ctx.date,
            ctx.sessions_completed,
            ctx.total_focus_secs / 60,
            ctx.xp_gained,
            ctx.level,
        ),
        Language::En => format!(
            "{}: {} sessions, {} minutes focused, +{} XP. Level {}.",
            ctx.date,
            ctx.sessions_completed,
            ctx.total_focus_secs / 60,
            ctx.xp_gained,
            ctx.level,
        ),
    }
}

/// Phase 3 hook — completed prompt for the LLM with chat template wrapping.
/// Returns a SmolLM2-style ChatML string. Not yet consumed in Phase 1.
pub fn coaching_llm_prompt(trigger: CoachingTrigger, ctx: &FocusContext) -> String {
    let system = match ctx.language {
        Language::Es => "Eres un coach minimalista. Responde en español, máximo 1 frase, sin emojis.",
        Language::En => "You are a minimal coach. Reply in English, 1 sentence max, no emojis.",
    };
    let user = match (trigger, ctx.language) {
        (CoachingTrigger::SessionStart, Language::Es) => format!(
            "Empezando sesión de {} minutos. Hoy llevo {} sesiones, racha {}.",
            ctx.focus_duration_secs / 60, ctx.sessions_today, ctx.streak,
        ),
        (CoachingTrigger::SessionStart, Language::En) => format!(
            "Starting a {}-minute session. Today: {} sessions, streak {}.",
            ctx.focus_duration_secs / 60, ctx.sessions_today, ctx.streak,
        ),
        (CoachingTrigger::SessionComplete, Language::Es) => format!(
            "Acabo de completar la sesión {}. Racha {}, XP {}.",
            ctx.sessions_today, ctx.streak, ctx.xp_today,
        ),
        (CoachingTrigger::SessionComplete, Language::En) => format!(
            "Just finished session {}. Streak {}, XP {}.",
            ctx.sessions_today, ctx.streak, ctx.xp_today,
        ),
        (other, _) => format!("{:?}", other),
    };
    format!(
        "<|im_start|>system\n{}\n<|im_end|>\n<|im_start|>user\n{}\n<|im_end|>\n<|im_start|>assistant\n",
        system, user
    )
}
