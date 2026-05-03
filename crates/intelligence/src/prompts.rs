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

/// Phase 3 + FEAT smarter-coach — completed prompt for the LLM with rich
/// context. Returns a SmolLM2-style ChatML string. The system prompt
/// gives the model a personality + length constraint; the user turn
/// gives concrete real numbers so coaching feels personal, not canned.
pub fn coaching_llm_prompt(trigger: CoachingTrigger, ctx: &FocusContext) -> String {
    let system = match ctx.language {
        Language::Es =>
            "Eres un coach de productividad cálido y conciso. Tu trabajo: ayudar a la persona a mantener el foco \
             usando los datos que se te dan. Reglas estrictas:\n\
             - Responde SIEMPRE en español.\n\
             - Una sola frase, máximo 20 palabras.\n\
             - Sin emojis.\n\
             - Personaliza con los números reales (sesiones, racha, hora, distracciones).\n\
             - Tono: directo, motivador, nunca sermoneador.",
        Language::En =>
            "You are a warm, concise productivity coach. Your job: help the person stay focused \
             using the data you're given. Strict rules:\n\
             - ALWAYS reply in English.\n\
             - One sentence, 20 words max.\n\
             - No emojis.\n\
             - Personalize with the real numbers (sessions, streak, hour, distractions).\n\
             - Tone: direct, motivating, never preachy.",
    };

    let part_of_day_es = match ctx.hour_of_day {
        5..=11 => "mañana",
        12..=17 => "tarde",
        18..=22 => "noche",
        _ => "madrugada",
    };
    let part_of_day_en = match ctx.hour_of_day {
        5..=11 => "morning",
        12..=17 => "afternoon",
        18..=22 => "evening",
        _ => "late night",
    };
    let weekday_es = ["lunes", "martes", "miércoles", "jueves", "viernes", "sábado", "domingo"]
        [(ctx.weekday as usize).min(6)];
    let weekday_en = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"]
        [(ctx.weekday as usize).min(6)];

    let user = match (trigger, ctx.language) {
        (CoachingTrigger::SessionStart, Language::Es) => format!(
            "CONTEXTO:\n\
             - Hora local: {}h ({})\n\
             - Día: {}\n\
             - Sesión #{} de hoy (racha actual: {})\n\
             - Distracciones confirmadas hoy: {}\n\
             - Foco total últimos 7 días: {} min\n\
             - Duración planeada: {} minutos\n\
             {}\n\
             TAREA: Saluda y motiva para esta sesión. Una frase.",
            ctx.hour_of_day, part_of_day_es, weekday_es,
            ctx.sessions_today + 1, ctx.streak,
            ctx.distractions_today, ctx.focus_minutes_7d,
            ctx.focus_duration_secs / 60,
            ctx.last_distraction.as_ref()
                .map(|d| format!("- Última distracción: {}", d))
                .unwrap_or_default(),
        ),
        (CoachingTrigger::SessionStart, Language::En) => format!(
            "CONTEXT:\n\
             - Local time: {}h ({})\n\
             - Day: {}\n\
             - Session #{} of today (current streak: {})\n\
             - Confirmed distractions today: {}\n\
             - Total focus last 7 days: {} min\n\
             - Planned duration: {} minutes\n\
             {}\n\
             TASK: Greet and motivate for this session. One sentence.",
            ctx.hour_of_day, part_of_day_en, weekday_en,
            ctx.sessions_today + 1, ctx.streak,
            ctx.distractions_today, ctx.focus_minutes_7d,
            ctx.focus_duration_secs / 60,
            ctx.last_distraction.as_ref()
                .map(|d| format!("- Last distraction: {}", d))
                .unwrap_or_default(),
        ),
        (CoachingTrigger::SessionComplete, Language::Es) => format!(
            "CONTEXTO:\n\
             - Sesión {} acabada · racha {}\n\
             - Hora local: {}h ({})\n\
             - Distracciones hoy: {}\n\
             - Foco últimos 7 días: {} min\n\
             TAREA: Felicita brevemente y sugiere si seguir o tomar pausa. Una frase.",
            ctx.sessions_today, ctx.streak,
            ctx.hour_of_day, part_of_day_es,
            ctx.distractions_today, ctx.focus_minutes_7d,
        ),
        (CoachingTrigger::SessionComplete, Language::En) => format!(
            "CONTEXT:\n\
             - Session {} done · streak {}\n\
             - Local time: {}h ({})\n\
             - Distractions today: {}\n\
             - Focus last 7 days: {} min\n\
             TASK: Briefly congratulate and suggest continue or take break. One sentence.",
            ctx.sessions_today, ctx.streak,
            ctx.hour_of_day, part_of_day_en,
            ctx.distractions_today, ctx.focus_minutes_7d,
        ),
        (CoachingTrigger::BreakStart, Language::Es) => format!(
            "CONTEXTO: Empezando pausa después de la sesión {}. Hora {}h ({}).\n\
             TAREA: Sugiere algo concreto para la pausa (estirar, respirar, agua). Una frase.",
            ctx.sessions_today, ctx.hour_of_day, part_of_day_es,
        ),
        (CoachingTrigger::BreakStart, Language::En) => format!(
            "CONTEXT: Starting break after session {}. Time {}h ({}).\n\
             TASK: Suggest something concrete for the break (stretch, breathe, water). One sentence.",
            ctx.sessions_today, ctx.hour_of_day, part_of_day_en,
        ),
        (CoachingTrigger::LongPauseDetected, Language::Es) => format!(
            "CONTEXTO: Pausa larga detectada durante la sesión. Hora {}h.\n\
             TAREA: Pregunta amablemente si vuelve al foco o termina la sesión. Una frase.",
            ctx.hour_of_day,
        ),
        (CoachingTrigger::LongPauseDetected, Language::En) => format!(
            "CONTEXT: Long pause detected during the session. Time {}h.\n\
             TASK: Gently ask if returning to focus or ending session. One sentence.",
            ctx.hour_of_day,
        ),
        (CoachingTrigger::StreakMilestone(n), Language::Es) => format!(
            "CONTEXTO: Racha de {} sesiones alcanzada. Día {}.\n\
             TAREA: Celebra el hito de forma breve y específica. Una frase.",
            n, weekday_es,
        ),
        (CoachingTrigger::StreakMilestone(n), Language::En) => format!(
            "CONTEXT: Streak of {} sessions reached. Day {}.\n\
             TASK: Celebrate the milestone briefly and specifically. One sentence.",
            n, weekday_en,
        ),
    };

    format!(
        "<|im_start|>system\n{}\n<|im_end|>\n<|im_start|>user\n{}\n<|im_end|>\n<|im_start|>assistant\n",
        system, user
    )
}
