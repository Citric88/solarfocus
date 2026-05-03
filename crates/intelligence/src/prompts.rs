//! Plantillas de prompts para coaching y resúmenes. Phase 1 las usa el `MockCoach`
//! como strings canónicos. Phase 3 las inyectará en el LLM con el chat template
//! correcto (ChatML para SmolLM2, etc.).

use crate::types::{CoachingTrigger, FocusContext, DaySummaryContext, Language};

/// FIX-COACH — Curated handcrafted messages keyed on trigger × language × time-of-day.
/// Picks one randomly so users see variety. The LLM is *not* used here — it's
/// reserved for tasks where it adds real value (daily summary paraphrasing).
///
/// Why: SmolLM2-1.7B is too small to reliably write coherent Spanish coaching
/// lines. After live testing produced "Felicidades, te has felicitado!" the
/// strategic call is "use the LLM as a paraphraser, not a writer."
pub fn coaching_curated(trigger: CoachingTrigger, ctx: &FocusContext) -> String {
    let pool: &[&str] = match (trigger, ctx.language, time_bucket(ctx.hour_of_day)) {
        // ─── ES · SessionStart ──────────────────────────────────────────
        (CoachingTrigger::SessionStart, Language::Es, TimeBucket::Morning) => &[
            "Buenos días. Vamos por la primera sesión del día.",
            "Mañana fresca, mente fresca. A enfocar.",
            "Empieza el día con foco. Cierra pestañas innecesarias.",
            "Una sesión completa antes del primer café.",
            "Un sprint de foco para arrancar bien.",
        ],
        (CoachingTrigger::SessionStart, Language::Es, TimeBucket::Afternoon) => &[
            "Tarde productiva. A por la siguiente.",
            "Una sesión más antes de bajar el ritmo.",
            "Cierra otras apps y vamos.",
            "Foco profundo durante los próximos minutos.",
            "Respira y arranca. Tienes esto.",
        ],
        (CoachingTrigger::SessionStart, Language::Es, TimeBucket::Evening) => &[
            "Sesión de noche. Calma y constancia.",
            "Foco vespertino. Ritmo tranquilo.",
            "Cierre del día con foco.",
            "Una última sesión y a descansar bien.",
        ],
        (CoachingTrigger::SessionStart, Language::Es, TimeBucket::LateNight) => &[
            "Tarde-noche: cuida los ojos y la postura.",
            "Sesión nocturna. Mantén la pantalla atenuada.",
            "A enfocar, pero recuerda dormir lo suficiente.",
        ],
        // ─── ES · SessionComplete ───────────────────────────────────────
        (CoachingTrigger::SessionComplete, Language::Es, _) => &[
            "Sesión completada. Buen trabajo.",
            "Una más al marcador. Levántate y respira.",
            "Foco logrado. Tomate cinco minutos.",
            "Cerraste la sesión. Hidrátate.",
            "Bien hecho. Camina un poco antes de la siguiente.",
            "Cierre limpio. Pausa breve antes de continuar.",
        ],
        // ─── ES · BreakStart ────────────────────────────────────────────
        (CoachingTrigger::BreakStart, Language::Es, _) => &[
            "Pausa: estira el cuello y los hombros.",
            "Mira algo a más de 6 metros durante un minuto.",
            "Levántate, respira hondo y camina un poco.",
            "Bebe agua antes de la siguiente sesión.",
            "Aleja la vista de la pantalla. Mira por la ventana.",
            "Pausa real: no abras redes sociales.",
        ],
        // ─── ES · LongPauseDetected ─────────────────────────────────────
        (CoachingTrigger::LongPauseDetected, Language::Es, _) => &[
            "Llevas un rato pausado. ¿Vuelves o terminas?",
            "Pausa larga detectada. Decide y sigue.",
            "Si necesitas más tiempo, termina la sesión y vuelve después.",
        ],
        // ─── ES · StreakMilestone ───────────────────────────────────────
        (CoachingTrigger::StreakMilestone(_), Language::Es, _) => &[
            "Buena racha. Sigue con el mismo ritmo.",
            "Constancia. Eso es lo que cuenta.",
            "Racha sostenida. Mañana más.",
        ],

        // ─── EN · SessionStart ──────────────────────────────────────────
        (CoachingTrigger::SessionStart, Language::En, TimeBucket::Morning) => &[
            "Good morning. First session of the day.",
            "Fresh morning, fresh mind. Let's focus.",
            "Start the day with focus. Close unneeded tabs.",
            "One full session before the first coffee.",
            "A focus sprint to kick things off.",
        ],
        (CoachingTrigger::SessionStart, Language::En, TimeBucket::Afternoon) => &[
            "Productive afternoon. On to the next.",
            "One more session before slowing down.",
            "Close other apps and go.",
            "Deep focus for the next few minutes.",
            "Breathe in and begin. You've got this.",
        ],
        (CoachingTrigger::SessionStart, Language::En, TimeBucket::Evening) => &[
            "Evening session. Calm and steady.",
            "Late-day focus. Keep the pace gentle.",
            "Closing the day with focus.",
            "One last session, then rest well.",
        ],
        (CoachingTrigger::SessionStart, Language::En, TimeBucket::LateNight) => &[
            "Late night: watch your eyes and posture.",
            "Night session. Dim the screen.",
            "Focus, but remember to get enough sleep.",
        ],
        // ─── EN · SessionComplete ───────────────────────────────────────
        (CoachingTrigger::SessionComplete, Language::En, _) => &[
            "Session complete. Nice work.",
            "One more on the board. Stand up and breathe.",
            "Focus achieved. Take five.",
            "Session closed. Hydrate.",
            "Well done. Walk a little before the next one.",
            "Clean finish. Brief pause before continuing.",
        ],
        // ─── EN · BreakStart ────────────────────────────────────────────
        (CoachingTrigger::BreakStart, Language::En, _) => &[
            "Break: stretch your neck and shoulders.",
            "Look at something 20 feet away for a minute.",
            "Stand up, breathe deeply, walk a little.",
            "Drink water before the next session.",
            "Look away from the screen. Look out the window.",
            "Real break: no social media.",
        ],
        // ─── EN · LongPauseDetected ─────────────────────────────────────
        (CoachingTrigger::LongPauseDetected, Language::En, _) => &[
            "You've been paused a while. Resume or end?",
            "Long pause detected. Decide and move on.",
            "If you need more time, end the session and come back.",
        ],
        // ─── EN · StreakMilestone ───────────────────────────────────────
        (CoachingTrigger::StreakMilestone(_), Language::En, _) => &[
            "Nice streak. Keep the same rhythm.",
            "Consistency. That's what counts.",
            "Streak holding. More tomorrow.",
        ],
    };

    // Cheap deterministic-ish pick: hash the trigger + hour + sessions_today.
    let seed = (ctx.hour_of_day as usize)
        .wrapping_mul(31)
        .wrapping_add(ctx.sessions_today as usize)
        .wrapping_add(match trigger {
            CoachingTrigger::SessionStart => 1,
            CoachingTrigger::SessionComplete => 2,
            CoachingTrigger::BreakStart => 3,
            CoachingTrigger::LongPauseDetected => 4,
            CoachingTrigger::StreakMilestone(n) => 5 + n as usize,
        });
    pool[seed % pool.len()].to_string()
}

