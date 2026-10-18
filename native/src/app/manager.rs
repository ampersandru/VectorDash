use super::*;
impl App {
    pub(super) fn shortcut_settings(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        ui.heading(t("Keyboard shortcuts"));
        ui.weak(t("Click a binding, then press your shortcut. Windows key combinations are supported. Escape cancels."));
        egui::Grid::new("all-shortcuts")
            .num_columns(4)
            .spacing([14.0, 8.0])
            .show(ui, |ui| {
                for (id, label, global, _) in crate::shortcuts::ACTIONS {
                    ui.label(t(label));
                    ui.weak(t(if *global {
                        "Global"
                    } else {
                        "In search window"
                    }));
                    let text = if self.shortcut_capture.as_deref() == Some(id) {
                        t("Press shortcut…").to_owned()
                    } else {
                        let v = self.s.binding(id);
                        if v.is_empty() {
                            t("Not assigned").into()
                        } else {
                            v.into()
                        }
                    };
                    if ui.button(text).clicked() {
                        self.shortcut_capture = Some((*id).into());
                    }
                    if ui.small_button(t("Clear")).clicked() {
                        self.s.set_binding(id, String::new());
                    }
                    ui.end_row();
                }
            });
        ui.label(t("Voice shortcut popup defaults"));
        combo(
            ui,
            "voice-popup-tool",
            &mut self.s.voice_popup_tool,
            &["Search", "Clipboard", "AI", "Web"],
        );
        ui.weak(t("Applies when opening a closed search window. An open window keeps its current page. Disabled AI/Clipboard/Web falls back to Search."));
        ui.add_enabled(
            self.s.tts_enabled,
            egui::Checkbox::new(
                &mut self.s.voice_read_aloud,
                t("Read answers aloud automatically"),
            ),
        )
        .on_hover_text(t("Enable text to speech in Models and AI first"));
        if self.s.voice_popup_tool == "AI" {
            combo(
                ui,
                "voice-response",
                &mut self.s.voice_response,
                &["Brief", "Medium", "Full"],
            );
        }
        ui.weak(t(
            "Tab cycles enabled search modes. Emoji stays available by its button.",
        ));

        if ui.button(t("Reset shortcuts to defaults")).clicked() {
            self.s.reset_bindings();
            self.shortcut_capture = None;
        }
        ui.weak(t("Override reassigns a VectorDash action or intercepts the selected Windows shortcut while VectorDash is running. Some protected system shortcuts cannot be overridden."));
        if self.smoke.is_some() && std::env::args().any(|a| a == "--smoke-shortcuts-bottom") {
            ui.scroll_to_cursor(Some(egui::Align::Max));
        }
    }
    pub(super) fn capture_binding(&mut self, value: String) {
        let Some(action) = self.shortcut_capture.take() else {
            return;
        };
        if let Some(old) = self.s.binding_conflict(&action, &value) {
            self.shortcut_conflict = Some((action, value, old.into()));
        } else if crate::shortcuts::ACTIONS
            .iter()
            .any(|a| a.0 == action && a.2)
            && self.saved_settings.binding(&action) != value
            && !crate::platform::shortcut_available(&value)
        {
            self.shortcut_conflict = Some((action, value, "Windows / another application".into()));
        } else {
            self.s
                .set_binding(&action, crate::shortcuts::canonical(&value));
        }
    }
    pub(super) fn manager_shortcuts(&mut self, ctx: &egui::Context) {
        if self.smoke.is_some() {
            let args = std::env::args().collect::<Vec<_>>();
            if args.iter().any(|a| a == "--smoke-move-monitors")
                && self.startup.elapsed() > Duration::from_millis(800)
            {
                let key = egui::Id::new("qa-monitor-moves");
                let mut moves = ctx.data(|d| {
                    d.get_temp::<Vec<([i32; 2], [i32; 2])>>(key)
                        .unwrap_or_default()
                });
                let pending = ctx.data(|d| d.get_temp::<([i32; 2], Instant)>(key.with("pending")));
                if let Some((point, at)) = pending {
                    if at.elapsed() > Duration::from_millis(200) {
                        moves.push((
                            point,
                            crate::platform::main_window_position().unwrap_or([0, 0]),
                        ));
                        ctx.data_mut(|d| {
                            d.insert_temp(key, moves.clone());
                            d.remove::<([i32; 2], Instant)>(key.with("pending"));
                        });
                    }
                } else if let Ok(monitors) = xcap::Monitor::all() {
                    if let Some(m) = monitors.get(moves.len()) {
                        if let (Ok(x), Ok(y)) = (m.x(), m.y()) {
                            let point = [x + 80, y + 80];
                            crate::platform::position_main_window(point);
                            ctx.data_mut(|d| {
                                d.insert_temp(key.with("pending"), (point, Instant::now()))
                            });
                        }
                    }
                }
                ctx.request_repaint_after(Duration::from_millis(100));
            }
            if args.iter().any(|a| a == "--smoke-ai-image-turns")
                && !ctx.data(|d| {
                    d.get_temp::<bool>(egui::Id::new("qa-image-turns"))
                        .unwrap_or(false)
                })
            {
                ctx.data_mut(|d| d.insert_temp(egui::Id::new("qa-image-turns"), true));
                if let Some(path) = args
                    .iter()
                    .position(|a| a == "--smoke-capture-fixture")
                    .and_then(|i| args.get(i + 1))
                {
                    if let Ok(bytes) = std::fs::read(path) {
                        let first = crate::capture::vision(&bytes, None).unwrap();
                        let mut pixels = crate::capture::rgba(&bytes).unwrap();
                        pixels.get_pixel_mut(0, 0).0[0] ^= 255;
                        let mut output = std::io::Cursor::new(Vec::new());
                        image::DynamicImage::ImageRgba8(pixels)
                            .write_to(&mut output, image::ImageFormat::Png)
                            .unwrap();
                        let second = crate::capture::vision(output.get_ref(), None).unwrap();
                        self.ai_messages = vec![
                            crate::ai::Message {
                                role: "user".into(),
                                content: "First attached image".into(),
                                images: vec![first],
                            },
                            crate::ai::Message {
                                role: "assistant".into(),
                                content: "First answer".into(),
                                images: vec![],
                            },
                            crate::ai::Message {
                                role: "user".into(),
                                content: "Second attached image".into(),
                                images: vec![second],
                            },
                        ];
                    }
                }
            }
            if let Some(action) = args
                .iter()
                .position(|a| a == "--smoke-shortcut")
                .and_then(|i| args.get(i + 1))
            {
                let id = egui::Id::new("qa-shortcut-action");
                if self.startup.elapsed() > Duration::from_millis(500)
                    && !ctx.data(|d| d.get_temp::<bool>(id).unwrap_or(false))
                {
                    ctx.data_mut(|d| d.insert_temp(id, true));
                    let _ = self.engine.tx.send(Event::Shortcut(0, action.clone()));
                }
            }
            if args.iter().any(|a| a == "--smoke-win-bind")
                && !ctx.data(|d| {
                    d.get_temp::<bool>(egui::Id::new("qa-win-bind"))
                        .unwrap_or(false)
                })
            {
                ctx.data_mut(|d| d.insert_temp(egui::Id::new("qa-win-bind"), true));
                self.shortcut_capture = Some("ai".into());
                ctx.input_mut(|i| {
                    i.events.push(egui::Event::Key {
                        key: egui::Key::K,
                        physical_key: Some(egui::Key::K),
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers {
                            ctrl: true,
                            mac_cmd: true,
                            ..Default::default()
                        },
                    })
                });
            }
            if args.iter().any(|a| a == "--smoke-shortcut-conflict")
                && self.startup.elapsed() > Duration::from_millis(500)
                && !ctx.data(|d| {
                    d.get_temp::<bool>(egui::Id::new("qa-shortcut-conflict"))
                        .unwrap_or(false)
                })
            {
                ctx.data_mut(|d| d.insert_temp(egui::Id::new("qa-shortcut-conflict"), true));
                self.s.set_binding("ai", "Ctrl+Alt+K".into());
                self.shortcut_capture = Some("web".into());
                self.capture_binding("Ctrl+Alt+K".into());
            }
            if args.iter().any(|a| a == "--smoke-pin-selected")
                && self.tool == "Clipboard"
                && !self.tool_items.is_empty()
                && self.startup.elapsed() > Duration::from_secs(1)
                && !ctx.data(|d| {
                    d.get_temp::<bool>(egui::Id::new("qa-pin-clip"))
                        .unwrap_or(false)
                })
            {
                ctx.data_mut(|d| d.insert_temp(egui::Id::new("qa-pin-clip"), true));
                self.pin_selected();
            }
        }

        if self.shortcut_capture.is_some() {
            ctx.request_repaint_after(Duration::from_millis(10));
            if let Some(value) = crate::platform::capture_windows_shortcut() {
                self.capture_binding(value);
                return;
            }
            let key = ctx.input_mut(|i| {
                let key = i.events.iter().find_map(|e| {
                    if let egui::Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } = e
                    {
                        Some((*key, *modifiers))
                    } else {
                        None
                    }
                });
                i.events.retain(|e| !matches!(e, egui::Event::Key { .. }));
                key
            });
            if let Some((key, mods)) = key {
                if key == egui::Key::Escape {
                    self.shortcut_capture = None;
                } else {
                    let value = crate::ai::shortcut(key, mods);
                    self.capture_binding(value);
                }
            }
            return;
        }
        if self.page == 0
            && self.tool == "AI"
            && !self.ai_busy
            && ctx.input(|i| {
                i.modifiers.ctrl && i.key_pressed(egui::Key::V)
                    || i.events.iter().any(|e| matches!(e, egui::Event::Paste(_)))
            })
        {
            if let Ok(mut clipboard) = arboard::Clipboard::new() {
                if let Ok(image) = clipboard.get_image() {
                    if image.width as u64 * image.height as u64 <= 16000000 {
                        if let Some(rgba) = image::RgbaImage::from_raw(
                            image.width as u32,
                            image.height as u32,
                            image.bytes.into_owned(),
                        ) {
                            let mut bytes = std::io::Cursor::new(Vec::new());
                            if image::DynamicImage::ImageRgba8(rgba)
                                .write_to(&mut bytes, image::ImageFormat::Png)
                                .is_ok()
                            {
                                if let Err(e) = self.attach_ai_bytes(ctx, bytes.get_ref()) {
                                    self.ai_error = Some(e.to_string());
                                }
                                ctx.input_mut(|i| {
                                    i.events.retain(|e| !matches!(e, egui::Event::Paste(_)))
                                });
                            }
                        }
                    }
                }
            }
        }
        if self.page != 0 || self.shortcut_conflict.is_some() || self.clip_edit.is_some() {
            return;
        }
        let action = ctx.input_mut(|i| {
            let action = i.events.iter().find_map(|e| {
                if let egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } = e
                {
                    let value = crate::shortcuts::canonical(&crate::ai::shortcut(*key, *modifiers));
                    crate::shortcuts::ACTIONS
                        .iter()
                        .find(|a| {
                            !a.2 && !matches!(a.0, "cycle_modes" | "cycle_types")
                                && !self.saved_settings.binding(a.0).is_empty()
                                && crate::shortcuts::canonical(self.saved_settings.binding(a.0))
                                    == value
                        })
                        .map(|a| a.0)
                } else {
                    None
                }
            });
            if action.is_some() {
                i.events.retain(|e| !matches!(e, egui::Event::Key { .. }));
            }
            action
        });
        match action {
            Some("pin_clip") if self.tool == "Clipboard" => self.pin_selected(),
            Some("edit_clip") if self.tool == "Clipboard" => self.edit_selected(),
            Some("paste_plain") if self.tool == "Clipboard" => {
                if let Some(item) = self.tool_items.get(self.selected).cloned() {
                    self.paste_clip(ctx, item);
                }
            }
            Some("new_ai") if self.tool == "AI" => self.new_conversation(),
            Some("read_ai") if self.tool == "AI" => {
                if let Some(text) = self
                    .ai_messages
                    .iter()
                    .rev()
                    .find(|m| m.role == "assistant")
                    .map(|m| m.content.clone())
                {
                    self.start_read_aloud(text);
                }
            }
            Some("save_search") if self.tool == "Search" => self.save_search_open = true,
            Some("copy_path") if self.tool == "Search" => {
                if let Some(hit) = self.hits.get(self.selected) {
                    ctx.copy_text(hit.path.clone());
                }
            }
            Some("reveal") if self.tool == "Search" => {
                if let Some(hit) = self.hits.get(self.selected) {
                    let path = hit.path.clone();
                    crate::engine::dispatch_action(self.engine.tx.clone(), move || {
                        crate::platform::reveal(&path)
                    });
                }
            }
            _ => {}
        }
    }
    pub(super) fn manager_dialogs(&mut self, ctx: &egui::Context) {
        if (self.shortcut_conflict.is_some() || self.clip_edit.is_some())
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            self.shortcut_conflict = None;
            self.clip_edit = None;
        }

        if let Some((action, value, old)) = self.shortcut_conflict.clone() {
            let mut choice = None;
            dialog_window(ctx, t("Shortcut already assigned"), 460.0)
                .default_height(140.0)
                .resizable(false)
                .show(ctx, |ui| {
                    let label = crate::shortcuts::ACTIONS
                        .iter()
                        .find(|a| a.0 == old)
                        .map_or(old.as_str(), |a| a.1);
                    ui.label(format!("{value} · {}", t(label)));
                    ui.label(t(
                        "Use this shortcut for VectorDash? Its previous action will no longer receive this combination while VectorDash is running.",
                    ));
                    ui.horizontal(|ui| {
                        if ui.button(t("Override")).clicked() {
                            choice = Some(true);
                        }
                        if ui.button(t("Cancel")).clicked() {
                            choice = Some(false);
                        }
                    });
                });
            if let Some(force) = choice {
                if force {
                    let _ = self.s.assign_binding(&action, value, true);
                    if old == "Windows / another application" {
                        self.s.forced_shortcuts.insert(action);
                    }
                }
                self.shortcut_conflict = None;
            }
        }
        if let Some((id, mut title, mut tags, mut text)) = self.clip_edit.clone() {
            let mut save = false;
            let mut cancel = false;
            dialog_window(ctx, t("Edit clipboard item"), 560.0).show(ctx, |ui| {
                ui.label(t("Name"));
                ui.text_edit_singleline(&mut title);
                ui.label(t("Tags (comma separated)"));
                ui.text_edit_singleline(&mut tags);
                if !text.is_empty() {
                    egui::ScrollArea::vertical()
                        .max_height(260.0)
                        .show(ui, |ui| {
                            ui.add(
                                egui::TextEdit::multiline(&mut text).desired_width(f32::INFINITY),
                            );
                        });
                }
                ui.horizontal(|ui| {
                    save = ui.button(t("Save")).clicked();
                    cancel = ui.button(t("Cancel")).clicked();
                });
            });
            self.clip_edit = Some((id, title.clone(), tags.clone(), text.clone()));
            if save {
                let store =
                    crate::launcher::Clipboard::new(&self.home).configured(&self.saved_settings);
                let tx = self.engine.tx.clone();
                std::thread::spawn(move || {
                    let result = (|| {
                        if !text.is_empty() {
                            store.edit(id, &text)?;
                        }
                        store.annotate(id, &title, &tags)
                    })();
                    let _ = tx.send(Event::ClipboardCleared(
                        result.map_err(|e: anyhow::Error| e.to_string()),
                    ));
                });
                self.clip_edit = None;
            }
            if cancel {
                self.clip_edit = None;
            }
        }
    }
    pub(super) fn clipboard_settings(&mut self, ui: &mut egui::Ui) {
        ui.heading(t("Clipboard"));
        if self.clip_usage.is_none() {
            self.clip_usage = Some((0, 0, 0));
            let store = crate::launcher::Clipboard::new(&self.home);
            let tx = self.engine.tx.clone();
            std::thread::spawn(move || {
                if let Ok(stats) = store.stats() {
                    let _ = tx.send(Event::ClipboardUsage(stats));
                }
            });
        }
        if let Some((count, saved, bytes)) = self.clip_usage {
            ui.weak(format!(
                "{count} {} · {saved} {} · {:.1} MiB",
                t("items"),
                t("Saved"),
                bytes as f64 / 1048576.0
            ));
        }
        if ui.small_button(t("Refresh usage")).clicked() {
            self.clip_usage = None;
        }

        ui.checkbox(
            &mut self.s.clipboard_enabled,
            t("Save copied text, links and images"),
        );
        ui.group(|ui| {
            ui.label(RichText::new(t("Capture")).strong());
            ui.checkbox(
                &mut self.s.clipboard_options.capture_text,
                t("Save copied text and links"),
            );
            ui.checkbox(
                &mut self.s.clipboard_options.capture_images,
                t("Save copied images"),
            );
            ui.label(t(
                "Excluded applications (executable names, comma separated)",
            ));
            ui.text_edit_singleline(&mut self.s.clipboard_excluded_apps);
            ui.horizontal(|ui| {
                ui.label(t("Pause duration (minutes)"));
                ui.add(
                    egui::DragValue::new(&mut self.s.clipboard_options.pause_minutes)
                        .range(1..=1440),
                );
            });
            ui.horizontal(|ui| {
                if ui
                    .button(t(if crate::launcher::paused() {
                        "Resume capture"
                    } else {
                        "Pause capture"
                    }))
                    .clicked()
                {
                    crate::launcher::pause(if crate::launcher::paused() {
                        0
                    } else {
                        self.s.clipboard_options.pause_minutes
                    });
                }
                if ui.button(t("Ignore next copy")).clicked() {
                    crate::launcher::ignore_next();
                }
            });
        });
        ui.group(|ui| {
      ui.label(RichText::new(t("Retention and storage")).strong());
      retention_picker(ui,"Text history",&mut self.s.clipboard_hours);
      let mut same=self.s.clipboard_options.image_hours.is_none();if ui.checkbox(&mut same,t("Use text retention for images")).changed(){self.s.clipboard_options.image_hours=if same{None}else{Some(self.s.clipboard_hours)};}
      if let Some(hours)=&mut self.s.clipboard_options.image_hours {retention_picker(ui,"Image history",hours);}
      for (label,value,max) in [("Maximum history items (0 = unlimited)",&mut self.s.clipboard_options.max_items,100000),("History storage MiB (0 = unlimited)",&mut self.s.clipboard_options.max_mb,16384),("Saved storage MiB (0 = unlimited)",&mut self.s.clipboard_options.max_saved_mb,16384),("Maximum text size (KiB)",&mut self.s.clipboard_options.max_text_kib,1024),("Maximum image size (MiB)",&mut self.s.clipboard_options.max_image_mib,4)] {ui.horizontal(|ui|{ui.label(t(label));ui.add(egui::DragValue::new(value).range(if label.starts_with("Maximum text")||label.starts_with("Maximum image"){1..=max}else{0..=max}));});}
      ui.weak(t("Forever disables expiry. History limits can still remove old unsaved items. Saved items are never automatically removed."));
    });
        ui.group(|ui|{
      ui.label(RichText::new(t("Search and paste")).strong());
      ui.checkbox(&mut self.s.clipboard_options.paste_on_enter,t("Enter pastes into the previous application"));
      ui.weak(t("Text is pasted without formatting. Image items keep their image format."));
      ui.checkbox(&mut self.s.clipboard_options.ocr,t("Recognize text in copied images (OCR)"));
      ui.checkbox(&mut self.s.clipboard_image_embeddings,t("Use EmbeddingGemma 2 to search copied images by meaning"));
      ui.weak(t("OCR and image embeddings run only when idle conditions allow. Remote embedding servers receive the copied image when enabled."));
    });
        ui.group(|ui|{
      ui.label(RichText::new(t("Saved items and cleanup")).strong());
      ui.weak(t("Save items from their context menu. Names and tags are searchable. Exported backups contain readable clipboard content."));
      ui.horizontal(|ui|{
        if ui.button(t("Export saved items")).clicked(){if let Some(path)=rfd::FileDialog::new().set_file_name("clipboard-saved.jsonl").save_file(){let store=crate::launcher::Clipboard::new(&self.home);crate::engine::dispatch_action(self.engine.tx.clone(),move||store.export_saved(&path));}}
        if ui.button(t("Import saved items")).clicked(){if let Some(path)=rfd::FileDialog::new().pick_file(){let store=crate::launcher::Clipboard::new(&self.home).configured(&self.s);let tx=self.engine.tx.clone();std::thread::spawn(move||{let _=tx.send(Event::ClipboardCleared(store.import_saved(&path).map_err(|e|e.to_string())));});}}
      });
      ui.horizontal_wrapped(|ui|{for (label,seconds) in [("Clear last 15 minutes",Some(900)),("Clear last hour",Some(3600)),("Clear unsaved history",None)] {if ui.button(t(label)).clicked(){let store=crate::launcher::Clipboard::new(&self.home);let tx=self.engine.tx.clone();std::thread::spawn(move||{let _=tx.send(Event::ClipboardCleared(store.clear_recent(seconds).map_err(|e|e.to_string())));});}}});
      if ui.add(egui::Button::new(t("Clear clipboard history")).stroke(egui::Stroke::new(1.0,Color32::RED))).clicked(){self.clipboard_clear_confirm=true;}
    });
    }
    pub(super) fn pin_selected(&mut self) {
        if let Some(item) = self.tool_items.get(self.selected) {
            let id = item.id;
            let store =
                crate::launcher::Clipboard::new(&self.home).configured(&self.saved_settings);
            let tx = self.engine.tx.clone();
            std::thread::spawn(move || {
                let result = store.pinned(id).and_then(|v| store.pin(id, !v));
                let _ = tx.send(Event::ClipboardCleared(result.map_err(|e| e.to_string())));
            });
        }
    }
    pub(super) fn edit_selected(&mut self) {
        if let Some(item) = self.tool_items.get(self.selected) {
            let store = crate::launcher::Clipboard::new(&self.home);
            if let Ok((title, tags)) = store.metadata(item.id) {
                self.clip_edit = Some((item.id, title, tags, item.value.clone()));
            }
        }
    }
    pub(super) fn clipboard_item_menu(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        item: &crate::launcher::Item,
    ) {
        if ui.button(t("Pin / unpin item")).clicked() {
            let store =
                crate::launcher::Clipboard::new(&self.home).configured(&self.saved_settings);
            let id = item.id;
            let tx = self.engine.tx.clone();
            std::thread::spawn(move || {
                let result = store.sticky(id).and_then(|v| store.stick(id, !v));
                let _ = tx.send(Event::ClipboardCleared(result.map_err(|e| e.to_string())));
            });
            ui.close();
        }
        if ui.button(t("Save / unsave forever")).clicked() {
            self.selected = self
                .tool_items
                .iter()
                .position(|v| v.id == item.id)
                .unwrap_or(0);
            self.pin_selected();
            ui.close();
        }
        if ui.button(t("Rename / edit / tags")).clicked() {
            self.selected = self
                .tool_items
                .iter()
                .position(|v| v.id == item.id)
                .unwrap_or(0);
            self.edit_selected();
            ui.close();
        }
        if ui.button(t("Paste to previous app")).clicked() {
            self.paste_clip(ctx, item.clone());
            ui.close();
        }
        if item.image && ui.button(t("Save image as file")).clicked() {
            if let Some(path) = rfd::FileDialog::new()
                .set_file_name("clipboard.png")
                .save_file()
            {
                let store = crate::launcher::Clipboard::new(&self.home);
                let id = item.id;
                crate::engine::dispatch_action(self.engine.tx.clone(), move || {
                    std::fs::write(path, store.image(id)?)?;
                    Ok(())
                });
            }
            ui.close();
        }
    }
    pub(super) fn clipboard_toolbar(&mut self, ui: &mut egui::Ui, _ctx: &egui::Context) {
        if !self.clip_selected.is_empty() {
            ui.horizontal_wrapped(|ui| {
                ui.label(format!("{} {}", self.clip_selected.len(), t("selected")));
                if ui.button(t("Copy selected text")).clicked() {
                    let text = self
                        .tool_items
                        .iter()
                        .filter(|v| self.clip_selected.contains(&v.id) && !v.image)
                        .map(|v| v.value.as_str())
                        .collect::<Vec<_>>()
                        .join("\n");
                    _ctx.copy_text(text);
                }
                if ui.button(t("Delete selected")).clicked() {
                    let ids = std::mem::take(&mut self.clip_selected);
                    let store = crate::launcher::Clipboard::new(&self.home);
                    let tx = self.engine.tx.clone();
                    std::thread::spawn(move || {
                        let result = ids.into_iter().try_for_each(|id| store.remove(id));
                        let _ = tx.send(Event::ClipboardCleared(result.map_err(|e| e.to_string())));
                    });
                }
                if ui.button(t("Clear selection")).clicked() {
                    self.clip_selected.clear();
                }
            });
        }
        ui.horizontal_wrapped(|ui| {
            for label in ["All", "Pinned items", "Saved", "Text", "Images", "Links"] {
                if ui
                    .selectable_value(&mut self.clip_filter, label.into(), t(label))
                    .changed()
                {
                    self.changed();
                }
            }
            if ui
                .small_button(t(if crate::launcher::paused() {
                    "Resume capture"
                } else {
                    "Pause capture"
                }))
                .clicked()
            {
                crate::launcher::pause(if crate::launcher::paused() {
                    0
                } else {
                    self.saved_settings.clipboard_options.pause_minutes
                });
            }
        });
    }
    pub(super) fn paste_clip(&mut self, ctx: &egui::Context, item: crate::launcher::Item) {
        if self.target == 0 {
            self.copy_tool_item(ctx, item);
            self.status = "No previous application; copied to clipboard".into();
            return;
        }
        let target = self.target;
        let store = crate::launcher::Clipboard::new(&self.home);
        self.close_window(ctx);
        crate::engine::dispatch_action(self.engine.tx.clone(), move || {
            if item.image {
                store.copy_image(item.id)?;
            } else {
                arboard::Clipboard::new()?.set_text(item.value)?;
            }
            crate::platform::paste_to_window(target)
        });
    }
    pub(super) fn attach_ai_bytes(
        &mut self,
        ctx: &egui::Context,
        bytes: &[u8],
    ) -> anyhow::Result<()> {
        let url = crate::capture::vision(bytes, None)?;
        let rgba = crate::capture::rgba(bytes)?;
        let thumb = image::DynamicImage::ImageRgba8(rgba)
            .thumbnail(240, 160)
            .to_rgba8();
        self.ai_attachment = Some(ctx.load_texture(
            "ai-attachment",
            egui::ColorImage::from_rgba_unmultiplied(
                [thumb.width() as usize, thumb.height() as usize],
                thumb.as_raw(),
            ),
            egui::TextureOptions::LINEAR,
        ));
        self.ai_images = vec![url];
        Ok(())
    }
    pub(super) fn start_area_capture(&mut self, ctx: &egui::Context) {
        if self.smoke.is_some() {
            let args = std::env::args().collect::<Vec<_>>();
            if let Some(path) = args
                .iter()
                .position(|a| a == "--smoke-capture-fixture")
                .and_then(|i| args.get(i + 1))
            {
                let _ = self.engine.tx.send(Event::AreaCapture(
                    std::fs::read(path).map_err(|e| e.to_string()),
                ));
                return;
            }
        }
        if self.capture_busy {
            return;
        }
        self.capture_return_visible = self.search_visible && crate::platform::main_window_visible();
        self.capture_busy = true;
        self.search_visible = false;
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        self.capture_open = false;
        self.capture_bytes = None;
        self.capture_texture = None;
        let tx = self.engine.tx.clone();
        crate::platform::hide_main_window();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(120));
            let result = crate::capture::select_area().map_err(|e| e.to_string());
            let _ = tx.send(Event::AreaCapture(result));
        });
    }
}

fn retention_picker(ui: &mut egui::Ui, label: &str, hours: &mut u32) {
    ui.horizontal(|ui| {
        ui.label(t(label));
        let options = [
            (0, "Forever"),
            (1, "1 hour"),
            (24, "1 day"),
            (168, "1 week"),
            (720, "1 month"),
            (2160, "3 months"),
            (8760, "1 year"),
        ];
        egui::ComboBox::from_id_salt(("clipboard-retention", label))
            .selected_text(t(options
                .iter()
                .find(|(value, _)| value == hours)
                .map_or("Custom", |(_, label)| label)))
            .show_ui(ui, |ui| {
                for (value, label) in options {
                    ui.selectable_value(hours, value, t(label));
                }
                if ui
                    .selectable_label(!options.iter().any(|(v, _)| v == hours), t("Custom"))
                    .clicked()
                {
                    *hours = 48;
                }
            });
        if !options.iter().any(|(v, _)| v == hours) {
            ui.add(
                egui::DragValue::new(hours)
                    .range(1..=87600)
                    .suffix(t(" hours")),
            );
        }
    });
}
