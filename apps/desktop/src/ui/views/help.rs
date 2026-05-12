//! v1.12.2 — Help canvas, full rewrite.
//!
//! Strategic intent: first canvas a new user clicks. Must (a) sell the
//! privacy-first contract, (b) document every visible feature so the
//! user discovers what's available, (c) offer a quick-start path,
//! (d) anchor keyboard shortcuts. Replaces the 5-feature v1.6.0 layout
//! that documented only 3/12 of the actual product.

use iced::widget::{button, column, container, text};
use iced::{Element, Length};
use solar_focus_intelligence::Language;

use crate::ui::components::{badge_local, settings_card_local, BadgeVariant};
use crate::ui::palette::*;
use crate::ui::sidebar::{IconCanvas, IconGlyph, Route};
use crate::{App, Message};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const REPO_URL: &str = "github.com/Citric88/solarfocus";

impl App {
    pub fn view_help(&self) -> Element<'_, Message> {
        let lang = self.settings.language;
        let pick = |es: &'static str, en: &'static str| -> &'static str {
            if lang == Language::Es { es } else { en }
        };

        // 1. Identity strip — compact title + version pill + tagline.
        let identity = column![
            iced::widget::row![
                text("SolarFocus OS").size(FONT_TITLE).color(TEXT_PRIMARY),
                iced::widget::Space::with_width(SPACE_SM as f32),
                badge_local(format!("v{VERSION}"), BadgeVariant::Accent),
            ]
            .align_y(iced::alignment::Vertical::Center),
            text(pick(
                "Pomodoro con coach IA local, cosechas de semillas y detección de presencia. Todo el procesamiento ocurre en tu equipo.",
                "Pomodoro with a local AI coach, seed rewards and presence detection. Everything runs on your machine.",
            ))
            .size(FONT_BODY)
            .color(TEXT_SECONDARY),
        ]
        .spacing(SPACE_XS as u16);

        // 2. Privacy hero — three chips reinforcing the contract.
        let privacy_hero = container(
            column![
                text(pick("Privacidad por diseño", "Privacy by design"))
                    .size(FONT_LEAD)
                    .color(TEXT_PRIMARY),
                iced::widget::row![
                    badge_local(pick("Sin nube", "No cloud").to_string(), BadgeVariant::Muted),
                    iced::widget::Space::with_width(SPACE_XS as f32),
                    badge_local(
                        pick("Sin telemetría", "No telemetry").to_string(),
                        BadgeVariant::Muted,
                    ),
                    iced::widget::Space::with_width(SPACE_XS as f32),
                    badge_local(
                        pick("Sin cuenta", "No account").to_string(),
                        BadgeVariant::Muted,
                    ),
                    iced::widget::Space::with_width(SPACE_XS as f32),
                    badge_local(
                        pick("100% local", "100% local").to_string(),
                        BadgeVariant::Accent,
                    ),
                ],
            ]
            .spacing(SPACE_SM as u16),
        )
        .padding(SPACE_MD as u16)
        .style(|_| container::Style {
            background: Some(iced::Background::Color(SURFACE_RAISED)),
            border: iced::Border {
                color: ACCENT_DIM,
                width: 1.0,
                radius: 8.0.into(),
            },
            ..Default::default()
        });

        // 3. CTA + coach status row.
        let cta = button(text(pick("Empezar foco", "Start focus").to_string()).size(FONT_BODY).color(BG))
            .on_press(Message::SwitchRoute(Route::Focus))
            .padding([12, 32])
            .style(|_, _| iced::widget::button::Style {
                background: Some(iced::Background::Color(ACCENT)),
                text_color: BG,
                border: iced::Border { radius: 8.0.into(), ..Default::default() },
                ..Default::default()
            });
        let coach_status_label: String = if cfg!(feature = "llm") {
            match (self.settings.ai_enabled, self.coach.is_ready()) {
                (true, true) => match lang {
                    Language::Es => format!("Coach IA · {:?}", self.settings.model_choice),
                    Language::En => format!("AI coach · {:?}", self.settings.model_choice),
                },
                (true, false) => pick(
                    "Coach IA · esperando modelo",
                    "AI coach · awaiting model",
                )
                .to_string(),
                _ => pick("Coach IA · desactivado", "AI coach · disabled").to_string(),
            }
        } else {
            pick("Build sin LLM", "Build without LLM").to_string()
        };
        let cta_row: Element<'_, Message> = iced::widget::row![
            cta,
            iced::widget::Space::with_width(SPACE_MD as f32),
            badge_local(coach_status_label, BadgeVariant::Muted),
        ]
        .align_y(iced::alignment::Vertical::Center)
        .into();

        // 4. Quick start card.
        let quick_start = settings_card_local(
            pick("Empezar rápido", "Quick start"),
            column![
                text(pick(
                    "1. Abre Focus y elige una categoría (chip o texto libre).",
                    "1. Open Focus and pick a category (chip or free text).",
                ))
                .size(FONT_SMALL)
                .color(TEXT_PRIMARY),
                text(pick(
                    "2. Pulsa Empezar foco. SolarFocus cuenta los minutos y avisa de distracciones.",
                    "2. Hit Start focus. SolarFocus counts minutes and flags distractions.",
                ))
                .size(FONT_SMALL)
                .color(TEXT_PRIMARY),
                text(pick(
                    "3. Cierra la sesión y revisa Stats / Coach: cosechas semillas si la Atención supera el umbral.",
                    "3. End the session and check Stats / Coach: you harvest seeds if Focus % crosses the threshold.",
                ))
                .size(FONT_SMALL)
                .color(TEXT_PRIMARY),
            ]
            .spacing(SPACE_XS as u16)
            .into(),
        );

        // 5. Feature grid — compact card builder.
        let feature = |glyph: IconGlyph, title_str: String, summary_str: String| -> Element<'_, Message> {
            container(
                iced::widget::row![
                    iced::widget::Canvas::new(IconCanvas { glyph, selected: true })
                        .width(Length::Fixed(28.0))
                        .height(Length::Fixed(28.0)),
                    iced::widget::Space::with_width(SPACE_SM as f32),
                    column![
                        text(title_str).size(FONT_BODY).color(TEXT_PRIMARY),
                        text(summary_str).size(FONT_SMALL).color(TEXT_SECONDARY),
                    ]
                    .spacing(2),
                ]
                .padding(SPACE_SM as u16)
                .align_y(iced::alignment::Vertical::Top),
            )
            .padding(SPACE_XS as u16)
            .width(Length::Fixed(420.0))
            .style(|_| container::Style {
                background: Some(iced::Background::Color(SURFACE)),
                border: iced::Border { radius: 8.0.into(), width: 1.0, color: ACCENT_DIM },
                ..Default::default()
            })
            .into()
        };

        let f1 = feature(
            IconGlyph::Focus,
            pick("Pomodoro personalizado", "Custom Pomodoro").to_string(),
            pick(
                "Foco/pausa/pausa-larga editables (1–180 min). Esc termina la sesión.",
                "Focus/break/long-break editable (1–180 min). Esc ends the session.",
            )
            .to_string(),
        );
        let f2 = feature(
            IconGlyph::Setup,
            pick("Categorías + Modo profundo", "Categories + Deep mode").to_string(),
            pick(
                "Etiqueta cada sesión y encadena foco sin descanso para bloques largos.",
                "Tag each session and chain focus without breaks for long blocks.",
            )
            .to_string(),
        );
        let f3 = feature(
            IconGlyph::Coach,
            pick("Coach IA local", "Local AI coach").to_string(),
            pick(
                "SmolLM2 1.7B + ~50 mensajes curados. Categoría + hora ajustan el tono.",
                "SmolLM2 1.7B + ~50 curated messages. Category + time of day shape the tone.",
            )
            .to_string(),
        );
        let f4 = feature(
            IconGlyph::Stats,
            pick("Sistema de semillas", "Seed reward system").to_string(),
            pick(
                "+1 por sesión válida, +1 si Atención ≥ 80%, +1 cada 4 sesiones, +N por plugins.",
                "+1 per valid session, +1 if Focus ≥ 80%, +1 every 4 sessions, +N from plugins.",
            )
            .to_string(),
        );
        let f5 = feature(
            IconGlyph::Setup,
            pick("Detección de ventanas", "Window detection").to_string(),
            pick(
                "Cada 10 s lee la ventana activa y avisa si está en la deny-list. Auto-pausa.",
                "Every 10 s reads the active window and alerts on deny-list matches. Auto-pauses.",
            )
            .to_string(),
        );
        let f6 = feature(
            IconGlyph::Setup,
            pick("Cámara: cara + celular", "Camera: face + phone").to_string(),
            pick(
                "YuNet detecta presencia; YOLOv8n caza el celular. Sin grabación, sin upload.",
                "YuNet detects presence; YOLOv8n catches your phone. No recording, no upload.",
            )
            .to_string(),
        );
        let f7 = feature(
            IconGlyph::Setup,
            pick("Calendario en vivo", "Live calendar").to_string(),
            pick(
                "EventKit muestra tu próximo evento como badge en Focus (macOS).",
                "EventKit shows your next event as a Focus-canvas badge (macOS).",
            )
            .to_string(),
        );
        let f8 = feature(
            IconGlyph::Setup,
            pick("Plugins TOML", "TOML plugins").to_string(),
            pick(
                "Carpeta plugins/*.toml extiende el clasificador y suma bonus de semillas.",
                "plugins/*.toml folder extends the classifier and stacks seed bonuses.",
            )
            .to_string(),
        );
        let f9 = feature(
            IconGlyph::Stats,
            pick("Stats + Export JSON/CSV", "Stats + JSON/CSV export").to_string(),
            pick(
                "Sesiones, atención, distracciones, semillas. Exporta tu historial cuando quieras.",
                "Sessions, focus, distractions, seeds. Export your history any time.",
            )
            .to_string(),
        );

        let feature_grid = column![
            iced::widget::row![f1, iced::widget::Space::with_width(SPACE_MD as f32), f2]
                .align_y(iced::alignment::Vertical::Top),
            iced::widget::row![f3, iced::widget::Space::with_width(SPACE_MD as f32), f4]
                .align_y(iced::alignment::Vertical::Top),
            iced::widget::row![f5, iced::widget::Space::with_width(SPACE_MD as f32), f6]
                .align_y(iced::alignment::Vertical::Top),
            iced::widget::row![f7, iced::widget::Space::with_width(SPACE_MD as f32), f8]
                .align_y(iced::alignment::Vertical::Top),
            f9, // odd one out
        ]
        .spacing(SPACE_MD as u16);

        // 6. Keyboard shortcuts — comprehensive, two-column key/value.
        let shortcut_row = |key: &'static str, action: String| -> Element<'_, Message> {
            iced::widget::row![
                container(text(key.to_string()).size(FONT_SMALL).color(TEXT_PRIMARY))
                    .padding([2, 8])
                    .width(Length::Fixed(120.0))
                    .style(|_| container::Style {
                        background: Some(iced::Background::Color(SURFACE_RAISED)),
                        border: iced::Border { radius: 4.0.into(), ..Default::default() },
                        ..Default::default()
                    }),
                iced::widget::Space::with_width(SPACE_SM as f32),
                text(action).size(FONT_SMALL).color(TEXT_SECONDARY),
            ]
            .align_y(iced::alignment::Vertical::Center)
            .into()
        };
        let shortcuts = settings_card_local(
            pick("Atajos de teclado", "Keyboard shortcuts"),
            column![
                shortcut_row("Space / P", pick("Pausar", "Pause").to_string()),
                shortcut_row("R", pick("Reanudar", "Resume").to_string()),
                shortcut_row("Esc", pick("Terminar sesión", "End session").to_string()),
                shortcut_row("B", pick("Tomar descanso", "Take break").to_string()),
                shortcut_row("S", pick("Abrir Setup", "Open Setup").to_string()),
                shortcut_row(
                    "C",
                    pick("Abrir Calibración", "Open Calibration").to_string(),
                ),
                shortcut_row(
                    "1 / 2 / 3 / 4",
                    pick("Focus / Stats / Coach / Setup", "Focus / Stats / Coach / Setup").to_string(),
                ),
                shortcut_row("5 / ?", pick("Ayuda", "Help").to_string()),
            ]
            .spacing(SPACE_XS as u16)
            .into(),
        );

        // 6.5 Calibración card — when to tune what.
        let calibration_card = settings_card_local(
            pick("¿Cómo afinar SolarFocus?", "How to tune SolarFocus"),
            column![
                text(pick(
                    "Si la app marca distracciones que no lo son → Setup → Calibración → sube Confianza mínima a 0.8 o usa el botón \"Falso positivo\" en el toast.",
                    "If the app flags non-distractions → Setup → Calibration → raise Min confidence to 0.8, or use the \"False positive\" button on the toast.",
                ))
                .size(FONT_SMALL)
                .color(TEXT_SECONDARY),
                text(pick(
                    "Si la cámara te pausa cuando sigues sentado → sube Muestras Absent a 5–7.",
                    "If the camera pauses you while you're still sitting there → raise Absent samples to 5–7.",
                ))
                .size(FONT_SMALL)
                .color(TEXT_SECONDARY),
                text(pick(
                    "Si tu celular no se detecta → baja Umbral YOLO a 0.30–0.35. Si te coge demasiados objetos como celular → sube a 0.55+.",
                    "If your phone isn't detected → lower YOLO threshold to 0.30–0.35. If it catches too many things as phone → raise to 0.55+.",
                ))
                .size(FONT_SMALL)
                .color(TEXT_SECONDARY),
                text(pick(
                    "Si el coach IA repite mensajes que no te sirven → vota 👎; el siguiente vendrá del banco curado durante el cooldown configurado (default 60 min).",
                    "If the AI coach gives messages you don't find helpful → vote 👎; the next one comes from the curated bank during the configured cooldown (default 60 min).",
                ))
                .size(FONT_SMALL)
                .color(TEXT_SECONDARY),
                text(pick(
                    "Pulsa \"Probar detección ahora\" en Calibración para una inferencia inmediata sin tener que esperar a una sesión.",
                    "Press \"Test detection now\" in Calibration for an immediate inference without waiting for a session.",
                ))
                .size(FONT_SMALL)
                .color(TEXT_MUTED),
            ]
            .spacing(SPACE_XS as u16)
            .into(),
        );

        // 7. Datos y privacidad card.
        let data_card = settings_card_local(
            pick("Datos y privacidad", "Data and privacy"),
            column![
                text(pick(
                    "Toda tu actividad se guarda en SQLite local (~/Library/Application Support/SolarFocus OS/solarfocus.db). Los modelos IA viven en la subcarpeta models/, los plugins en plugins/.",
                    "All activity lives in local SQLite (~/Library/Application Support/SolarFocus OS/solarfocus.db). AI models live in the models/ subfolder, plugins in plugins/.",
                ))
                .size(FONT_SMALL)
                .color(TEXT_SECONDARY),
                text(pick(
                    "Privacy → Exportar tus datos descarga JSON o CSV. Privacy → Borrar todos los datos elimina DB + modelos + ajustes en una sola acción.",
                    "Privacy → Export your data writes JSON or CSV. Privacy → Clear all data wipes DB + models + settings in one action.",
                ))
                .size(FONT_SMALL)
                .color(TEXT_SECONDARY),
            ]
            .spacing(SPACE_XS as u16)
            .into(),
        );

        // 8. Footer.
        let footer = iced::widget::row![
            text(format!("v{VERSION}")).size(FONT_TINY).color(TEXT_MUTED),
            iced::widget::Space::with_width(SPACE_SM as f32),
            text("·").size(FONT_TINY).color(TEXT_MUTED),
            iced::widget::Space::with_width(SPACE_SM as f32),
            text("Apache-2.0 / MIT").size(FONT_TINY).color(TEXT_MUTED),
            iced::widget::Space::with_width(SPACE_SM as f32),
            text("·").size(FONT_TINY).color(TEXT_MUTED),
            iced::widget::Space::with_width(SPACE_SM as f32),
            text(REPO_URL.to_string()).size(FONT_TINY).color(TEXT_MUTED),
        ];

        let body = column![
            identity,
            privacy_hero,
            cta_row,
            quick_start,
            feature_grid,
            shortcuts,
            calibration_card,
            data_card,
            footer,
        ]
        .spacing(SPACE_LG as u16)
        .padding(SPACE_XL as u16)
        .max_width(900);

        let centered = container(body)
            .width(Length::Fill)
            .center_x(Length::Fill);

        container(iced::widget::scrollable(centered))
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_| container::Style {
                background: Some(iced::Background::Color(BG)),
                ..Default::default()
            })
            .into()
    }
}
