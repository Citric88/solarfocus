//! v1.6.0 — Setup → IA tab. AI toggle + window watch + model
//! picker + model status + classifier + optional DistilBERT
//! downloader + advanced collapsible + optional presence card.
//! Also hosts model_download_panel (shared with wizard).

use iced::widget::{column, container, text};
use iced::{Element, Length};

use crate::app::helpers::recommended_model_choice;
use crate::ui::components::{chip_local, settings_card_local};
use crate::ui::palette::*;
use crate::{App, Message};
use solar_focus_intelligence::Language;

#[cfg(feature = "llm")]
use std::sync::atomic::Ordering;

impl App {
    pub fn view_settings(&self) -> Element<'_, Message> {
        use crate::infra::settings::{ClassifierMode, ModelChoice};

        let lang = self.settings.language;
        let pick = |es: &'static str, en: &'static str| -> &'static str {
            if lang == Language::Es { es } else { en }
        };

        let ai_toggle_card = settings_card_local(
            pick("Coach IA", "AI Coach"),
            iced::widget::row![
                text(if self.settings.ai_enabled {
                    pick("Activado", "Enabled")
                } else {
                    pick("Desactivado", "Disabled")
                })
                .size(FONT_BODY)
                .color(TEXT_PRIMARY),
                iced::widget::horizontal_space(),
                chip_local(
                    if self.settings.ai_enabled {
                        pick("Desactivar", "Disable").to_string()
                    } else {
                        pick("Activar", "Enable").to_string()
                    },
                    false,
                    Message::ToggleAi(!self.settings.ai_enabled),
                ),
            ]
            .into(),
        );

        let watch_toggle_card = settings_card_local(
            pick("Vigilancia de ventana", "Window watch"),
            iced::widget::row![
                text(if self.settings.window_watch_enabled {
                    pick("Activada", "Enabled")
                } else {
                    pick("Desactivada", "Disabled")
                })
                .size(FONT_BODY)
                .color(TEXT_PRIMARY),
                iced::widget::horizontal_space(),
                chip_local(
                    if self.settings.window_watch_enabled {
                        pick("Desactivar", "Disable").to_string()
                    } else {
                        pick("Activar", "Enable").to_string()
                    },
                    false,
                    Message::ToggleWindowWatch(!self.settings.window_watch_enabled),
                ),
            ]
            .into(),
        );

        let recommended = recommended_model_choice();
        let mark = |c: ModelChoice, name: &str| -> String {
            if c == recommended { format!("{} ★", name) } else { name.to_string() }
        };
        let model_picker_body: Element<'_, Message> = column![
            iced::widget::row![
                text(format!("{:?}", self.settings.model_choice))
                    .size(FONT_BODY)
                    .color(TEXT_PRIMARY),
                iced::widget::horizontal_space(),
                chip_local(
                    mark(ModelChoice::SmolLM2, "SmolLM2"),
                    self.settings.model_choice == ModelChoice::SmolLM2,
                    Message::SetModelChoice(ModelChoice::SmolLM2),
                ),
                iced::widget::Space::with_width(SPACE_XS as f32),
                chip_local(
                    mark(ModelChoice::Llama1B, "Llama1B"),
                    self.settings.model_choice == ModelChoice::Llama1B,
                    Message::SetModelChoice(ModelChoice::Llama1B),
                ),
                iced::widget::Space::with_width(SPACE_XS as f32),
                chip_local(
                    mark(ModelChoice::Qwen15, "Qwen15"),
                    self.settings.model_choice == ModelChoice::Qwen15,
                    Message::SetModelChoice(ModelChoice::Qwen15),
                ),
            ],
            text(format!(
                "{} {:?} ({})",
                pick("Recomendado para tu hardware:", "Recommended for your hardware:"),
                recommended,
                if cfg!(all(target_arch = "aarch64", target_os = "macos")) {
                    "Apple Silicon"
                } else {
                    "general"
                },
            ))
            .size(FONT_TINY)
            .color(TEXT_MUTED),
        ]
        .spacing(SPACE_SM as u16)
        .into();
        let model_picker_card = settings_card_local(pick("Modelo IA", "AI model"), model_picker_body);

        let model_status_card = settings_card_local(
            pick("Estado del modelo", "Model status"),
            self.model_download_panel(),
        );

        let classifier_card = settings_card_local(
            pick("Clasificador", "Classifier"),
            iced::widget::row![
                text(format!("{:?}", self.settings.classifier_mode))
                    .size(FONT_BODY)
                    .color(TEXT_PRIMARY),
                iced::widget::horizontal_space(),
                chip_local(
                    "Mock".to_string(),
                    self.settings.classifier_mode == ClassifierMode::Mock,
                    Message::SetClassifierMode(ClassifierMode::Mock),
                ),
                iced::widget::Space::with_width(SPACE_XS as f32),
                chip_local(
                    "Rules".to_string(),
                    self.settings.classifier_mode == ClassifierMode::Rules,
                    Message::SetClassifierMode(ClassifierMode::Rules),
                ),
                iced::widget::Space::with_width(SPACE_XS as f32),
                chip_local(
                    "DistilBERT".to_string(),
                    self.settings.classifier_mode == ClassifierMode::Distilbert,
                    Message::SetClassifierMode(ClassifierMode::Distilbert),
                ),
            ]
            .into(),
        );

        let distilbert_card: Element<'_, Message> = if matches!(
            self.settings.classifier_mode,
            crate::infra::settings::ClassifierMode::Distilbert
        ) {
            #[cfg(feature = "classifier")]
            {
                let present = crate::infra::distilbert_download::is_present();
                let label = match (present, lang) {
                    (true, Language::Es) => "DistilBERT presente",
                    (true, Language::En) => "DistilBERT present",
                    (false, Language::Es) => "Descargar DistilBERT (~67 MB)",
                    (false, Language::En) => "Download DistilBERT (~67 MB)",
                };
                settings_card_local(
                    pick("DistilBERT", "DistilBERT"),
                    iced::widget::row![
                        text(label.to_string()).size(FONT_BODY).color(TEXT_PRIMARY),
                        iced::widget::horizontal_space(),
                        chip_local(
                            if present {
                                pick("Re-descargar", "Re-download").to_string()
                            } else {
                                pick("Descargar", "Download").to_string()
                            },
                            false,
                            Message::StartDistilbertDownload,
                        ),
                    ]
                    .into(),
                )
            }
            #[cfg(not(feature = "classifier"))]
            {
                settings_card_local(
                    pick("DistilBERT", "DistilBERT"),
                    text(pick(
                        "Requiere build con --features classifier",
                        "Requires build with --features classifier",
                    ))
                    .size(FONT_SMALL)
                    .color(TEXT_MUTED)
                    .into(),
                )
            }
        } else {
            iced::widget::Space::with_height(Length::Fixed(0.0)).into()
        };

        let advanced_header = iced::widget::row![
            text(pick("Avanzado", "Advanced"))
                .size(FONT_SMALL)
                .color(TEXT_MUTED),
            iced::widget::horizontal_space(),
            chip_local(
                if self.setup_show_advanced {
                    pick("Ocultar", "Hide").to_string()
                } else {
                    pick("Mostrar", "Show").to_string()
                },
                false,
                Message::ToggleSetupAdvanced,
            ),
        ];
        let advanced_card: Element<'_, Message> = if self.setup_show_advanced {
            container(
                column![
                    advanced_header,
                    text(format!(
                        "{}: conf={:.2}  ·  {}={}  ·  poll={}s",
                        pick("Umbral", "Threshold"),
                        self.settings.min_confidence,
                        pick("muestras consecutivas", "consecutive samples"),
                        self.settings.min_consecutive_samples,
                        self.settings.window_poll_secs,
                    ))
                    .size(FONT_TINY)
                    .color(TEXT_MUTED),
                    iced::widget::row![
                        chip_local(
                            pick("Generar resumen ahora", "Generate recap now").to_string(),
                            false,
                            Message::GenerateRecapNow,
                        ),
                    ],
                ]
                .spacing(SPACE_SM as u16),
            )
            .padding(SPACE_MD as u16)
            .style(|_| container::Style {
                background: Some(iced::Background::Color(SURFACE)),
                border: iced::Border {
                    radius: 8.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .into()
        } else {
            container(advanced_header)
                .padding(SPACE_MD as u16)
                .style(|_| container::Style {
                    background: Some(iced::Background::Color(SURFACE)),
                    border: iced::Border {
                        radius: 8.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                })
                .into()
        };

        #[cfg(feature = "presence")]
        let presence_card: Element<'_, Message> = {
            let body: Element<'_, Message> = if let Some(err) = &self.presence_error {
                column![
                    text(pick(
                        "No se pudo iniciar la cámara.",
                        "Camera initialization failed.",
                    ))
                    .size(FONT_BODY)
                    .color(DANGER),
                    text(err.clone()).size(FONT_TINY).color(TEXT_MUTED),
                    chip_local(
                        pick("Reintentar", "Retry").to_string(),
                        false,
                        Message::TogglePresence(true),
                    ),
                ]
                .spacing(SPACE_SM as u16)
                .into()
            } else {
                let status_label: String = match self.last_presence {
                    Some(crate::infra::presence::Presence::Present) => pick("Presente", "Present").to_string(),
                    Some(crate::infra::presence::Presence::Absent) => pick("Ausente", "Absent").to_string(),
                    Some(crate::infra::presence::Presence::Unknown) | None => {
                        pick("—", "—").to_string()
                    }
                };
                let yunet_present = crate::infra::yunet_download::is_present();
                let mode_label = match (self.presence_probe.as_ref(), yunet_present) {
                    (Some(p), _) => match p.mode() {
                        crate::infra::presence::DetectionMode::YunetFace =>
                            pick("Detección facial (YuNet)", "Face detection (YuNet)").to_string(),
                        crate::infra::presence::DetectionMode::Brightness =>
                            pick("Heurística por luminosidad", "Brightness heuristic").to_string(),
                    },
                    (None, true) => pick("YuNet listo (cámara desactivada)", "YuNet ready (camera off)").to_string(),
                    (None, false) => pick("Heurística por luminosidad (sin YuNet)", "Brightness heuristic (no YuNet)").to_string(),
                };
                let yunet_action: Element<'_, Message> = if yunet_present {
                    text(pick("✓ YuNet descargado (~337 KB)", "✓ YuNet downloaded (~337 KB)"))
                        .size(FONT_TINY)
                        .color(ACCENT)
                        .into()
                } else {
                    chip_local(
                        pick("Descargar YuNet (~337 KB)", "Download YuNet (~337 KB)").to_string(),
                        false,
                        Message::DownloadYunet,
                    )
                };
                column![
                    iced::widget::row![
                        text(if self.settings.presence_enabled {
                            pick("Activada", "Enabled")
                        } else {
                            pick("Desactivada", "Disabled")
                        })
                        .size(FONT_BODY)
                        .color(TEXT_PRIMARY),
                        iced::widget::horizontal_space(),
                        chip_local(
                            if self.settings.presence_enabled {
                                pick("Desactivar", "Disable").to_string()
                            } else {
                                pick("Activar", "Enable").to_string()
                            },
                            false,
                            Message::TogglePresence(!self.settings.presence_enabled),
                        ),
                    ],
                    text(format!(
                        "{}: {}",
                        pick("Estado", "Status"),
                        status_label
                    ))
                    .size(FONT_TINY)
                    .color(TEXT_MUTED),
                    text(format!(
                        "{}: {}",
                        pick("Modo", "Mode"),
                        mode_label,
                    ))
                    .size(FONT_TINY)
                    .color(TEXT_MUTED),
                    yunet_action,
                    text(pick(
                        "Sin YuNet: detección por cambios bruscos de luz. Con YuNet: detección facial real (foto se descarta tras la inferencia).",
                        "Without YuNet: detection via sharp light swings. With YuNet: actual face detection (frame discarded after inference).",
                    ))
                    .size(FONT_TINY)
                    .color(TEXT_MUTED),
                ]
                .spacing(SPACE_SM as u16)
                .into()
            };
            settings_card_local(pick("Detección de presencia", "Presence detection"), body)
        };

        let mut card_col = iced::widget::Column::new().spacing(SPACE_MD as u16);
        card_col = card_col
            .push(ai_toggle_card)
            .push(watch_toggle_card)
            .push(model_picker_card)
            .push(model_status_card)
            .push(classifier_card)
            .push(distilbert_card);
        #[cfg(feature = "presence")]
        {
            card_col = card_col.push(presence_card);
        }
        card_col = card_col.push(advanced_card);
        let content = card_col.max_width(640);

        content.into()
    }

    pub(crate) fn model_download_panel(&self) -> Element<'_, Message> {
        #[cfg(not(feature = "llm"))]
        {
            return container(
                text(match self.settings.language {
                    Language::Es =>
                        "Esta build no incluye el LLM. Recompila con --features llm para activarlo.",
                    Language::En =>
                        "This build was compiled without the LLM. Rebuild with --features llm to enable.",
                })
                .size(FONT_SMALL)
                .color(TEXT_MUTED),
            )
            .padding(SPACE_MD as u16)
            .style(|_| container::Style {
                background: Some(iced::Background::Color(SURFACE)),
                border: iced::Border { radius: 8.0.into(), ..Default::default() },
                ..Default::default()
            })
            .into();
        }

        #[cfg(feature = "llm")]
        {
            use crate::infra::model_download::{manifest_for, model_present};

            let Some(manifest) = manifest_for(self.settings.model_choice) else {
                return text("(modelo desconocido)").size(FONT_SMALL).color(TEXT_MUTED).into();
            };

            let snap = self.download_progress.lock().unwrap().clone();
            let downloading = self.download_active.load(Ordering::Relaxed);

            if downloading {
                if let Some(s) = snap {
                    let pct = if s.total > 0 {
                        (s.downloaded as f64 / s.total as f64).min(1.0)
                    } else {
                        0.0
                    };
                    let mb_done = s.downloaded as f64 / 1_048_576.0;
                    let mb_total = s.total as f64 / 1_048_576.0;
                    let kbps = s.bytes_per_sec / 1024;
                    let is_resume = s.downloaded > 0 && s.bytes_per_sec == 0;
                    let label = if s.verifying {
                        match self.settings.language {
                            Language::Es => "Verificando…".to_string(),
                            Language::En => "Verifying…".to_string(),
                        }
                    } else if is_resume {
                        match self.settings.language {
                            Language::Es => format!("Reanudando desde {:.1} MB", mb_done),
                            Language::En => format!("Resuming from {:.1} MB", mb_done),
                        }
                    } else {
                        format!("{:.1}/{:.1} MB · {} KB/s", mb_done, mb_total, kbps)
                    };
                    return container(
                        column![
                            text(manifest.filename.to_string())
                                .size(FONT_SMALL)
                                .color(TEXT_MUTED),
                            iced::widget::progress_bar(0.0..=1.0, pct as f32)
                                .width(Length::Fixed(420.0)),
                            iced::widget::row![
                                text(label).size(FONT_SMALL).color(TEXT_SECONDARY),
                                iced::widget::horizontal_space(),
                                iced::widget::button(text(match self.settings.language {
                                    Language::Es => "Cancelar",
                                    Language::En => "Cancel",
                                }).size(FONT_SMALL))
                                .on_press(Message::CancelDownload)
                                .padding([4, 12])
                                .style(|_, _| iced::widget::button::Style {
                                    background: Some(iced::Background::Color(SURFACE_RAISED)),
                                    text_color: DANGER,
                                    border: iced::Border {
                                        color: DANGER,
                                        width: 1.0,
                                        radius: 4.0.into(),
                                    },
                                    ..Default::default()
                                }),
                            ],
                        ]
                        .spacing(SPACE_SM as u16),
                    )
                    .padding(SPACE_MD as u16)
                    .style(|_| container::Style {
                        background: Some(iced::Background::Color(SURFACE)),
                        border: iced::Border { radius: 8.0.into(), ..Default::default() },
                        ..Default::default()
                    })
                    .into();
                }
            }

            if self.model_present_cache.unwrap_or_else(|| model_present(manifest)) {
                return container(
                    column![
                        text(format!(
                            "{} {}",
                            match self.settings.language {
                                Language::Es => "✓ Modelo presente:",
                                Language::En => "✓ Model present:",
                            },
                            manifest.filename
                        ))
                        .size(FONT_BODY)
                        .color(ACCENT),
                        text(format!(
                            "{:.1} MB",
                            manifest.size_bytes as f64 / 1_048_576.0
                        ))
                        .size(FONT_SMALL)
                        .color(TEXT_MUTED),
                        iced::widget::Space::with_height(SPACE_SM as f32),
                        iced::widget::row![
                            iced::widget::button(text(match self.settings.language {
                                Language::Es => "Re-descargar",
                                Language::En => "Re-download",
                            }))
                            .on_press(Message::StartModelDownload)
                            .padding([6, 18]),
                            iced::widget::Space::with_width(SPACE_SM as f32),
                            iced::widget::button(text(match self.settings.language {
                                Language::Es => "Eliminar",
                                Language::En => "Delete",
                            }))
                            .on_press(Message::DeleteModel)
                            .padding([6, 18])
                            .style(|_, _| iced::widget::button::Style {
                                background: Some(iced::Background::Color(DANGER)),
                                text_color: BG,
                                border: iced::Border { radius: 6.0.into(), ..Default::default() },
                                ..Default::default()
                            }),
                        ],
                    ]
                    .spacing(2)
                    .padding(SPACE_SM as u16),
                )
                .padding(SPACE_MD as u16)
                .style(|_| container::Style {
                    background: Some(iced::Background::Color(SURFACE)),
                    border: iced::Border { radius: 8.0.into(), ..Default::default() },
                    ..Default::default()
                })
                .into();
            }

            let err = self.download_error.clone();
            container(
                column![
                    text(format!(
                        "{} {} (~{} MB)",
                        match self.settings.language {
                            Language::Es => "Modelo:",
                            Language::En => "Model:",
                        },
                        manifest.filename,
                        manifest.size_bytes / 1_048_576,
                    ))
                    .size(FONT_BODY)
                    .color(TEXT_PRIMARY),
                    text(match self.settings.language {
                        Language::Es => "Todo el procesamiento ocurre en tu equipo.",
                        Language::En => "All processing happens on your machine.",
                    })
                    .size(FONT_SMALL)
                    .color(TEXT_MUTED),
                    iced::widget::row![
                        iced::widget::button(text(match self.settings.language {
                            Language::Es => "Descargar",
                            Language::En => "Download",
                        }))
                        .on_press(Message::StartModelDownload)
                        .padding([8, 18])
                        .style(|_, _| iced::widget::button::Style {
                            background: Some(iced::Background::Color(ACCENT)),
                            text_color: BG,
                            border: iced::Border { radius: 6.0.into(), ..Default::default() },
                            ..Default::default()
                        }),
                        iced::widget::Space::with_width(SPACE_SM as f32),
                        iced::widget::button(text(match self.settings.language {
                            Language::Es => "Saltar",
                            Language::En => "Skip",
                        }))
                        .on_press(Message::SkipModelDownload)
                        .padding([8, 18]),
                    ],
                    if let Some(e) = err {
                        text(format!("Error: {}", e)).size(FONT_SMALL).color(DANGER).into()
                    } else {
                        Element::from(iced::widget::Space::with_height(Length::Fixed(0.0)))
                    },
                ]
                .spacing(SPACE_SM as u16),
            )
            .padding(SPACE_MD as u16)
            .style(|_| container::Style {
                background: Some(iced::Background::Color(SURFACE)),
                border: iced::Border { radius: 8.0.into(), ..Default::default() },
                ..Default::default()
            })
            .into()
        }
    }
}
