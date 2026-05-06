//! v1.13.0 — Setup → Calibración tab.
//!
//! Surfaces every tunable AI threshold as a visible slider with copy
//! explaining what each knob does. Adds a "Probar detección ahora"
//! card that fires one-shot tests against the live window probe and
//! camera + an audit list of the auto-generated user-exceptions
//! plugin.

use iced::widget::{button, column, container, slider, text};
use iced::{Element, Length};
use solar_focus_intelligence::Language;

use crate::ui::components::{badge_local, settings_card_local, BadgeVariant};
use crate::ui::palette::*;
use crate::{App, Message};

impl App {
    pub fn view_setup_calibration(&self) -> Element<'_, Message> {
        let lang = self.settings.language;
        let pick = |es: &'static str, en: &'static str| -> &'static str {
            if lang == Language::Es { es } else { en }
        };

        // v1.13.0 — when the guided wizard is active, replace the slider
        // grid with the wizard panel.
        if self.calibration_wizard.is_some() {
            return self.view_calibration_wizard();
        }

        // ---------- Probar detección ahora ----------
        let test_btn = |label: &str, msg: Message| -> Element<'_, Message> {
            button(text(label.to_string()).size(FONT_SMALL).color(BG))
                .on_press(msg)
                .padding([6, 14])
                .style(|_, _| iced::widget::button::Style {
                    background: Some(iced::Background::Color(ACCENT)),
                    text_color: BG,
                    border: iced::Border {
                        radius: 6.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                })
                .into()
        };

        let window_result_str: String = match &self.last_window_test {
            Some((proc, rule, conf, label)) => {
                let rule_str = rule.clone().unwrap_or_else(|| "—".to_string());
                match lang {
                    Language::Es => format!(
                        "Proceso: {proc} · regla: {rule_str} · confianza: {:.2} · {:?}",
                        conf, label
                    ),
                    Language::En => format!(
                        "Process: {proc} · rule: {rule_str} · confidence: {:.2} · {:?}",
                        conf, label
                    ),
                }
            }
            None => pick(
                "Pulsa 'Probar ventana' para evaluar tu ventana activa.",
                "Press 'Test window' to evaluate your active window.",
            )
            .to_string(),
        };

        #[cfg(feature = "presence")]
        let face_result_str: String = match &self.last_face_test {
            Some((p, score)) => match lang {
                Language::Es => format!("Veredicto: {:?} · score: {:.2}", p, score),
                Language::En => format!("Verdict: {:?} · score: {:.2}", p, score),
            },
            None => pick(
                "Pulsa 'Probar cara' para una inferencia inmediata.",
                "Press 'Test face' for an immediate inference.",
            )
            .to_string(),
        };
        #[cfg(not(feature = "presence"))]
        let face_result_str: String = pick(
            "Build sin cámara — recompila con --features presence.",
            "Build without camera — rebuild with --features presence.",
        )
        .to_string();

        #[cfg(feature = "presence")]
        let phone_result_str: String = match &self.last_phone_test {
            Some(score) => {
                let verdict = if *score >= self.settings.phone_conf_min {
                    pick("celular detectado", "phone detected")
                } else {
                    pick("sin celular", "no phone")
                };
                format!("Score: {:.2} · {verdict}", score)
            }
            None => pick(
                "Pulsa 'Probar celular' para una inferencia inmediata.",
                "Press 'Test phone' for an immediate inference.",
            )
            .to_string(),
        };
        #[cfg(not(feature = "presence"))]
        let phone_result_str: String = pick(
            "Build sin cámara — recompila con --features presence.",
            "Build without camera — rebuild with --features presence.",
        )
        .to_string();

        let test_row = iced::widget::row![
            test_btn(pick("Probar ventana", "Test window"), Message::TestWindowDetection),
            iced::widget::Space::with_width(SPACE_SM as f32),
            {
                #[cfg(feature = "presence")]
                {
                    test_btn(pick("Probar cara", "Test face"), Message::TestFaceDetection)
                }
                #[cfg(not(feature = "presence"))]
                {
                    let elem: Element<'_, Message> = text("").into();
                    elem
                }
            },
            iced::widget::Space::with_width(SPACE_SM as f32),
            {
                #[cfg(feature = "presence")]
                {
                    test_btn(pick("Probar celular", "Test phone"), Message::TestPhoneDetection)
                }
                #[cfg(not(feature = "presence"))]
                {
                    let elem: Element<'_, Message> = text("").into();
                    elem
                }
            },
        ];

        let test_card = settings_card_local(
            pick("Probar detección ahora", "Test detection now"),
            column![
                text(pick(
                    "Dispara una inferencia inmediata sin tener que iniciar una sesión.",
                    "Fire an immediate inference without starting a session.",
                ))
                .size(FONT_SMALL)
                .color(TEXT_SECONDARY),
                test_row,
                iced::widget::Space::with_height(SPACE_XS as f32),
                text(format!(
                    "{}: {}",
                    pick("Ventana", "Window"),
                    window_result_str
                ))
                .size(FONT_SMALL)
                .color(TEXT_PRIMARY),
                text(format!(
                    "{}: {}",
                    pick("Cara", "Face"),
                    face_result_str
                ))
                .size(FONT_SMALL)
                .color(TEXT_PRIMARY),
                text(format!(
                    "{}: {}",
                    pick("Celular", "Phone"),
                    phone_result_str
                ))
                .size(FONT_SMALL)
                .color(TEXT_PRIMARY),
            ]
            .spacing(SPACE_XS as u16)
            .into(),
        );

        // ---------- Slider helper ----------
        let slider_card = |label: String,
                           description: String,
                           value: f32,
                           range: std::ops::RangeInclusive<f32>,
                           step: f32,
                           default_value: f32,
                           formatted: String,
                           on_change: fn(f32) -> Message|
         -> Element<'_, Message> {
            let is_default = (value - default_value).abs() < 1e-3;
            let badge: Element<'_, Message> = if is_default {
                badge_local(pick("recomendado", "recommended").to_string(), BadgeVariant::Accent)
            } else {
                badge_local(
                    format!(
                        "{} {:.2}",
                        pick("default", "default"),
                        default_value
                    ),
                    BadgeVariant::Muted,
                )
            };
            settings_card_local(
                Box::leak(label.into_boxed_str()) as &'static str,
                column![
                    text(description).size(FONT_SMALL).color(TEXT_SECONDARY),
                    iced::widget::row![
                        slider(range, value, on_change).step(step).width(Length::Fixed(360.0)),
                        iced::widget::Space::with_width(SPACE_SM as f32),
                        text(formatted).size(FONT_BODY).color(TEXT_PRIMARY),
                        iced::widget::Space::with_width(SPACE_SM as f32),
                        badge,
                    ]
                    .align_y(iced::alignment::Vertical::Center),
                ]
                .spacing(SPACE_XS as u16)
                .into(),
            )
        };

        // ---------- 5 sliders ----------
        let confidence_card = slider_card(
            pick(
                "Confianza mínima del clasificador",
                "Classifier minimum confidence",
            )
            .to_string(),
            pick(
                "Sube si la app marca distracciones que no lo son. Baja si se le pasan distracciones obvias.",
                "Raise if the app flags non-distractions. Lower if it misses obvious ones.",
            )
            .to_string(),
            self.settings.min_confidence,
            0.30..=1.0,
            0.05,
            0.7,
            format!("{:.2}", self.settings.min_confidence),
            Message::SetMinConfidence,
        );

        let samples_card = {
            let value = self.settings.min_consecutive_samples as f32;
            slider_card(
                pick(
                    "Muestras consecutivas para confirmar distracción",
                    "Consecutive samples to confirm distraction",
                )
                .to_string(),
                pick(
                    "Cuántas muestras seguidas en la deny-list antes de pausar. Más alto = menos falsos positivos por cambio rápido de pestaña.",
                    "How many consecutive deny-list hits before pausing. Higher = fewer false positives from quick tab switches.",
                )
                .to_string(),
                value,
                1.0..=5.0,
                1.0,
                2.0,
                format!("{}", self.settings.min_consecutive_samples),
                |v| Message::SetMinConsecutiveSamples(v.round() as u8),
            )
        };

        let absent_card = {
            let value = self.settings.presence_absent_threshold as f32;
            slider_card(
                pick(
                    "Muestras Absent antes de auto-pausar (cámara)",
                    "Absent samples before auto-pausing (camera)",
                )
                .to_string(),
                pick(
                    "Cuántas muestras seguidas marcadas Absent antes de que la sesión se pause sola.",
                    "How many consecutive Absent samples before the session auto-pauses.",
                )
                .to_string(),
                value,
                1.0..=10.0,
                1.0,
                3.0,
                format!("{}", self.settings.presence_absent_threshold),
                |v| Message::SetPresenceAbsentThreshold(v.round() as u8),
            )
        };

        let phone_card = slider_card(
            pick(
                "Umbral YOLO para detectar celular",
                "YOLO threshold for phone detection",
            )
            .to_string(),
            pick(
                "Más bajo = detecta el celular antes pero hay más falsos positivos. Más alto = solo lo coge cuando es muy claro.",
                "Lower = catches the phone earlier but more false positives. Higher = only fires when very clear.",
            )
            .to_string(),
            self.settings.phone_conf_min,
            0.30..=0.80,
            0.05,
            0.45,
            format!("{:.2}", self.settings.phone_conf_min),
            Message::SetPhoneConfMin,
        );

        let face_card = slider_card(
            pick(
                "Umbral YuNet para detectar cara",
                "YuNet threshold for face detection",
            )
            .to_string(),
            pick(
                "Si la cámara te marca ausente cuando estás presente, baja este valor. Si te detecta cuando ya te fuiste, súbelo.",
                "If the camera marks you absent while you're there, lower this. If it detects you after you left, raise it.",
            )
            .to_string(),
            self.settings.face_conf_min,
            0.40..=0.90,
            0.05,
            0.6,
            format!("{:.2}", self.settings.face_conf_min),
            Message::SetFaceConfMin,
        );

        let cooldown_card = {
            let value = self.settings.coach_negative_cooldown_mins as f32;
            slider_card(
                pick(
                    "Cooldown del coach IA tras 👎 (minutos)",
                    "AI coach cooldown after 👎 (minutes)",
                )
                .to_string(),
                pick(
                    "Tras un voto negativo, el coach usa el banco curado en vez del LLM durante este tiempo. 0 = desactivado.",
                    "After a negative vote, the coach uses the curated bank instead of the LLM for this many minutes. 0 = off.",
                )
                .to_string(),
                value,
                0.0..=240.0,
                15.0,
                60.0,
                format!("{} min", self.settings.coach_negative_cooldown_mins),
                |v| Message::SetCoachCooldownMins(v.round() as u32),
            )
        };

        // ---------- User exceptions plugin audit ----------
        let exceptions_card: Element<'_, Message> = {
            let exception_path = crate::infra::plugins::plugins_dir()
                .join("excepciones-usuario.toml");
            let body_str = std::fs::read_to_string(&exception_path).unwrap_or_default();
            let parsed: Option<crate::infra::plugins::PluginFile> = if body_str.is_empty() {
                None
            } else {
                toml::from_str(&body_str).ok()
            };
            let processes: Vec<String> = parsed
                .as_ref()
                .and_then(|p| {
                    p.classifier_rules
                        .as_ref()
                        .map(|r| r.focus.processes.clone())
                })
                .unwrap_or_default();
            let inner: Element<'_, Message> = if processes.is_empty() {
                text(pick(
                    "Aún no has marcado ninguna distracción como falso positivo. Cuando lo hagas, los procesos aparecerán aquí.",
                    "You haven't marked any distraction as false positive yet. When you do, processes will appear here.",
                ))
                .size(FONT_SMALL)
                .color(TEXT_MUTED)
                .into()
            } else {
                column(
                    processes
                        .iter()
                        .map(|p| {
                            let elem: Element<'_, Message> = text(format!("• {p}"))
                                .size(FONT_SMALL)
                                .color(TEXT_PRIMARY)
                                .into();
                            elem
                        })
                        .collect::<Vec<_>>(),
                )
                .spacing(2)
                .into()
            };
            settings_card_local(
                pick("Falsos positivos guardados", "Saved false positives"),
                column![
                    text(format!(
                        "{}: {}",
                        pick("Archivo", "File"),
                        exception_path.display()
                    ))
                    .size(FONT_TINY)
                    .color(TEXT_MUTED),
                    inner,
                ]
                .spacing(SPACE_XS as u16)
                .into(),
            )
        };

        // ---------- Calibración guiada banner ----------
        let guided_card: Element<'_, Message> = settings_card_local(
            pick("Calibración guiada", "Guided calibration"),
            column![
                text(pick(
                    "En vez de adivinar valores, deja que SolarFocus capture muestras de tu cámara y proponga umbrales basados en TUS condiciones reales (luz, distancia, fondo).",
                    "Instead of guessing values, let SolarFocus capture samples from your camera and propose thresholds based on YOUR real conditions (light, distance, background).",
                ))
                .size(FONT_SMALL)
                .color(TEXT_SECONDARY),
                text(pick(
                    "Tarda ~30 segundos en total. No entrena los modelos — calibra el punto de decisión usando tus datos.",
                    "Takes ~30 seconds total. Doesn't train the models — calibrates the decision point using your data.",
                ))
                .size(FONT_TINY)
                .color(TEXT_MUTED),
                iced::widget::row![
                    button(
                        text(pick("Iniciar wizard", "Start wizard").to_string())
                            .size(FONT_SMALL)
                            .color(BG),
                    )
                    .on_press(Message::StartCalibrationWizard)
                    .padding([8, 18])
                    .style(|_, _| iced::widget::button::Style {
                        background: Some(iced::Background::Color(ACCENT)),
                        text_color: BG,
                        border: iced::Border {
                            radius: 6.0.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    }),
                ],
            ]
            .spacing(SPACE_XS as u16)
            .into(),
        );

        // ---------- Final layout ----------
        let body = column![
            text(pick("Calibración", "Calibration"))
                .size(FONT_TITLE)
                .color(TEXT_PRIMARY),
            text(pick(
                "Afina cómo SolarFocus detecta distracciones, presencia y celular. Cada slider tiene un valor recomendado en verde; mueve los demás solo si tu workflow lo requiere.",
                "Tune how SolarFocus detects distractions, presence and phone. Each slider has a recommended green value; move the others only if your workflow needs it.",
            ))
            .size(FONT_SMALL)
            .color(TEXT_SECONDARY),
            guided_card,
            test_card,
            confidence_card,
            samples_card,
            absent_card,
            phone_card,
            face_card,
            cooldown_card,
            exceptions_card,
        ]
        .spacing(SPACE_MD as u16)
        .max_width(720);

        container(body).width(Length::Fill).into()
    }

    /// v1.13.0 — guided calibration wizard panel. Replaces the slider
    /// grid while `self.calibration_wizard` is `Some`.
    fn view_calibration_wizard(&self) -> Element<'_, Message> {
        use crate::app::state::CalibrationStage;
        let lang = self.settings.language;
        let pick = |es: &'static str, en: &'static str| -> &'static str {
            if lang == Language::Es { es } else { en }
        };
        let wiz = self.calibration_wizard.as_ref().expect("wizard active");
        let total_stages: u8 = 5; // welcome→summary minus summary itself for the progress dots
        let stage_idx: u8 = match wiz.stage {
            CalibrationStage::Welcome => 1,
            CalibrationStage::FaceWith => 2,
            CalibrationStage::FaceWithout => 3,
            CalibrationStage::PhoneWith => 4,
            CalibrationStage::PhoneWithout => 5,
            CalibrationStage::Summary => 5,
        };

        let title = if wiz.stage_warning.is_some() {
            pick("⚠ Problema detectado", "⚠ Problem detected")
        } else {
            match wiz.stage {
                CalibrationStage::Welcome => pick("Bienvenido al wizard", "Welcome to the wizard"),
                CalibrationStage::FaceWith => pick("Paso 1 · Cara presente", "Step 1 · Face present"),
                CalibrationStage::FaceWithout => pick("Paso 2 · Cara ausente", "Step 2 · Face absent"),
                CalibrationStage::PhoneWith => pick("Paso 3 · Celular en cámara", "Step 3 · Phone in frame"),
                CalibrationStage::PhoneWithout => pick("Paso 4 · Sin celular", "Step 4 · No phone"),
                CalibrationStage::Summary => pick("Resumen", "Summary"),
            }
        };

        let instruction: String = if let Some(ref msg) = wiz.stage_warning {
            msg.clone()
        } else { match wiz.stage {
            CalibrationStage::Welcome => pick(
                "Vamos a capturar 4 batches de 10 frames (uno por situación) para calibrar los umbrales de cara y celular usando tus datos reales. Tarda ~30 s.",
                "We'll capture 4 batches of 10 frames (one per situation) to calibrate the face + phone thresholds using your real data. Takes ~30 s.",
            ).to_string(),
            CalibrationStage::FaceWith => pick(
                "Mira fijamente a la cámara durante los próximos 3 segundos. Pulsa Capturar cuando estés listo.",
                "Look directly at the camera for the next 3 seconds. Press Capture when ready.",
            ).to_string(),
            CalibrationStage::FaceWithout => pick(
                "Aparta la mirada o cubre la cámara durante 3 segundos. Pulsa Capturar.",
                "Look away or cover the camera for 3 seconds. Press Capture.",
            ).to_string(),
            CalibrationStage::PhoneWith => pick(
                "Levanta tu celular frente a la cámara durante 3 segundos. Pulsa Capturar.",
                "Hold your phone in front of the camera for 3 seconds. Press Capture.",
            ).to_string(),
            CalibrationStage::PhoneWithout => pick(
                "Quita el celular del campo de visión. Pulsa Capturar.",
                "Move the phone out of frame. Press Capture.",
            ).to_string(),
            CalibrationStage::Summary => {
                // v1.13.0 — render quality + error rate + overlap +
                // actionable recommendation per detector.
                let line = |label: &str,
                            current: f32,
                            sug: Option<f32>,
                            quality: &Option<(String, u32, bool, f32, f32)>|
                 -> String {
                    match (sug, quality) {
                        (Some(v), Some((q, err, overlap, m_with, m_without))) => {
                            let q_es = match q.as_str() {
                                "strong" => pick("✓ excelente", "✓ excellent"),
                                "marginal" => pick("⚠ marginal", "⚠ marginal"),
                                _ => pick("⚠", "⚠"),
                            };
                            let overlap_note = if *overlap {
                                pick(
                                    " · solapamiento detectado",
                                    " · overlap detected",
                                )
                            } else {
                                ""
                            };
                            format!(
                                "{}: {} → {:.2} {} · error esperado ~{}% (con={:.2} · sin={:.2}){}",
                                label,
                                format!("{:.2}", current),
                                v,
                                q_es,
                                err,
                                m_with,
                                m_without,
                                overlap_note,
                            )
                        }
                        (None, Some((q, err, _overlap, m_with, m_without))) => {
                            let reason = match q.as_str() {
                                "unusable" => pick(
                                    "no separable (tu cámara/ángulo no permite distinguir presente vs ausente con este modelo)",
                                    "not separable (your camera/angle can't distinguish present vs absent with this model)",
                                ),
                                "insufficient" => pick(
                                    "datos insuficientes",
                                    "insufficient data",
                                ),
                                _ => pick("separación insuficiente", "insufficient separation"),
                            };
                            format!(
                                "{}: {} · error esperado ~{}% (con={:.2} · sin={:.2})",
                                label, reason, err, m_with, m_without,
                            )
                        }
                        (_, None) => format!(
                            "{}: {}",
                            label,
                            pick("sin datos", "no data"),
                        ),
                    }
                };
                let face = line(
                    pick("YuNet (cara)", "YuNet (face)"),
                    self.settings.face_conf_min,
                    wiz.suggested_face,
                    &wiz.face_quality,
                );
                let phone = line(
                    pick("YOLO (celular)", "YOLO (phone)"),
                    self.settings.phone_conf_min,
                    wiz.suggested_phone,
                    &wiz.phone_quality,
                );

                // Actionable recommendation block.
                let unusable_face = wiz
                    .face_quality
                    .as_ref()
                    .map(|(q, ..)| q == "unusable")
                    .unwrap_or(false);
                let unusable_phone = wiz
                    .phone_quality
                    .as_ref()
                    .map(|(q, ..)| q == "unusable")
                    .unwrap_or(false);
                let recommendation = if unusable_face && unusable_phone {
                    pick(
                        "\n\nRecomendación: tu setup no es separable por estos modelos. \
                         (1) Mueve la cámara a un ángulo más frontal · \
                         (2) Mejora la iluminación · \
                         (3) Re-intenta · \
                         (4) Si nada funciona, desactiva Detección de presencia en Setup → IA.",
                        "\n\nRecommendation: your setup isn't separable by these models. \
                         (1) Move the camera to a more frontal angle · \
                         (2) Improve lighting · \
                         (3) Retry · \
                         (4) If nothing works, disable Presence detection in Setup → AI.",
                    )
                    .to_string()
                } else if unusable_face || unusable_phone {
                    pick(
                        "\n\nRecomendación: el detector marcado como 'no separable' falló para tu setup. Considera reposicionar la cámara o desactivar ese detector.",
                        "\n\nRecommendation: the detector marked 'not separable' failed for your setup. Reposition the camera or disable that detector.",
                    )
                    .to_string()
                } else {
                    String::new()
                };

                format!("{}\n\n{}{}", face, phone, recommendation)
            }
        }};

        // v1.13.0 — when a per-stage warning is set, the primary button
        // becomes "Reintentar este paso" (re-runs CalibrationCapture
        // for the same stage) and a secondary "Continuar de todos modos"
        // appears next to it.
        let warning_active = wiz.stage_warning.is_some();
        let primary_btn_label = if warning_active {
            pick("Reintentar este paso", "Retry this step")
        } else {
            match wiz.stage {
                CalibrationStage::Welcome => pick("Empezar", "Begin"),
                CalibrationStage::Summary => pick("Aplicar", "Apply"),
                _ if wiz.capturing => pick("Capturando…", "Capturing…"),
                _ => pick("Capturar", "Capture"),
            }
        };
        let primary_msg: Option<Message> = if warning_active {
            Some(Message::CalibrationCapture)
        } else {
            match wiz.stage {
                CalibrationStage::Welcome => Some(Message::CalibrationCapture),
                CalibrationStage::Summary => Some(Message::CalibrationApply),
                _ if wiz.capturing => None,
                _ => Some(Message::CalibrationCapture),
            }
        };

        let primary_btn: Element<'_, Message> = {
            let label = text(primary_btn_label.to_string()).size(FONT_BODY).color(BG);
            let mut b = button(label).padding([10, 24]).style(|_, _| {
                iced::widget::button::Style {
                    background: Some(iced::Background::Color(ACCENT)),
                    text_color: BG,
                    border: iced::Border {
                        radius: 8.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }
            });
            if let Some(m) = primary_msg {
                b = b.on_press(m);
            }
            b.into()
        };

        let cancel_btn: Element<'_, Message> = button(
            text(pick("Cancelar", "Cancel").to_string())
                .size(FONT_SMALL)
                .color(TEXT_SECONDARY),
        )
        .on_press(Message::CalibrationWizardCancel)
        .padding([8, 18])
        .style(|_, _| iced::widget::button::Style {
            background: Some(iced::Background::Color(SURFACE_RAISED)),
            text_color: TEXT_PRIMARY,
            border: iced::Border {
                radius: 6.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into();

        // v1.13.0 — "Continuar de todos modos" only when there's an active
        // stage warning. Lets the user push through despite warning.
        let continue_anyway_btn: Element<'_, Message> = if warning_active {
            button(
                text(pick("Continuar de todos modos", "Continue anyway").to_string())
                    .size(FONT_SMALL)
                    .color(TEXT_PRIMARY),
            )
            .on_press(Message::CalibrationContinueAnyway)
            .padding([8, 18])
            .style(|_, _| iced::widget::button::Style {
                background: Some(iced::Background::Color(SURFACE_RAISED)),
                text_color: TEXT_PRIMARY,
                border: iced::Border {
                    radius: 6.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .into()
        } else {
            iced::widget::Space::with_width(0.0).into()
        };

        // v1.13.1 — selective retry on Summary: separate buttons for
        // face vs phone, so a user with one good detector + one bad
        // doesn't waste capture cycles re-doing the working one.
        let retry_btn: Element<'_, Message> = if matches!(wiz.stage, CalibrationStage::Summary) {
            let mk_btn = |label: String, msg: Message| -> Element<'_, Message> {
                button(text(label).size(FONT_SMALL).color(TEXT_PRIMARY))
                    .on_press(msg)
                    .padding([8, 14])
                    .style(|_, _| iced::widget::button::Style {
                        background: Some(iced::Background::Color(SURFACE_RAISED)),
                        text_color: TEXT_PRIMARY,
                        border: iced::Border {
                            radius: 6.0.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    })
                    .into()
            };
            iced::widget::row![
                mk_btn(
                    pick("Reintentar cara", "Retry face").to_string(),
                    Message::CalibrationRetryFace
                ),
                iced::widget::Space::with_width(SPACE_XS as f32),
                mk_btn(
                    pick("Reintentar celular", "Retry phone").to_string(),
                    Message::CalibrationRetryPhone
                ),
            ]
            .into()
        } else {
            iced::widget::Space::with_width(0.0).into()
        };

        // Inline mean for current stage's last batch (if any).
        let last_batch_summary: String = match wiz.stage {
            CalibrationStage::FaceWith => summarize(&wiz.face_with),
            CalibrationStage::FaceWithout => summarize(&wiz.face_without),
            CalibrationStage::PhoneWith => summarize(&wiz.phone_with),
            CalibrationStage::PhoneWithout => summarize(&wiz.phone_without),
            CalibrationStage::Summary => format!(
                "{}: {} · {}: {}",
                pick("cara con", "face with"),
                summarize(&wiz.face_with),
                pick("cara sin", "face without"),
                summarize(&wiz.face_without),
            ),
            _ => String::new(),
        };

        let body = column![
            text(pick("Calibración guiada", "Guided calibration"))
                .size(FONT_TITLE)
                .color(TEXT_PRIMARY),
            text(format!(
                "{} {} / {}",
                pick("Paso", "Step"),
                stage_idx,
                total_stages
            ))
            .size(FONT_SMALL)
            .color(TEXT_MUTED),
            text(title.to_string()).size(FONT_LEAD).color(TEXT_PRIMARY),
            text(instruction).size(FONT_BODY).color(TEXT_SECONDARY),
            iced::widget::Space::with_height(SPACE_SM as f32),
            iced::widget::row![
                primary_btn,
                iced::widget::Space::with_width(SPACE_SM as f32),
                continue_anyway_btn,
                iced::widget::Space::with_width(SPACE_SM as f32),
                retry_btn,
                iced::widget::Space::with_width(SPACE_SM as f32),
                cancel_btn,
            ]
            .align_y(iced::alignment::Vertical::Center),
            iced::widget::Space::with_height(SPACE_XS as f32),
            text(last_batch_summary).size(FONT_TINY).color(TEXT_MUTED),
        ]
        .spacing(SPACE_SM as u16)
        .max_width(720);

        container(body).width(Length::Fill).into()
    }
}

fn summarize(scores: &[f32]) -> String {
    if scores.is_empty() {
        return String::new();
    }
    let m: f32 = scores.iter().sum::<f32>() / scores.len() as f32;
    let mn: f32 = scores.iter().copied().fold(f32::INFINITY, f32::min);
    let mx: f32 = scores.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    format!("n={} · mean={:.2} · min={:.2} · max={:.2}", scores.len(), m, mn, mx)
}
