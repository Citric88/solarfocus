//! v1.6.0 — Setup → Privacidad tab: privacy banner, screen-recording
//! permission card, distraction-detection transparency explainer,
//! optional presence transparency (when `presence` feature on),
//! danger-zone two-step destructive confirm.

use iced::widget::{column, container, text};
use iced::Element;

use crate::ui::components::{destructive_button, ghost_button};
use crate::ui::palette::*;
use crate::{App, Message, PermissionStatus};
use solar_focus_intelligence::Language;

impl App {
    pub fn view_setup_privacy(&self) -> Element<'_, Message> {
        let copy_es = "SolarFocus procesa todo localmente. Tu actividad no sale del equipo. \
                       Los modelos IA (cuando se descargan) corren en tu hardware.";
        let copy_en = "SolarFocus processes everything locally. Your activity never leaves your machine. \
                       AI models (when downloaded) run on your own hardware.";

        let banner = container(
            column![
                text(match self.settings.language {
                    Language::Es => "Privacidad",
                    Language::En => "Privacy",
                })
                .size(FONT_LEAD)
                .color(TEXT_PRIMARY),
                text(if self.settings.language == Language::Es {
                    copy_es
                } else {
                    copy_en
                })
                .size(FONT_SMALL)
                .color(TEXT_SECONDARY),
            ]
            .spacing(SPACE_SM as u16),
        )
        .padding(SPACE_MD as u16)
        .style(|_| container::Style {
            background: Some(iced::Background::Color(SURFACE)),
            border: iced::Border {
                color: ACCENT_DIM,
                width: 1.0,
                radius: 8.0.into(),
            },
            ..Default::default()
        });

