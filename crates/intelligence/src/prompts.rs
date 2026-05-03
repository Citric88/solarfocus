//! Plantillas de prompts para coaching y resúmenes. Phase 1 las usa el `MockCoach`
//! como strings canónicos. Phase 3 las inyectará en el LLM con el chat template
//! correcto (ChatML para SmolLM2, etc.).

use crate::types::{CoachingTrigger, FocusContext, DaySummaryContext, Language};

/// FIX-COACH — Curated handcrafted messages keyed on trigger × language × time-of-day.
/// Picks one deterministically by seed so users see variety. The LLM is *not*
/// used here — it's reserved for tasks where it adds real value (daily summary
/// paraphrasing).
///
/// Why: SmolLM2-1.7B is too small to reliably write coherent Spanish coaching
/// lines. After live testing produced "Felicidades, te has felicitado!" the
/// strategic call is "use the LLM as a paraphraser, not a writer."
///
/// FIX-1 (rc14) — SessionStart pools are now split by SessionPhase so we
/// don't say "Buenos días, primera sesión del día" on the user's 5th session.
pub fn coaching_curated(trigger: CoachingTrigger, ctx: &FocusContext) -> String {
    let phase = if ctx.sessions_today == 0 {
        SessionPhase::First
    } else {
        SessionPhase::Continuation
    };

    // v1.3 Wave A3 — category-aware override for SessionStart only.
    // If the user picked a known category, give them a line that names
    // it. Other triggers stay generic to keep the pool small.
    if matches!(trigger, CoachingTrigger::SessionStart) {
        if let Some(cat) = ctx.category.as_deref() {
            if let Some(line) = category_session_start(cat, ctx.language, ctx.hour_of_day, ctx.sessions_today) {
                return line;
            }
        }
    }

    let pool: &[&str] = match (trigger, ctx.language, time_bucket(ctx.hour_of_day)) {
        // ─── ES · SessionStart (split by phase) ─────────────────────────
        (CoachingTrigger::SessionStart, Language::Es, TimeBucket::Morning) => match phase {
            SessionPhase::First => &[
                "Buenos días. Vamos por la primera sesión del día.",
                "Mañana fresca, mente fresca. A enfocar.",
                "Empieza el día con foco. Cierra pestañas innecesarias.",
                "Una sesión completa antes del primer café.",
                "Un sprint de foco para arrancar bien.",
            ],
            SessionPhase::Continuation => &[
                "Sigamos la mañana con otra sesión.",
                "Otra sesión más antes del mediodía.",
                "Mantén el ritmo de la mañana.",
                "Cierra distracciones y a enfocar.",
                "Foco continuo. Vamos.",
            ],
        },
        (CoachingTrigger::SessionStart, Language::Es, TimeBucket::Afternoon) => match phase {
            SessionPhase::First => &[
                "Buenas tardes. Primera sesión del día, calma y foco.",
                "Tarde productiva por delante. Empezamos.",
                "Sin prisa pero con foco.",
            ],
            SessionPhase::Continuation => &[
                "Tarde productiva. A por la siguiente.",
                "Una sesión más antes de bajar el ritmo.",
                "Cierra otras apps y vamos.",
                "Foco profundo durante los próximos minutos.",
                "Respira y arranca. Tienes esto.",
            ],
        },
        (CoachingTrigger::SessionStart, Language::Es, TimeBucket::Evening) => match phase {
            SessionPhase::First => &[
                "Sesión de noche. Calma y constancia.",
                "Aún hay tiempo para una sesión productiva.",
                "Tarde-noche con foco. Vamos.",
            ],
            SessionPhase::Continuation => &[
                "Otra sesión más antes de cerrar.",
                "Foco vespertino. Ritmo tranquilo.",
                "Cierre del día con foco.",
                "Una última sesión y a descansar bien.",
            ],
        },
        (CoachingTrigger::SessionStart, Language::Es, TimeBucket::LateNight) => match phase {
            SessionPhase::First => &[
                "Sesión de madrugada: cuida los ojos y la postura.",
                "Si decides empezar ahora, mantén la pantalla atenuada.",
            ],
            SessionPhase::Continuation => &[
                "Otra sesión nocturna. Recuerda dormir lo suficiente.",
                "Foco breve y a la cama.",
                "Mantén el ritmo, pero sin abusar.",
            ],
        },
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

        // ─── EN · SessionStart (split by phase) ─────────────────────────
        (CoachingTrigger::SessionStart, Language::En, TimeBucket::Morning) => match phase {
            SessionPhase::First => &[
                "Good morning. First session of the day.",
                "Fresh morning, fresh mind. Let's focus.",
                "Start the day with focus. Close unneeded tabs.",
                "One full session before the first coffee.",
                "A focus sprint to kick things off.",
            ],
            SessionPhase::Continuation => &[
                "Keep the morning going with another session.",
                "One more before noon.",
                "Hold the morning rhythm.",
                "Close distractions and refocus.",
                "Continued focus. Go.",
            ],
        },
        (CoachingTrigger::SessionStart, Language::En, TimeBucket::Afternoon) => match phase {
            SessionPhase::First => &[
                "Good afternoon. First session of the day, ease into focus.",
                "Productive afternoon ahead. Let's begin.",
                "Slow start, sharp focus.",
            ],
            SessionPhase::Continuation => &[
                "Productive afternoon. On to the next.",
                "One more session before slowing down.",
                "Close other apps and go.",
                "Deep focus for the next few minutes.",
                "Breathe in and begin. You've got this.",
            ],
        },
        (CoachingTrigger::SessionStart, Language::En, TimeBucket::Evening) => match phase {
            SessionPhase::First => &[
                "Evening session. Calm and steady.",
                "Still time for a focused session.",
                "Late-day focus. Let's go.",
            ],
            SessionPhase::Continuation => &[
                "One more before wrapping up.",
                "Late-day focus. Keep the pace gentle.",
                "Closing the day with focus.",
                "One last session, then rest well.",
            ],
        },
        (CoachingTrigger::SessionStart, Language::En, TimeBucket::LateNight) => match phase {
            SessionPhase::First => &[
                "Late-night session: watch your eyes and posture.",
                "If you start now, dim the screen.",
            ],
            SessionPhase::Continuation => &[
                "Another late-night session. Don't skimp on sleep.",
                "Short focus, then bed.",
                "Hold the rhythm but don't overdo it.",
            ],
        },
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

/// v1.3 Wave A3 — category-flavored SessionStart lines. Returns
/// `None` when the category isn't one we have copy for (e.g. user
/// typed "Other" or a free-form label) — caller falls back to the
/// generic pool.
fn category_session_start(category: &str, lang: Language, hour: u8, sessions_today: u8) -> Option<String> {
    let key = category.trim().to_lowercase();
    let pool: &[&str] = match (key.as_str(), lang) {
        ("coding" | "código" | "codigo", Language::Es) => &[
            "A escribir código sin distracciones.",
            "Sesión de código. Cierra Slack y a por el siguiente bug.",
            "Una sesión limpia: el editor y tú.",
        ],
        ("coding" | "código" | "codigo", Language::En) => &[
            "Coding focus. Close Slack and ship the next change.",
            "Editor and you. No distractions.",
            "One clean coding session.",
        ],
        ("writing" | "escritura", Language::Es) => &[
            "Sesión de escritura. Una idea a la vez.",
            "Escribe sin editar. Editar es para después.",
            "Una página, un borrador, un avance.",
        ],
        ("writing" | "escritura", Language::En) => &[
            "Writing session. One idea at a time.",
            "Write first, edit later.",
            "One page, one draft, one step forward.",
        ],
        ("reading" | "lectura", Language::Es) => &[
            "Sesión de lectura profunda. Sin tabs paralelas.",
            "Lee con calma. Anota lo importante.",
            "Una lectura, un highlight, una idea nueva.",
        ],
        ("reading" | "lectura", Language::En) => &[
            "Deep reading session. No side tabs.",
            "Read slowly. Note what matters.",
            "One read, one highlight, one new idea.",
        ],
        ("deep work" | "trabajo profundo", Language::Es) => &[
            "Trabajo profundo. Sin notificaciones, sin chats.",
            "Una sesión sin interrupciones. Apaga lo que no necesites.",
            "Foco profundo. Llega al estado de flow.",
        ],
        ("deep work" | "trabajo profundo", Language::En) => &[
            "Deep work. No notifications, no chats.",
            "One uninterrupted session. Mute what you can.",
            "Deep focus. Reach the flow state.",
        ],
        _ => return None,
    };
    let seed = (hour as usize)
        .wrapping_mul(31)
        .wrapping_add(sessions_today as usize);
    Some(pool[seed % pool.len()].to_string())
}

#[derive(Copy, Clone, PartialEq, Eq)]
enum TimeBucket {
    Morning,
    Afternoon,
    Evening,
    LateNight,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum SessionPhase {
    /// `sessions_today == 0` — greeting / welcoming language allowed.
    First,
    /// `sessions_today >= 1` — must NOT claim "first" / "buenos días".
    Continuation,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx_at_hour(hour: u8, sessions: u8, lang: Language) -> FocusContext {
        let mut c = FocusContext::empty(lang, 1500);
        c.hour_of_day = hour;
        c.sessions_today = sessions;
        c
    }

    /// Sample a pool many times across a representative axis (different
    /// session counts + hour offsets) so the seed-hash hits multiple entries.
    fn sample_curated(
        trigger: CoachingTrigger,
        lang: Language,
        hour: u8,
        sessions: u8,
    ) -> Vec<String> {
        let mut out = Vec::new();
        for s in 0..16u8 {
            let ctx = ctx_at_hour(hour, sessions.saturating_add(s), lang);
            // We want the *same phase* across samples but vary the seed.
            // The seed includes sessions_today, so this gives us coverage.
            if (s == 0 && sessions == 0) || (s > 0 && sessions == 0) {
                // First-phase samples
                if sessions == 0 && s == 0 {
                    out.push(coaching_curated(trigger, &ctx));
                }
            } else if sessions > 0 {
                out.push(coaching_curated(trigger, &ctx));
            }
        }
        // Also add the "exact" case with the original sessions count.
        out.push(coaching_curated(trigger, &ctx_at_hour(hour, sessions, lang)));
        out
    }

    #[test]
    fn morning_first_vs_continuation_diverge_es() {
        let first = coaching_curated(
            CoachingTrigger::SessionStart,
            &ctx_at_hour(9, 0, Language::Es),
        );
        // Sample several continuation messages to make sure none equals First.
        for s in 1..6u8 {
            let cont = coaching_curated(
                CoachingTrigger::SessionStart,
                &ctx_at_hour(9, s, Language::Es),
            );
            assert!(
                !cont.to_lowercase().contains("primera"),
                "continuation sample at sessions={} contains 'primera': {}",
                s,
                cont
            );
            assert!(
                !cont.to_lowercase().contains("buenos días"),
                "continuation sample at sessions={} contains 'buenos días': {}",
                s,
                cont
            );
            assert!(
                !cont.to_lowercase().contains("primer café"),
                "continuation sample at sessions={} contains 'primer café': {}",
                s,
                cont
            );
            assert_ne!(first, cont, "First and Continuation must not collide at sessions={}", s);
        }
    }

    #[test]
    fn morning_first_vs_continuation_diverge_en() {
        for s in 1..6u8 {
            let cont = coaching_curated(
                CoachingTrigger::SessionStart,
                &ctx_at_hour(9, s, Language::En),
            );
            let lower = cont.to_lowercase();
            assert!(
                !lower.contains("first session"),
                "EN continuation contains 'first session': {}",
                cont
            );
            assert!(
                !lower.contains("good morning"),
                "EN continuation contains 'good morning': {}",
                cont
            );
            assert!(
                !lower.contains("first coffee"),
                "EN continuation contains 'first coffee': {}",
                cont
            );
            assert!(
                !lower.contains("kick things off"),
                "EN continuation contains 'kick things off': {}",
                cont
            );
        }
    }

    #[test]
    fn continuation_pool_never_says_first_in_any_bucket_or_lang() {
        let banned_es = ["primera", "buenos días", "buenas tardes"];
        let banned_en = ["first session", "good morning", "good afternoon"];
        for hour in [9u8, 14, 20, 2] {
            for sessions in [1u8, 2, 5, 10] {
                let es = coaching_curated(
                    CoachingTrigger::SessionStart,
                    &ctx_at_hour(hour, sessions, Language::Es),
                );
                let lower_es = es.to_lowercase();
                for w in banned_es.iter() {
                    assert!(
                        !lower_es.contains(w),
                        "ES @ hour={} sessions={} contains '{}': {}",
                        hour,
                        sessions,
                        w,
                        es
                    );
                }
                let en = coaching_curated(
                    CoachingTrigger::SessionStart,
                    &ctx_at_hour(hour, sessions, Language::En),
                );
                let lower_en = en.to_lowercase();
                for w in banned_en.iter() {
                    assert!(
                        !lower_en.contains(w),
                        "EN @ hour={} sessions={} contains '{}': {}",
                        hour,
                        sessions,
                        w,
                        en
                    );
                }
            }
        }
    }

    #[test]
    fn each_bucket_has_nonempty_first_and_cont_pool() {
        // Sanity: every hour bucket × language × phase combo returns a
        // non-empty string (would catch an accidental empty array).
        for hour in [9u8, 14, 20, 2] {
            for sessions in [0u8, 3] {
                for lang in [Language::Es, Language::En] {
                    let s = coaching_curated(
                        CoachingTrigger::SessionStart,
                        &ctx_at_hour(hour, sessions, lang),
                    );
                    assert!(
                        s.len() >= 8,
                        "Empty/short pool at hour={} sessions={} lang={:?}: '{}'",
                        hour,
                        sessions,
                        lang,
                        s
                    );
                }
            }
        }
        let _ = sample_curated; // silence unused warning if tests evolve
    }

    /// v1.3 Wave A3 — when category="Coding" the SessionStart line
    /// must mention coding (in either language).
    #[test]
    fn category_aware_session_start_es_coding() {
        let mut ctx = ctx_at_hour(10, 0, Language::Es);
        ctx.category = Some("Coding".to_string());
        let s = coaching_curated(CoachingTrigger::SessionStart, &ctx);
        assert!(
            s.to_lowercase().contains("código") || s.to_lowercase().contains("editor") || s.to_lowercase().contains("slack"),
            "expected ES coding-flavored line, got: {}",
            s
        );
    }

    #[test]
    fn category_aware_session_start_en_writing() {
        let mut ctx = ctx_at_hour(15, 2, Language::En);
        ctx.category = Some("Writing".to_string());
        let s = coaching_curated(CoachingTrigger::SessionStart, &ctx);
        assert!(
            s.to_lowercase().contains("writ") || s.to_lowercase().contains("draft") || s.to_lowercase().contains("page"),
            "expected EN writing-flavored line, got: {}",
            s
        );
    }

    #[test]
    fn category_unknown_falls_back_to_generic_pool() {
        let mut ctx = ctx_at_hour(10, 0, Language::Es);
        ctx.category = Some("RandomThing".to_string());
        let s = coaching_curated(CoachingTrigger::SessionStart, &ctx);
        // Should be from the generic morning-first pool (no coding/writing words).
        assert!(!s.to_lowercase().contains("código"));
        assert!(s.len() > 8);
    }

    /// Other triggers ignore category — SessionComplete should never
    /// pull from the coding-flavored pool.
    #[test]
    fn category_only_overrides_session_start() {
        let mut ctx = ctx_at_hour(10, 0, Language::Es);
        ctx.category = Some("Coding".to_string());
        let complete = coaching_curated(CoachingTrigger::SessionComplete, &ctx);
        // SessionComplete pool has no coding-specific lines.
        assert!(!complete.to_lowercase().contains("código"));
    }
}
