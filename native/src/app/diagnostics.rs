use super::*;
impl App {
    pub(super) fn control_debug_ui(&self, ui: &mut egui::Ui) {
        if let Some(log) = &self.control_debug_log {
            ui.horizontal_wrapped(|ui| {
                ui.label(t("Control debug file"));
                if ui.button(t("Export debug transcript...")).clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Debug transcript", &["jsonl"])
                        .set_file_name("VectorDash-control-debug.jsonl")
                        .save_file()
                    {
                        if let Err(e) = log.export(&path) {
                            let _ = self.engine.tx.send(Event::Log(
                                "Warning".into(),
                                format!("Debug export failed: {e}"),
                            ));
                        }
                    }
                }
                if ui.button(t("Open debug folder")).clicked() {
                    if let Some(parent) = log.path.parent() {
                        let _ = crate::platform::open(&parent.to_string_lossy());
                    }
                }
            });
            ui.weak(log.path.display().to_string());
            if let Some(error) = log.error() {
                ui.colored_label(
                    egui::Color32::YELLOW,
                    format!("Debug recording error: {error}"),
                );
            }
        }
    }
    pub(super) fn ai_diagnostics_ui(&self, ui: &mut egui::Ui) {
        self.control_debug_ui(ui);
        if self.ai_diagnostics.is_empty() {
            return;
        }
        egui::CollapsingHeader::new(t("AI step diagnostics"))
            .id_salt("ai-step-diagnostics")
            .default_open(self.smoke.is_some() && std::env::args().any(|v| v == "--smoke-diagnostics-open"))
            .show(ui, |ui| {
                if ui.button(t("Copy diagnostics")).clicked() {
                    if let Ok(json) = serde_json::to_string_pretty(&serde_json::json!({"job":self.ai_diagnostics_job,"steps":self.ai_diagnostics})) {
                        ui.ctx().copy_text(json);
                    }
                }
                ui.weak(t(
                    "Missing server metrics are shown as —. Action wait includes approval time.",
                ));
                for row in &self.ai_diagnostics {
                    ui.separator();
                    let count = |value: Option<u64>| {
                        value.map(|v| v.to_string()).unwrap_or_else(|| "—".into())
                    };
                    let seconds = |value: Option<f64>| {
                        value
                            .map(|v| format!("{:.2} s", v / 1000.0))
                            .unwrap_or_else(|| "—".into())
                    };
                    ui.horizontal_wrapped(|ui| {
                        ui.strong(format!("{} {}", t("Step"), row.step));
                        ui.label(format!(
                            "{}: {}",
                            t("Input tokens"),
                            count(row.input_tokens)
                        ));
                        let cache = row
                            .cache_percent()
                            .map(|v| format!(" ({v:.1}%)"))
                            .unwrap_or_default();
                        ui.label(format!(
                            "{}: {}{}",
                            t("Cached tokens"),
                            count(row.cached_tokens),
                            cache
                        ));
                        ui.label(format!(
                            "{}: {}",
                            t("Output tokens"),
                            count(row.output_tokens)
                        ));
                    });
                    ui.horizontal_wrapped(|ui| {
                        ui.label(format!(
                            "{}: {}",
                            t("First token"),
                            seconds(row.first_token_ms)
                        ));
                        ui.label(format!("{}: {}", t("Model time"), seconds(row.model_ms)));
                        ui.label(format!(
                            "{}: {}",
                            t("Action wait"),
                            seconds(row.action_wait_ms)
                        ));
                    });
                    if row.earlier_context_changed {
                        ui.weak(t("Earlier context changed"));
                    }
                }
            });
    }
}