#[derive(Copy, Clone, PartialEq, Eq)]
enum TimeBucket {
    Morning,
    Afternoon,
    Evening,
    LateNight,
}

fn time_bucket(hour: u8) -> TimeBucket {
    match hour {
        5..=11 => TimeBucket::Morning,
        12..=17 => TimeBucket::Afternoon,
        18..=22 => TimeBucket::Evening,
        _ => TimeBucket::LateNight,
    }
}

/// FIX-COACH — Validates an LLM-generated string against basic quality rules.
/// Returns true if the string is safe to display; false → caller should fall
/// back to `coaching_curated`. Catches the failure modes seen in live testing
/// ("te estoy feliz", "te has felicitado", echoes of the prompt, English when
/// Spanish requested, etc.).
pub fn looks_coherent(s: &str, lang: Language) -> bool {
    let s = s.trim();
    if s.len() < 8 || s.len() > 220 {
        return false;
    }
    let lower = s.to_ascii_lowercase();
    // Reject if it echoes the prompt scaffolding
    let bad_substrings = [
        "context:", "contexto:", "task:", "tarea:", "<|", "|>", "system",
        "estoy feliz", "te has felicitado", "querido estudiante",
        "necesito", "ayudarte con", "como un asistente",
    ];
    for bad in bad_substrings {
        if lower.contains(bad) {
            return false;
        }
    }
    // Language sanity: if user wants ES, reject lines with too many English
    // common-words; if EN, reject lines with too many ES.
    let es_markers = ["el ", "la ", "de ", "que ", "los ", "una ", "para ", "con ", "está ", "tu "];
    let en_markers = ["the ", "and ", "your ", "you ", "this ", "that ", "with ", "for "];
    let es_hits = es_markers.iter().filter(|w| lower.contains(*w)).count();
    let en_hits = en_markers.iter().filter(|w| lower.contains(*w)).count();
    match lang {
        Language::Es if en_hits > es_hits + 2 => return false,
        Language::En if es_hits > en_hits + 2 => return false,
        _ => {}
    }
    true
}

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
