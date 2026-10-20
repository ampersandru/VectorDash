use super::*;
impl App {
    pub(super) fn clear_image_query(&mut self) {
        if self.image_previous.is_none() && !self.image_preparing && self.image_query.is_none() {
            return;
        }
        self.image_generation.fetch_add(1, Ordering::AcqRel);
        self.image_query = None;
        self.image_preparing = false;
        self.image_search_submitted = false;
        self.image_search_requested = false;
        if let Some((query, category, filter)) = self.image_previous.take() {
            self.query = query;
            self.category = category;
            self.clip_filter = filter;
        }
        self.tool_items.clear();
        self.hits.clear();
        self.popup_size_state = None;
        self.changed();
    }
    fn start_image_input(&mut self, input: crate::image_search::Input) {
        if !self.saved_settings.image_search_enabled
            || !matches!(self.tool.as_str(), "Search" | "Clipboard")
        {
            return;
        }
        if self.image_previous.is_none() {
            self.image_previous = Some((
                self.query.clone(),
                self.category.clone(),
                self.clip_filter.clone(),
            ));
        }
        self.image_query = None;
        self.image_preparing = true;
        self.image_search_submitted = false;
        self.image_search_requested = false;
        self.query.clear();
        self.hits.clear();
        self.tool_items.clear();
        self.popup_size_state = None;
        self.changed();
        self.dirty = false;
        let id = self.image_generation.fetch_add(1, Ordering::AcqRel) + 1;
        crate::image_search::prepare(
            input,
            self.saved_settings.clone(),
            self.image_generation.clone(),
            id,
            self.engine.tx.clone(),
        );
    }
    pub(super) fn finish_image_input(
        &mut self,
        ctx: &egui::Context,
        result: std::result::Result<Vec<u8>, String>,
    ) {
        self.image_preparing = false;
        match result.and_then(|bytes| {
            let image = image::load_from_memory(&bytes)
                .map_err(|e| e.to_string())?
                .thumbnail(96, 64)
                .to_rgba8();
            let thumb = ctx.load_texture(
                "image-query",
                egui::ColorImage::from_rgba_unmultiplied(
                    [image.width() as usize, image.height() as usize],
                    image.as_raw(),
                ),
                egui::TextureOptions::LINEAR,
            );
            Ok((Arc::new(bytes), thumb))
        }) {
            Ok(query) => {
                self.image_query = Some(query);
                if self.tool == "Search" {
                    self.category = "Images".into();
                } else {
                    self.clip_filter = "Images".into();
                }
                self.query.clear();
                let submit = self.image_search_requested || self.saved_settings.image_search_auto;
                self.changed();
                self.image_search_requested = submit;
                self.dirty = submit;
            }
            Err(error) => {
                self.clear_image_query();
                self.search_error = Some(error);
            }
        }
        self.popup_size_state = None;
    }
    pub(super) fn image_drop_input(&mut self, ctx: &egui::Context) {
        if self.page != 0 || !matches!(self.tool.as_str(), "Search" | "Clipboard" | "AI") {
            return;
        }
        if self.smoke.is_some() {
            let args = std::env::args().collect::<Vec<_>>();
            let key = egui::Id::new("qa-drop-image");
            if let Some(path) = args
                .iter()
                .position(|a| a == "--smoke-drop-image")
                .and_then(|i| args.get(i + 1))
            {
                if !ctx.data(|d| d.get_temp::<bool>(key).unwrap_or(false)) {
                    ctx.data_mut(|d| d.insert_temp(key, true));
                    ctx.input_mut(|i| {
                        i.raw.dropped_files.push(egui::DroppedFile {
                            path: Some(path.into()),
                            ..Default::default()
                        })
                    });
                }
            }
        }
        let path = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.as_ref())
                .find(|p| crate::content::is_image(&crate::content::extension(p)))
                .cloned()
        });
        let Some(path) = path else { return };
        if self.tool != "AI" {
            self.start_image_input(crate::image_search::Input::File(path));
            return;
        }
        // Decode off the UI thread, using the same bounded media pipeline as image search.
        let id = self.image_generation.fetch_add(1, Ordering::AcqRel) + 1;
        let settings = self.saved_settings.clone();
        let tx = self.engine.tx.clone();
        let generation = self.image_generation.clone();
        self.ai_error = None;
        self.status = "Preparing image…".into();
        std::thread::spawn(move || {
            let result = (|| -> anyhow::Result<Vec<u8>> {
                let image = crate::content::decode(
                    &crate::platform::preview_path(&path.to_string_lossy()),
                    &settings,
                )?;
                crate::content::jpeg(&image, 1600)
            })()
            .map_err(|e| e.to_string());
            if generation.load(Ordering::Acquire) == id {
                let _ = tx.send(Event::AiAttachmentReady(id, result));
            }
        });
    }
    pub(super) fn image_search_input(&mut self, ctx: &egui::Context) {
        if self.page != 0
            || !self.saved_settings.image_search_enabled
            || !matches!(self.tool.as_str(), "Search" | "Clipboard")
        {
            return;
        }
        let args = if self.smoke.is_some() {
            std::env::args().collect::<Vec<_>>()
        } else {
            vec![]
        };
        if let Some(path) = args
            .iter()
            .position(|a| a == "--smoke-image-query")
            .and_then(|i| args.get(i + 1))
        {
            let key = egui::Id::new("qa-image-query-input");
            if !ctx.data(|d| d.get_temp::<bool>(key).unwrap_or(false)) {
                ctx.data_mut(|d| d.insert_temp(key, true));
                self.start_image_input(crate::image_search::Input::File(path.into()));
            }
        }
        let submit = (self.image_query.is_some()
            && args.iter().any(|a| a == "--smoke-image-submit"))
            || (self.image_preparing && args.iter().any(|a| a == "--smoke-image-early-enter"));
        let key = egui::Id::new("qa-image-enter");
        if submit && !ctx.data(|d| d.get_temp::<bool>(key).unwrap_or(false)) {
            ctx.data_mut(|d| d.insert_temp(key, true));
            ctx.input_mut(|i| {
                i.events.push(egui::Event::Key {
                    key: egui::Key::Enter,
                    physical_key: Some(egui::Key::Enter),
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                })
            });
        }
        // Handle image submission before TextEdit or focused buttons consume Enter.
        // An early key queues the request until background preprocessing completes.
        if (self.image_preparing || self.image_query.is_some() && !self.image_search_submitted)
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter))
        {
            self.image_search_requested = true;
            if !self.image_preparing {
                self.dirty = true;
            }
        }
        let paste = ctx.input(|i| {
            (i.modifiers.ctrl
                && i.key_pressed(egui::Key::V)
                && !i
                    .events
                    .iter()
                    .any(|e| matches!(e, egui::Event::Paste(text) if !text.is_empty())))
                || i.events
                    .iter()
                    .any(|e| matches!(e,egui::Event::Paste(text) if text.is_empty()))
        });
        if paste {
            if let Ok(mut clipboard) = arboard::Clipboard::new() {
                if let Ok(image) = clipboard.get_image() {
                    if image.width as u64 * image.height as u64 > 16_000_000 {
                        self.search_error =
                            Some("Image exceeds the 16 megapixel clipboard limit".into());
                        return;
                    }
                    self.start_image_input(crate::image_search::Input::Rgba {
                        width: image.width as u32,
                        height: image.height as u32,
                        bytes: image.bytes.into_owned(),
                    });
                    ctx.input_mut(|i| i.events.retain(|e| !matches!(e, egui::Event::Paste(_))));
                }
            }
        }
    }
    pub(super) fn image_query_bar(&mut self, ui: &mut egui::Ui) {
        if self.image_query.is_none() && !self.image_preparing {
            return;
        }
        ui.horizontal(|ui| {
            ui.set_min_height(54.0);
            if let Some((_, texture)) = &self.image_query {
                ui.add(
                    egui::Image::new(texture)
                        .max_size(egui::vec2(64.0, 48.0))
                        .corner_radius(6),
                );
            }
            ui.label(t(if self.image_preparing {
                "Preparing image…"
            } else {
                "Image similarity"
            }));
            if ui
                .add_enabled(
                    !self.image_preparing && !self.searching,
                    egui::Button::new(t("Find similar")),
                )
                .clicked()
            {
                self.image_search_requested = true;
                self.dirty = true;
            }
            if ui.button(t("Remove image")).clicked() {
                self.clear_image_query();
            }
        });
    }
    pub(super) fn dispatch_image_search(&mut self) {
        let Some((bytes, _)) = &self.image_query else {
            return;
        };
        let bytes = bytes.clone();
        let engine = self.engine.clone();
        let settings = self.saved_settings.clone();
        let category = self.category.clone();
        let filter = self.clip_filter.clone();
        let scope = self.scope.clone();
        let excluded = self.exclude_types.clone();
        let home = self.home.clone();
        let clipboard = self.tool == "Clipboard";
        self.image_search_requested = false;
        self.image_search_submitted = true;
        self.search_pending = false;
        self.searching = true;
        self.search_inflight = Some(self.sequence);
        self.reset_result_selection();
        let sequence = self.sequence;
        let ticket = engine.models.query_generation.load(Ordering::Acquire);
        std::thread::spawn(move || {
            let start = Instant::now();
            let check = || {
                anyhow::ensure!(
                    engine.models.query_generation.load(Ordering::Acquire) == ticket,
                    "Search superseded"
                );
                Ok(())
            };
            let store = crate::launcher::Clipboard::new(&home).configured(&settings);
            let vector = (|| -> Result<Vec<f32>> {
                if clipboard {
                    anyhow::ensure!(
                        settings.clipboard_image_embeddings,
                        "Enable clipboard image embeddings in Clipboard settings first."
                    );
                    anyhow::ensure!(
                        store.has_image_vectors(&settings)?,
                        "No compatible clipboard image embeddings found. Allow clipboard image indexing to finish first."
                    );
                } else {
                    anyhow::ensure!(
                        engine.has_visual_vectors()?,
                        "No visual embeddings found. Full-index images with image embeddings enabled first."
                    );
                }
                engine.models.image_query_checked(&settings, &bytes, &check)
            })();
            if clipboard {
                let result = vector
                    .and_then(|v| store.image_matches(&v, &settings, &filter, &check))
                    .map_err(|e| e.to_string());
                let _ = engine.tx.send(Event::Launcher(sequence, result));
            } else {
                let result = vector
                    .and_then(|v| {
                        engine.image_matches(&v, &settings, &category, &scope, &excluded, &check)
                    })
                    .map_err(|e| e.to_string());
                let _ = engine.tx.send(Event::Search(
                    sequence,
                    result,
                    start.elapsed().as_secs_f64() * 1000.0,
                ));
            }
        });
    }
}
