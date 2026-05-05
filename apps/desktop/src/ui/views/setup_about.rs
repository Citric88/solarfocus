//! v1.12.2 — Setup → Acerca, full rewrite.
//!
//! Was: a single tiny card with hardcoded `v1.6.0` and a tagline.
//! Now: identity hero, current-release Novedades, live diagnostic
//! snapshot, build-flag matrix, license/repo, and library credits.
//! Dynamic version via `env!("CARGO_PKG_VERSION")` — never stale again.

use iced::widget::{column, container, text};
use iced::{Element, Length};
use solar_focus_intelligence::Language;

use crate::ui::components::{badge_local, settings_card_local, BadgeVariant};
use crate::ui::palette::*;
use crate::{App, Message};

const VERSION: &str = env!("CARGO_PKG_VERSION");

impl App {
    pub fn view_setup_about(&self) -> Element<'_, Message> {
        let lang = self.settings.language;
        let pick = |es: &'static str, en: &'static str| -> &'static str {
            if lang == Language::Es { es } else { en }
        };

        // ---- 1. Identity hero ----
        let identity_body: Element<'_, Message> = column![
            text("SolarFocus OS").size(FONT_TITLE).color(TEXT_PRIMARY),
            iced::widget::row![
                text(format!("v{VERSION}")).size(FONT_LEAD).color(TEXT_SECONDARY),
                iced::widget::Space::with_width(SPACE_SM as f32),
                badge_local(
                    pick("Estable", "Stable").to_string(),
                    BadgeVariant::Accent,
                ),
            ]
            .align_y(iced::alignment::Vertical::Center),
            text(pick(
                "Productividad enfocada con IA local. Privacidad por diseño.",
                "Focused productivity with local AI. Privacy by design.",
            ))
            .size(FONT_SMALL)
            .color(TEXT_SECONDARY),
            iced::widget::Space::with_height(SPACE_XS as f32),
            iced::widget::row![
                badge_local(
                    pick("Privacidad por diseño", "Privacy by design").to_string(),
                    BadgeVariant::Muted,
                ),
                iced::widget::Space::with_width(SPACE_XS as f32),
                badge_local(
                    pick("100% local", "100% local").to_string(),
                    BadgeVariant::Muted,
                ),
                iced::widget::Space::with_width(SPACE_XS as f32),
                badge_local(
                    pick("Open source", "Open source").to_string(),
                    BadgeVariant::Muted,
                ),
            ],
        ]
        .spacing(SPACE_XS as u16)
        .into();
        let identity_card = settings_card_local(
            pick("Identidad", "Identity"),
            identity_body,
        );

        // ---- 2. Novedades de v1.12.2 ----
        let novedades_body: Element<'_, Message> = column![
            text(pick("Sistema de semillas y jardín en Coach", "Seed reward + Coach garden"))
                .size(FONT_SMALL)
                .color(TEXT_PRIMARY),
            text(pick("Detector de celular en cámara con YOLOv8n", "Cell-phone camera detector via YOLOv8n"))
                .size(FONT_SMALL)
                .color(TEXT_PRIMARY),
            text(pick("Modo estudio profundo: sesiones encadenadas sin descanso", "Deep study mode: chained sessions without breaks"))
                .size(FONT_SMALL)
                .color(TEXT_PRIMARY),
            text(pick("Plugins TOML para extender clasificador y bonus de semillas", "TOML plugins extend classifier + seed bonuses"))
                .size(FONT_SMALL)
                .color(TEXT_PRIMARY),
            text(pick("Export JSON/CSV + validez de sesión por umbral de Atención", "JSON/CSV export + session validity threshold"))
                .size(FONT_SMALL)
                .color(TEXT_PRIMARY),
            text(pick("Stats con anillo de racha + chart de semillas + cosechas recientes", "Stats with streak ring + seed chart + recent harvests"))
                .size(FONT_SMALL)
                .color(TEXT_PRIMARY),
        ]
        .spacing(SPACE_XS as u16)
        .into();
        let novedades_card = settings_card_local(
            pick("Novedades de esta versión", "What's new in this version"),
            novedades_body,
        );

        // ---- 3. Estado actual (live diagnostic) ----
        let pair = |label: String, value: String| -> Element<'_, Message> {
            iced::widget::row![
                text(label).size(FONT_SMALL).color(TEXT_MUTED),
                iced::widget::Space::with_width(SPACE_SM as f32),
                text(value).size(FONT_SMALL).color(TEXT_PRIMARY),
            ]
            .align_y(iced::alignment::Vertical::Center)
            .into()
        };

        let lang_value = match lang {
            Language::Es => "Español".to_string(),
            Language::En => "English".to_string(),
        };
        let model_value = if cfg!(feature = "llm") {
            match (self.settings.ai_enabled, self.coach.is_ready()) {
                (true, true) => format!(
                    "{:?} ({})",
                    self.settings.model_choice,
                    pick("cargado", "loaded"),
                ),
                (true, false) => format!(
                    "{:?} ({})",
                    self.settings.model_choice,
                    pick("no descargado", "not downloaded"),
                ),
                _ => pick("desactivado", "disabled").to_string(),
            }
        } else {
            pick("build sin LLM", "build without LLM").to_string()
        };
        let classifier_value = format!("{:?}", self.settings.classifier_mode);
        let presence_value = {
            #[cfg(feature = "presence")]
            {
                if !self.settings.presence_enabled {
                    pick("desactivada", "disabled").to_string()
                } else {
                    match self.presence_probe.as_ref().map(|p| p.mode()) {
                        Some(crate::infra::presence::DetectionMode::YunetAndYoloPhone) => {
                            pick("YuNet + YOLOv8n", "YuNet + YOLOv8n").to_string()
                        }
                        Some(crate::infra::presence::DetectionMode::YunetFace) => "YuNet".to_string(),
                        Some(crate::infra::presence::DetectionMode::Brightness) => {
                            pick("Luminancia (sin modelo)", "Brightness (no model)").to_string()
                        }
                        None => pick("inicializando", "initializing").to_string(),
                    }
                }
            }
            #[cfg(not(feature = "presence"))]
            {
                pick("build sin cámara", "build without camera").to_string()
            }
        };
        let deep_value = if self.settings.deep_mode_enabled {
            pick("activado", "enabled").to_string()
        } else {
            pick("desactivado", "disabled").to_string()
        };
        let plugins_value = {
            let total = self.plugins.len();
            let active = self.plugins.iter().filter(|p| p.enabled).count();
            match lang {
                Language::Es => format!("{total} cargados ({active} activos)"),
                Language::En => format!("{total} loaded ({active} active)"),
            }
        };
        let attention_value = format!(
            "{}%",
            self.settings.min_attention_for_valid_session,
        );
        let phone_value = {
            #[cfg(feature = "presence")]
            {
                if crate::infra::yolo_download::is_present() {
                    pick("YOLOv8n descargado", "YOLOv8n downloaded").to_string()
                } else {
                    pick("no descargado", "not downloaded").to_string()
                }
            }
            #[cfg(not(feature = "presence"))]
            {
                pick("build sin cámara", "build without camera").to_string()
            }
        };

        let estado_body: Element<'_, Message> = column![
            pair(pick("Idioma", "Language").to_string(), lang_value),
            pair(pick("Modelo IA", "AI model").to_string(), model_value),
            pair(pick("Clasificador", "Classifier").to_string(), classifier_value),
            pair(pick("Detección de presencia", "Presence detection").to_string(), presence_value),
            pair(pick("Detector de celular", "Phone detector").to_string(), phone_value),
            pair(pick("Modo profundo", "Deep mode").to_string(), deep_value),
            pair(pick("Plugins", "Plugins").to_string(), plugins_value),
            pair(pick("Umbral de Atención", "Focus threshold").to_string(), attention_value),
        ]
        .spacing(SPACE_XS as u16)
        .into();
        let estado_card = settings_card_local(
            pick("Estado actual", "Current state"),
            estado_body,
        );

        // ---- 4. Compilación (build flags) ----
        let flag = |name: &'static str, on: bool| -> Element<'_, Message> {
            iced::widget::row![
                text(name.to_string()).size(FONT_SMALL).color(TEXT_PRIMARY),
                iced::widget::Space::with_width(SPACE_SM as f32),
                badge_local(
                    if on { "ON".to_string() } else { "OFF".to_string() },
                    if on { BadgeVariant::Accent } else { BadgeVariant::Muted },
                ),
            ]
            .align_y(iced::alignment::Vertical::Center)
            .into()
        };
        let llm_on = cfg!(feature = "llm");
        let classifier_on = cfg!(feature = "classifier");
        let presence_on = cfg!(feature = "presence");
        let calendar_on = cfg!(feature = "calendar");
        let gpu_metal_on = cfg!(feature = "gpu-metal");
        let gpu_cuda_on = cfg!(feature = "gpu-cuda");

        let build_body: Element<'_, Message> = column![
            iced::widget::row![
                flag("llm", llm_on),
                iced::widget::Space::with_width(SPACE_MD as f32),
                flag("classifier", classifier_on),
                iced::widget::Space::with_width(SPACE_MD as f32),
                flag("presence", presence_on),
            ],
            iced::widget::row![
                flag("calendar", calendar_on),
                iced::widget::Space::with_width(SPACE_MD as f32),
                flag("gpu-metal", gpu_metal_on),
                iced::widget::Space::with_width(SPACE_MD as f32),
                flag("gpu-cuda", gpu_cuda_on),
            ],
            iced::widget::Space::with_height(SPACE_XS as f32),
            text(format!("OS · {}", std::env::consts::OS))
                .size(FONT_TINY)
                .color(TEXT_MUTED),
            text(format!("ARCH · {}", std::env::consts::ARCH))
                .size(FONT_TINY)
                .color(TEXT_MUTED),
        ]
        .spacing(SPACE_SM as u16)
        .into();
        let build_card = settings_card_local(
            pick("Compilación", "Build"),
            build_body,
        );

        // ---- 5. Open source ----
        let oss_body: Element<'_, Message> = column![
            text("Apache-2.0 / MIT").size(FONT_SMALL).color(TEXT_PRIMARY),
            text("github.com/Citric88/solarfocus").size(FONT_SMALL).color(TEXT_PRIMARY),
            text(pick(
                "Contribuciones, issues y PRs son bienvenidos. El proyecto se mantiene de manera independiente.",
                "Contributions, issues and PRs are welcome. The project is maintained independently.",
            ))
            .size(FONT_TINY)
            .color(TEXT_MUTED),
        ]
        .spacing(SPACE_XS as u16)
        .into();
        let oss_card = settings_card_local(
            pick("Código abierto", "Open source"),
            oss_body,
        );

        // ---- 6. Reconocimientos ----
        let credit = |name: &'static str, what: &'static str| -> Element<'_, Message> {
            iced::widget::row![
                text(name.to_string()).size(FONT_TINY).color(TEXT_PRIMARY),
                iced::widget::Space::with_width(SPACE_SM as f32),
                text(what.to_string()).size(FONT_TINY).color(TEXT_MUTED),
            ]
            .align_y(iced::alignment::Vertical::Center)
            .into()
        };
        let credits_body: Element<'_, Message> = column![
            credit("iced", pick("framework de UI declarativo en Rust", "declarative Rust UI framework")),
            credit("ort", pick("runtime ONNX (YuNet + YOLOv8n + DistilBERT)", "ONNX runtime (YuNet + YOLOv8n + DistilBERT)")),
            credit("llama-cpp-2", pick("inferencia LLM en CPU/GPU local", "local CPU/GPU LLM inference")),
            credit("nokhwa", pick("captura de cámara cross-platform", "cross-platform camera capture")),
            credit("YuNet 2023mar", pick("detector facial (OpenCV Zoo)", "face detector (OpenCV Zoo)")),
            credit("YOLOv8n", pick("detector de objetos COCO (Ultralytics)", "COCO object detector (Ultralytics)")),
            credit("rusqlite + objc2-event-kit", pick("persistencia + EventKit en macOS", "persistence + macOS EventKit")),
        ]
        .spacing(SPACE_XS as u16)
        .into();
        let credits_card = settings_card_local(
            pick("Reconocimientos", "Acknowledgements"),
            credits_body,
        );

        let body = column![
            identity_card,
            novedades_card,
            estado_card,
            build_card,
            oss_card,
            credits_card,
        ]
        .spacing(SPACE_MD as u16)
        .max_width(720);

        container(body)
            .width(Length::Fill)
            .into()
    }
}
