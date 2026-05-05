//! v1.12.0 — Setup → Plugins tab. Lists discovered plugins from
//! `<data>/SolarFocus OS/plugins/*.toml`, shows their metadata, and
//! exposes a per-plugin enable/disable toggle plus a Reload button.

use iced::widget::{column, container, text};
use iced::{Element, Length};
use solar_focus_intelligence::Language;

use crate::ui::components::{badge_local, chip_local, settings_card_local, BadgeVariant};
use crate::ui::palette::*;
use crate::{App, Message};

impl App {
    pub fn view_setup_plugins(&self) -> Element<'_, Message> {
        let lang = self.settings.language;
        let pick = |es: &'static str, en: &'static str| -> &'static str {
            if lang == Language::Es { es } else { en }
        };

        let header = container(
            column![
                text(pick("Plugins", "Plugins"))
                    .size(FONT_BODY)
                    .color(TEXT_PRIMARY),
                text(pick(
                    "Plugins son archivos TOML en la carpeta de datos. Pueden extender la lista de distracciones del clasificador y otorgar bonus de semillas por categoría. No ejecutan código — el sandbox es declarativo por diseño.",
                    "Plugins are TOML files in the data folder. They can extend the classifier deny-list and award seed bonuses per category. They never execute code — the sandbox is declarative by design.",
                ))
                .size(FONT_SMALL)
                .color(TEXT_SECONDARY),
                text(format!(
                    "{}: {}",
                    pick("Carpeta", "Folder"),
                    crate::infra::plugins::plugins_dir().display()
                ))
                .size(FONT_TINY)
                .color(TEXT_MUTED),
                iced::widget::row![
                    chip_local(
                        pick("Recargar", "Reload").to_string(),
                        false,
                        Message::ReloadPlugins,
                    ),
                    iced::widget::Space::with_width(SPACE_SM as f32),
                    text(format!(
                        "{} {}",
                        self.plugins.len(),
                        pick("plugin(s) cargados", "plugin(s) loaded"),
                    ))
                    .size(FONT_SMALL)
                    .color(TEXT_MUTED),
                ]
                .align_y(iced::alignment::Vertical::Center),
            ]
            .spacing(SPACE_XS as u16),
        )
        .padding(SPACE_MD as u16)
        .style(|_| container::Style {
            background: Some(iced::Background::Color(SURFACE)),
            border: iced::Border {
                radius: 8.0.into(),
                width: 1.0,
                color: ACCENT_DIM,
            },
            ..Default::default()
        });

        let mut col = iced::widget::Column::new().spacing(SPACE_MD as u16);
        col = col.push(header);

        if self.plugins.is_empty() {
            let empty = container(
                column![
                    text(pick(
                        "Sin plugins instalados.",
                        "No plugins installed.",
                    ))
                    .size(FONT_BODY)
                    .color(TEXT_PRIMARY),
                    text(pick(
                        "Coloca un archivo .toml en la carpeta de plugins y pulsa Recargar. Cada plugin puede declarar [metadata], [classifier_rules.focus] / [classifier_rules.distraction] (procesos + palabras clave), y [seed_rules.category_bonus] (mapa categoría → semillas extra por sesión válida).",
                        "Drop a .toml file into the plugins folder and press Reload. Each plugin can declare [metadata], [classifier_rules.focus] / [classifier_rules.distraction] (processes + title keywords), and [seed_rules.category_bonus] (map category → extra seeds per valid session).",
                    ))
                    .size(FONT_SMALL)
                    .color(TEXT_SECONDARY),
                ]
                .spacing(SPACE_XS as u16),
            )
            .padding(SPACE_MD as u16)
            .style(|_| container::Style {
                background: Some(iced::Background::Color(SURFACE)),
                border: iced::Border {
                    radius: 8.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            });
            col = col.push(empty);
        } else {
            for p in &self.plugins {
                let id = p.id.clone();
                let enabled = p.enabled;
                let name = if p.file.metadata.name.is_empty() {
                    p.id.clone()
                } else {
                    p.file.metadata.name.clone()
                };
                let badge = if enabled {
                    badge_local(pick("Activo", "Active").to_string(), BadgeVariant::Accent)
                } else {
                    badge_local(pick("Inactivo", "Inactive").to_string(), BadgeVariant::Muted)
                };
                let toggle_label = if enabled {
                    pick("Desactivar", "Disable")
                } else {
                    pick("Activar", "Enable")
                };
                let rules_summary = match &p.file.classifier_rules {
                    Some(r) => format!(
                        "{}: +{} {} · +{} {} · +{} {} · +{} {}",
                        pick("Reglas", "Rules"),
                        r.focus.processes.len(),
                        pick("foco-proc", "focus-proc"),
                        r.focus.title_keywords.len(),
                        pick("foco-kw", "focus-kw"),
                        r.distraction.processes.len(),
                        pick("dist-proc", "distract-proc"),
                        r.distraction.title_keywords.len(),
                        pick("dist-kw", "distract-kw"),
                    ),
                    None => pick("Sin reglas", "No rules").to_string(),
                };
                let seed_summary = if p.file.seed_rules.category_bonus.is_empty() {
                    pick("Sin bonus de semillas", "No seed bonus").to_string()
                } else {
                    let parts: Vec<String> = p
                        .file
                        .seed_rules
                        .category_bonus
                        .iter()
                        .map(|(k, v)| format!("{k} +{v}"))
                        .collect();
                    format!("{}: {}", pick("Bonus", "Bonus"), parts.join(", "))
                };
                let body: Element<'_, Message> = column![
                    iced::widget::row![
                        text(name).size(FONT_BODY).color(TEXT_PRIMARY),
                        iced::widget::Space::with_width(SPACE_SM as f32),
                        badge,
                        iced::widget::horizontal_space(),
                        chip_local(
                            toggle_label.to_string(),
                            false,
                            Message::TogglePlugin(id, !enabled),
                        ),
                    ]
                    .align_y(iced::alignment::Vertical::Center),
                    text(format!(
                        "v{} · {}",
                        if p.file.metadata.version.is_empty() {
                            "—".to_string()
                        } else {
                            p.file.metadata.version.clone()
                        },
                        if p.file.metadata.author.is_empty() {
                            pick("autor desconocido", "unknown author").to_string()
                        } else {
                            p.file.metadata.author.clone()
                        }
                    ))
                    .size(FONT_TINY)
                    .color(TEXT_MUTED),
                    text(if p.file.metadata.description.is_empty() {
                        pick("(sin descripción)", "(no description)").to_string()
                    } else {
                        p.file.metadata.description.clone()
                    })
                    .size(FONT_SMALL)
                    .color(TEXT_SECONDARY),
                    text(rules_summary).size(FONT_TINY).color(TEXT_MUTED),
                    text(seed_summary).size(FONT_TINY).color(TEXT_MUTED),
                    text(format!(
                        "id: {} · {}",
                        p.id,
                        p.path
                            .file_name()
                            .and_then(|s| s.to_str())
                            .unwrap_or("?")
                    ))
                    .size(FONT_TINY)
                    .color(TEXT_MUTED),
                ]
                .spacing(2)
                .into();
                col = col.push(settings_card_local(
                    pick("Plugin", "Plugin"),
                    body,
                ));
            }
        }

        col.width(Length::Fill).into()
    }
}
