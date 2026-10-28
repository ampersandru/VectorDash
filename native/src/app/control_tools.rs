use super::*;
fn decision_combo(ui: &mut egui::Ui, id: &str, mode: &mut crate::decision::Mode) -> bool {
    let before = *mode;
    egui::ComboBox::from_id_salt(id).width(65.0).selected_text(RichText::new(mode.label()).size(10.0)).show_ui(ui,|ui| {
        for value in [crate::decision::Mode::Off,crate::decision::Mode::Suggest,crate::decision::Mode::Execute] {ui.selectable_value(mode,value,t(value.label()));}
    }).response.on_hover_text(t("Local decision model: Off uses the main AI; Suggest proposes targets; Execute runs verified local steps."));
    before != *mode
}

impl App {
    pub(super) fn control_settings(&mut self, ui: &mut egui::Ui) {
        ui.group(|ui| {
            ui.set_min_width(ui.available_width());
            self.settings_anchor(ui,"Computer and browser use — BETA");
            ui.label(RichText::new(t("Computer and browser use — BETA")).strong());
            ui.checkbox(
                &mut self.s.computer_use_enabled,
                t("Enable computer control in AI chat"),
            );
            ui.checkbox(
                &mut self.s.browser_use_enabled,
                t("Enable browser control in AI chat"),
            );
            ui.separator();
            self.settings_anchor(ui,"Decision model — BETA");
            let decision_header=ui.label(RichText::new(t("Decision model — BETA")).strong());
            if self.smoke.is_some() && std::env::args().any(|a|a=="--smoke-decision-section") {ui.scroll_to_rect(decision_header.rect,Some(egui::Align::TOP));}
            ui.weak(t("Qev chooses observed controls. Your chat model still plans tasks and writes text. Permissions apply to every action."));
            if ui.checkbox(&mut self.s.decision_enabled,t("Enable decision model")).changed() && !self.s.decision_enabled { crate::decision::unload(); }
            ui.add_enabled_ui(self.s.decision_enabled, |ui| {
            let mut provider=if crate::decision::remote(&self.s){"Remote".to_owned()}else{"Local".to_owned()};
            let mut provider_chosen=false;
            egui::ComboBox::from_id_salt("decision-provider").selected_text(t(&provider)).show_ui(ui,|ui|{provider_chosen|=ui.selectable_value(&mut provider,"Local".into(),t("Local")).clicked();provider_chosen|=ui.selectable_value(&mut provider,"Remote".into(),t("Remote")).clicked();});
            if provider_chosen && self.s.decision_provider != provider {self.s.decision_provider=provider;crate::decision::unload();}
            if self.s.decision_enabled {
            ui.horizontal(|ui| {ui.label(t("Computer default"));decision_combo(ui,"computer-decision-default",&mut self.s.computer_decision_mode);ui.label(t("Browser default"));decision_combo(ui,"browser-decision-default",&mut self.s.browser_decision_mode);});
            }
            if !crate::decision::remote(&self.s) {
            ui.label(t("Qev GGUF model"));
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.s.decision_model).desired_width((ui.available_width()-90.0).max(100.0)));
                if ui.button(t("Browse…")).clicked() {if let Some(path)=rfd::FileDialog::new().add_filter("GGUF",&["gguf"]).pick_file(){self.s.decision_model=path.to_string_lossy().into_owned();}}
            });
            if ui.add_enabled(!self.setup_busy,egui::Button::new(t("Download Qev Q8_0 (about 812 MB)"))).clicked() {if self.save(){self.setup_busy=true;self.setup.install_decision(self.s.clone(),false);}}
            }
            ui.checkbox(&mut self.s.decision_vision,t("Enable vision for the decision model"));
            ui.weak(t("Vision sends a small screenshot alongside observed controls. Remote vision is checked before real screenshots are sent; the chat model's vision setting is separate."));
            if !crate::decision::remote(&self.s) {
            if ui.add_enabled(!self.setup_busy,egui::Button::new(t("Download Qev vision + projector (about 928 MB)"))).clicked(){if self.save(){self.setup_busy=true;self.setup.install_decision(self.s.clone(),true);}}
            if self.s.decision_vision {
                for (label,path) in [("Vision GGUF",&mut self.s.decision_vision_model),("Vision projector (mmproj)",&mut self.s.decision_mmproj)] {
                    ui.label(t(label));ui.horizontal(|ui|{ui.add(egui::TextEdit::singleline(path).desired_width((ui.available_width()-90.0).max(100.0)));if ui.button(t("Browse…")).clicked(){if let Some(file)=rfd::FileDialog::new().add_filter("GGUF",&["gguf"]).pick_file(){*path=file.to_string_lossy().into_owned();}}});
                }
            }
            ui.weak(t("Reuses the configured llama-server executable and GPU, in a separate on-demand process. EmbeddingGemma stays unchanged."));
            }
            if !crate::decision::remote(&self.s) {
            ui.collapsing(t("Decision runtime and server"),|ui| {
                ui.label(t("Runtime executable (empty = reuse Hardware runtime)"));ui.text_edit_singleline(&mut self.s.decision_runtime);
                if ui.button(t("Choose runtime…")).clicked(){if let Some(path)=rfd::FileDialog::new().add_filter("Executable",&["exe"]).pick_file(){self.s.decision_runtime=path.to_string_lossy().into_owned();}}
                });
                } else {
                ui.label(t("Decision server URL"));ui.text_edit_singleline(&mut self.s.decision_url);
                ui.label(t("Decision API key (optional)"));ui.add(egui::TextEdit::singleline(&mut self.s.decision_key).password(true));
                ui.label(t("Remote decision model ID (optional)"));ui.text_edit_singleline(&mut self.s.decision_remote_model);
                ui.weak(t("A remote decision server receives the compact window/page state after tool approval."));
                }
            ui.horizontal(|ui| {ui.label(t("Minimum choice probability"));ui.add(egui::DragValue::new(&mut self.s.decision_confidence).range(0.5..=0.999).speed(0.01));});
            ui.weak(t("Uncertain choices return to the main AI. High probability is not proof that an action succeeded."));
            ui.horizontal(|ui| {
                if ui.add_enabled(!self.setup_busy&&!self.ai_busy,egui::Button::new(t("Test decision model and vision"))).clicked(){if self.save(){self.setup_busy=true;self.setup.cancel.store(false,Ordering::Relaxed);let settings=self.s.clone();let stop=self.setup.cancel.clone();let tx=self.engine.tx.clone();std::thread::spawn(move||{let result=crate::decision::test(&settings,&stop).map_err(|e|e.to_string());let _=tx.send(Event::DecisionReady(result));});}}

            });
            ui.weak(t("Local Qev and its projector load when an AI request is submitted with Suggest or Execute selected, and unload after the full response, cancellation, error, or VectorDash exit. Remote model lifetime is managed by its server."));
            });
            ui.separator();
            ui.weak(t(
                "Requires a tool-capable AI model. Screenshots require vision; use 8192+ tokens.",
            ));
            ui.weak(t(
                "Window/page content is sent to that server only after approval.",
            ));
            ui.weak(t(
                "Actions can change files, submit forms and move your cursor.",
            ));
            ui.horizontal(|ui| {
                ui.label(t("Maximum actions per response"));
                ui.add(egui::DragValue::new(&mut self.s.control_max_steps).range(1..=128));
            });
            ui.add_enabled(!self.ai_busy, egui::Checkbox::new(&mut self.s.control_debug,t("Save computer/browser debug transcript")));
            ui.weak(t("Off by default. Saves your prompts, AI responses, actual tool steps and Qev diagnostics to one local file per conversation. May contain private screen text and file paths. Image payloads and API settings are excluded. Nothing is uploaded automatically."));
            self.control_debug_ui(ui);
            ui.checkbox(
                &mut self.s.control_browser_visible,
                t("Show browser window"),
            );
            ui.label(t("Browser executable (empty = detect Edge or Chrome)"));
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.s.control_browser_path)
                        .desired_width((ui.available_width() - 100.0).max(180.0)),
                );
                if ui.button(t("Browse…")).clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Browser", &["exe"])
                        .pick_file()
                    {
                        self.s.control_browser_path = path.to_string_lossy().into_owned();
                    }
                }
            });
            if let Some(path) = crate::computer::browser_path(&self.s.control_browser_path) {
                ui.weak(path.display().to_string());
            } else {
                ui.weak(t("No supported browser found."));
            }
            if ui.button(t("Download Microsoft Edge (optional)")).clicked() {
                let _ = crate::platform::open("https://www.microsoft.com/edge/download");
            }
            ui.weak(t(
                "No extra runtime. Uses an installed browser with a temporary profile.",
            ));
            ui.weak(t(
                "New Conversation clears permissions and closes the controlled browser.",
            ));
            ui.label(tr(&format!(
                "{}: {}",
                t("Emergency stop shortcut"),
                self.s.binding("stop_ai_tools")
            )));
            if ui.button(t("Stop tools and revoke permissions")).clicked() {
                self.stop_ai();
                self.control_session = Arc::new(crate::computer::Session::default());
            }
        });
    }
    pub(super) fn control_toggles(&mut self, ui: &mut egui::Ui) {
        let mut stop = false;
        let mut mode_changed = false;
        if self.saved_settings.browser_use_enabled {
            let response = ui.add(egui::Checkbox::new(
                &mut self.ai_browser_enabled,
                RichText::new(t("Browser")).size(11.0),
            ));
            if response.changed() && !self.ai_browser_enabled {
                stop = true;
            }
            response.on_hover_text(t("Allow control of a separate browser profile."));
            if self.ai_browser_enabled && self.saved_settings.decision_enabled {
                ui.add_enabled_ui(!self.ai_busy, |ui| {
                    mode_changed |= decision_combo(
                        ui,
                        "browser-decision-chat",
                        &mut self.s.browser_decision_mode,
                    );
                });
            }
        }
        if self.saved_settings.computer_use_enabled {
            let response = ui.add(egui::Checkbox::new(
                &mut self.ai_computer_enabled,
                RichText::new(t("Computer")).size(11.0),
            ));
            if response.changed() && !self.ai_computer_enabled {
                stop = true;
            }
            response.on_hover_text(t("Allow control of open Windows applications."));
            if self.ai_computer_enabled && self.saved_settings.decision_enabled {
                ui.add_enabled_ui(!self.ai_busy, |ui| {
                    mode_changed |= decision_combo(
                        ui,
                        "computer-decision-chat",
                        &mut self.s.computer_decision_mode,
                    );
                });
            }
        }
        if mode_changed {
            self.save();
        }
        if stop {
            self.stop_ai();
            self.control_session = Arc::new(crate::computer::Session::default());
        }
    }
    pub(super) fn control_smoke_tick(&mut self, ctx: &egui::Context) {
        if self.smoke.is_none() {
            return;
        }
        let args = std::env::args().collect::<Vec<_>>();
        if let Some(tool) = args
            .iter()
            .position(|a| a == "--smoke-control-tools")
            .and_then(|i| args.get(i + 1))
        {
            let key = egui::Id::new("qa-control-toggles");
            if !ctx.data(|d| d.get_temp::<bool>(key).unwrap_or(false)) {
                ctx.data_mut(|d| d.insert_temp(key, true));
                self.ai_computer_enabled = self.saved_settings.computer_use_enabled
                    && (tool == "computer" || tool == "both");
                self.ai_browser_enabled = self.saved_settings.browser_use_enabled
                    && (tool == "browser" || tool == "both");
            }
        }
        if args.iter().any(|a| a == "--smoke-control-run")
            && self.startup.elapsed() > Duration::from_secs(1)
        {
            let key = egui::Id::new("qa-control-run");
            if !ctx.data(|d| d.get_temp::<bool>(key).unwrap_or(false)) {
                ctx.data_mut(|d| d.insert_temp(key, true));
                self.ask_ai();
            }
        }
        if args.iter().any(|a| a == "--smoke-control-new-chat")
            && self.startup.elapsed() > Duration::from_secs(3)
        {
            let key = egui::Id::new("qa-control-new-chat");
            if !ctx.data(|d| d.get_temp::<bool>(key).unwrap_or(false)) {
                ctx.data_mut(|d| d.insert_temp(key, true));
                self.new_conversation();
            }
        }
        if let Some(question) = args
            .iter()
            .position(|a| a == "--smoke-control-followup")
            .and_then(|i| args.get(i + 1))
        {
            let key = egui::Id::new("qa-control-followup");
            if !self.ai_busy
                && self.ai_messages.iter().any(|m| m.role == "assistant")
                && !ctx.data(|d| d.get_temp::<bool>(key).unwrap_or(false))
            {
                ctx.data_mut(|d| d.insert_temp(key, true));
                self.query = question.clone();
                self.ask_ai();
            }
        }
    }
    pub(super) fn control_permission_ui(&mut self, ctx: &egui::Context) {
        let Some(prompt) = self.control_session.prompt() else {
            return;
        };
        if !self.ai_busy {
            return;
        }
        let tool = if prompt.tool == "computer_use" {
            "Computer control"
        } else {
            "Browser control"
        };
        let mut decision = None;
        let response =
            egui::Modal::new(egui::Id::new(("control-permission", prompt.id))).show(ctx, |ui| {
                ui.set_width((ctx.content_rect().width() - 80.0).clamp(260.0, 650.0));
                self.settings_anchor(ui, "Allow this AI action?");
                ui.heading(t("Allow this AI action?"));
                ui.label(tr(&format!("{} · {}", t(tool), self.ai_session_model)));
                if prompt.arguments["decision_vision"] == true {
                    ui.weak(t("Decision-model screenshot"));
                    if !crate::decision::remote(&self.saved_settings) {
                        ui.weak(t("Processed by local Qev"));
                    } else {
                        ui.weak(&self.saved_settings.decision_url);
                    }
                } else {
                    ui.weak(&self.saved_settings.llm_url);
                }
                ui.label(tr(&prompt.summary));
                egui::ScrollArea::vertical()
                    .max_height((ctx.content_rect().height() - 210.0).clamp(80.0, 260.0))
                    .show(ui, |ui| {
                        if prompt.arguments.get("password").is_some() {
                            ui.label(t("Password input (hidden)"));
                        }
                        for (key, label) in [
                            ("url", "Website"),
                            ("text", "Text to enter"),
                            ("keys", "Keys"),
                            ("delta", "Scroll amount"),
                        ] {
                            if let Some(value) = prompt.arguments.get(key) {
                                ui.label(
                                    RichText::new(t(
                                        if key == "text"
                                            && prompt.arguments["action"] == "applications"
                                        {
                                            "Search"
                                        } else {
                                            label
                                        },
                                    ))
                                    .strong(),
                                );
                                ui.add(
                                    egui::Label::new(
                                        value
                                            .as_str()
                                            .map(str::to_owned)
                                            .unwrap_or_else(|| value.to_string()),
                                    )
                                    .wrap(),
                                );
                            }
                        }
                        if let (Some(x), Some(y)) =
                            (prompt.arguments.get("x"), prompt.arguments.get("y"))
                        {
                            ui.label(tr(&format!("{}: {x}, {y}", t("Position"))));
                        }
                    });
                ui.weak(t(
                    "Permission covers this tool only, until you start a new chat.",
                ));
                ui.horizontal_wrapped(|ui| {
                    if ui.button(t("Allow once")).clicked() {
                        decision = Some(crate::computer::Decision::Once);
                    }
                    if ui.button(t("Allow all this session")).clicked() {
                        decision = Some(crate::computer::Decision::Session);
                    }
                    if ui.button(t("Don't allow")).clicked() {
                        decision = Some(crate::computer::Decision::Deny);
                    }
                });
            });
        if response.should_close() {
            decision = Some(crate::computer::Decision::Deny);
        }
        if self.smoke.is_some() {
            let args = std::env::args().collect::<Vec<_>>();
            if let Some(value) = args
                .iter()
                .position(|a| a == "--smoke-control-approval")
                .and_then(|i| args.get(i + 1))
            {
                decision = Some(match value.as_str() {
                    "once" => crate::computer::Decision::Once,
                    "session" => crate::computer::Decision::Session,
                    _ => crate::computer::Decision::Deny,
                });
            }
        }
        if let Some(decision) = decision {
            let _ = prompt.reply.try_send(decision);
            ctx.request_repaint();
        }
    }
}