        let (badge_color, status_text_es, status_text_en) = match self.permission_status {
            PermissionStatus::Granted => (ACCENT, "Concedido", "Granted"),
            PermissionStatus::NameOnly => (WARNING, "Parcial (solo procesos)", "Partial (process names only)"),
            PermissionStatus::Denied => (DANGER, "Denegado", "Denied"),
            PermissionStatus::Unknown => (TEXT_MUTED, "Verificando…", "Checking…"),
        };
        let perm = container(
            column![
                text(match self.settings.language {
                    Language::Es => "Permiso de Grabación de Pantalla (macOS)",
                    Language::En => "Screen Recording permission (macOS)",
                })
                .size(FONT_SMALL)
                .color(TEXT_MUTED),
                iced::widget::row![
                    text("●").size(FONT_LEAD).color(badge_color),
                    iced::widget::Space::with_width(SPACE_SM as f32),
                    text(if self.settings.language == Language::Es {
                        status_text_es
                    } else {
                        status_text_en
                    })
                    .size(FONT_BODY)
                    .color(TEXT_PRIMARY),
                    iced::widget::horizontal_space(),
                    iced::widget::button(
                        text(match self.settings.language {
                            Language::Es => "Abrir Ajustes del sistema",
                            Language::En => "Open System Settings",
                        })
                        .size(FONT_SMALL),
                    )
                    .on_press(Message::OpenSystemSettings)
                    .padding([6, 14])
                    .style(|_, _| iced::widget::button::Style {
                        background: Some(iced::Background::Color(SURFACE_RAISED)),
                        text_color: TEXT_PRIMARY,
                        border: iced::Border {
                            radius: 6.0.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    }),
                    iced::widget::Space::with_width(SPACE_XS as f32),
                    iced::widget::button(text(match self.settings.language {
                        Language::Es => "Re-verificar",
                        Language::En => "Re-check",
                    }).size(FONT_SMALL))
                    .on_press(Message::ProbePermission)
                    .padding([6, 14]),
                ],
            ]
            .spacing(SPACE_SM as u16),
        )
        .padding(SPACE_MD as u16)
        .style(|_| container::Style {
            background: Some(iced::Background::Color(SURFACE)),
            border: iced::Border {
                radius: 6.0.into(),
                ..Default::default()
            },
            ..Default::default()
        });

        let danger_zone: Element<'_, Message> = if self.confirming_clear {
            container(
                column![
                    text(match self.settings.language {
                        Language::Es => "¿Seguro? Esto borrará la base de datos, los ajustes y los modelos descargados.",
                        Language::En => "Are you sure? This will erase the database, settings, and any downloaded models.",
                    })
                    .size(FONT_SMALL)
                    .color(DANGER),
                    iced::widget::row![
                        destructive_button(
                            match self.settings.language {
                                Language::Es => "Sí, borrar todo".to_string(),
                                Language::En => "Yes, clear all".to_string(),
                            },
                            Message::ConfirmClearData,
                        ),
                        iced::widget::Space::with_width(SPACE_SM as f32),
                        ghost_button(
                            match self.settings.language {
                                Language::Es => "Cancelar",
                                Language::En => "Cancel",
                            },
                            Message::CancelClearData,
                        ),
                    ],
                ]
                .spacing(SPACE_SM as u16),
            )
            .padding(SPACE_MD as u16)
            .style(|_| container::Style {
                background: Some(iced::Background::Color(SURFACE_RAISED)),
                border: iced::Border {
                    color: DANGER,
                    width: 1.0,
                    radius: 6.0.into(),
                },
                ..Default::default()
            })
            .into()
        } else {
            container(
                iced::widget::row![
                    column![
                        text(match self.settings.language {
                            Language::Es => "Zona peligrosa",
                            Language::En => "Danger zone",
                        })
                        .size(FONT_SMALL)
                        .color(DANGER),
                        text(match self.settings.language {
                            Language::Es =>
                                "Elimina todos los datos locales (DB, ajustes, modelos).",
                            Language::En =>
                                "Erase all local data (DB, settings, models).",
                        })
                        .size(FONT_SMALL)
                        .color(TEXT_SECONDARY),
                    ]
                    .spacing(2),
                    iced::widget::horizontal_space(),
                    destructive_button(
                        match self.settings.language {
                            Language::Es => "Borrar todos los datos".to_string(),
                            Language::En => "Clear all data".to_string(),
                        },
                        Message::RequestClearData,
                    ),
                ]
                .padding(SPACE_SM as u16),
            )
            .padding(SPACE_MD as u16)
            .style(|_| container::Style {
                background: Some(iced::Background::Color(SURFACE)),
                border: iced::Border {
                    radius: 6.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .into()
        };

        let transparency = container(
            column![
                text(match self.settings.language {
                    Language::Es => "¿Cómo se detectan las distracciones?",
                    Language::En => "How are distractions detected?",
                })
                .size(FONT_BODY)
                .color(TEXT_PRIMARY),
                text(match self.settings.language {
                    Language::Es =>
                        "Cada 10 segundos, SolarFocus le pregunta al sistema operativo \
                         qué ventana está activa. macOS responde con dos cosas:",
                    Language::En =>
                        "Every 10 seconds, SolarFocus asks the operating system \
                         which window is active. macOS responds with two things:",
                })
                .size(FONT_SMALL)
                .color(TEXT_SECONDARY),
                text(match self.settings.language {
                    Language::Es =>
                        "  ·  Nombre del proceso (ej. \"Code\", \"Safari\", \"TikTok\")",
                    Language::En =>
                        "  ·  Process name (e.g. \"Code\", \"Safari\", \"TikTok\")",
                })
                .size(FONT_SMALL)
                .color(TEXT_PRIMARY),
                text(match self.settings.language {
                    Language::Es =>
                        "  ·  Título de la ventana (ej. \"Cool video — youtube.com/watch?v=abc\") \
                         — solo si concedes Grabación de Pantalla",
                    Language::En =>
                        "  ·  Window title (e.g. \"Cool video — youtube.com/watch?v=abc\") \
                         — only if you grant Screen Recording",
                })
                .size(FONT_SMALL)
                .color(TEXT_PRIMARY),
                iced::widget::Space::with_height(SPACE_XS as f32),
                text(match self.settings.language {
                    Language::Es =>
                        "Esos textos se comparan contra una lista local de procesos y \
                         palabras clave (TikTok, Instagram, youtube.com/watch, etc.). \
                         Si coinciden 2 veces seguidas con confianza ≥ 70%, aparece un aviso.",
                    Language::En =>
                        "Those texts are compared against a local list of processes and \
                         keywords (TikTok, Instagram, youtube.com/watch, etc.). \
                         If they match 2 times in a row with ≥ 70% confidence, an alert appears.",
                })
                .size(FONT_SMALL)
                .color(TEXT_SECONDARY),
                iced::widget::Space::with_height(SPACE_XS as f32),
                text(match self.settings.language {
                    Language::Es =>
                        "Lo que NO hacemos: capturar pantalla, leer contenido de páginas, \
                         enviar nada por la red, ni guardar el título en la base de datos. \
                         La detección es 100% local y se descarta inmediatamente después.",
                    Language::En =>
                        "What we DO NOT do: take screenshots, read page contents, \
                         send anything over the network, or save the title to the database. \
                         Detection is 100% local and discarded immediately after.",
                })
                .size(FONT_TINY)
                .color(TEXT_MUTED),
            ]
            .spacing(SPACE_XS as u16),
        )
        .padding(SPACE_MD as u16)
        .style(|_| container::Style {
            background: Some(iced::Background::Color(SURFACE)),
            border: iced::Border {
                radius: 6.0.into(),
                ..Default::default()
            },
            ..Default::default()
        });

        #[cfg(feature = "presence")]
        let presence_transparency: Element<'_, Message> = container(
            column![
                text(match self.settings.language {
                    Language::Es => "¿Cómo funciona la detección de presencia?",
                    Language::En => "How does presence detection work?",
                })
                .size(FONT_BODY)
                .color(TEXT_PRIMARY),
                text(match self.settings.language {
                    Language::Es => "Cuando la activas en Setup → IA, SolarFocus OS pide \
                                     permiso de Cámara. Una vez al segundo durante una \
                                     sesión de foco activa:",
                    Language::En => "When you turn it on in Setup → AI, SolarFocus OS \
                                     requests Camera permission. Once per second during \
                                     an active focus session:",
                })
                .size(FONT_SMALL)
                .color(TEXT_SECONDARY),
                text(match self.settings.language {
                    Language::Es => "  ·  Captura un frame en escala de grises (320×240).",
                    Language::En => "  ·  Captures one grayscale frame (320×240).",
                })
                .size(FONT_SMALL)
                .color(TEXT_PRIMARY),
                text(match self.settings.language {
                    Language::Es => "  ·  Calcula la luminancia promedio del frame.",
                    Language::En => "  ·  Computes the frame's average luminance.",
                })
                .size(FONT_SMALL)
                .color(TEXT_PRIMARY),
                text(match self.settings.language {
                    Language::Es => "  ·  Compara contra el frame anterior. Si la luz cambia \
                                     bruscamente (te alejaste, apagaste la luz), marca \
                                     'Ausente'. El frame se descarta inmediatamente.",
                    Language::En => "  ·  Compares against the previous frame. A sharp \
                                     swing (you stepped away, lights off) flags 'Absent'. \
                                     The frame is dropped immediately.",
                })
                .size(FONT_SMALL)
                .color(TEXT_PRIMARY),
                iced::widget::Space::with_height(SPACE_XS as f32),
                text(match self.settings.language {
                    Language::Es => "Tras 3 muestras consecutivas marcadas 'Ausente', \
                                     la sesión se pausa automáticamente.",
                    Language::En => "After 3 consecutive 'Absent' samples, the session \
                                     auto-pauses.",
                })
                .size(FONT_SMALL)
                .color(TEXT_SECONDARY),
                iced::widget::Space::with_height(SPACE_XS as f32),
                text(match self.settings.language {
                    Language::Es => "Lo que NO hacemos: grabar video, guardar imágenes, \
                                     hacer reconocimiento facial, identificar personas, \
                                     ni enviar nada por la red. La cámara solo se enciende \
                                     mientras una sesión está activa y la opción está activada.",
                    Language::En => "What we DO NOT do: record video, save images, do \
                                     facial recognition, identify people, or send anything \
                                     over the network. The camera only turns on while a \
                                     session is active and the option is enabled.",
                })
                .size(FONT_TINY)
                .color(TEXT_MUTED),
            ]
            .spacing(SPACE_XS as u16),
        )
        .padding(SPACE_MD as u16)
        .style(|_| container::Style {
            background: Some(iced::Background::Color(SURFACE)),
            border: iced::Border {
                radius: 6.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into();

        let mut col = iced::widget::Column::new().spacing(SPACE_MD as u16);
        col = col.push(banner).push(perm).push(transparency);
        #[cfg(feature = "presence")]
        {
            col = col.push(presence_transparency);
        }
        col = col.push(danger_zone);
        col.into()
    }
}
