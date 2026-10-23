use super::*;

impl App {
    pub(super) fn search_order_settings(&mut self, ui: &mut egui::Ui) {
        ui.group(|ui| {
            ui.label(RichText::new(t("Search mode order")).strong());
            ui.weak(t(
                "Reorder the buttons and keyboard cycle. Disabled modes stay hidden.",
            ));
            let mut order = self.s.ordered_search_tools();
            let mut move_item = None;
            for (index, tool) in order.iter().enumerate() {
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(index > 0, egui::Button::new("↑"))
                        .on_hover_text(t("Move up"))
                        .clicked()
                    {
                        move_item = Some((index, index - 1));
                    }
                    if ui
                        .add_enabled(index + 1 < order.len(), egui::Button::new("↓"))
                        .on_hover_text(t("Move down"))
                        .clicked()
                    {
                        move_item = Some((index, index + 1));
                    }
                    ui.label(t(tool));
                });
            }
            if let Some((a, b)) = move_item {
                order.swap(a, b);
                self.s.search_tool_order = order.into_iter().map(str::to_owned).collect();
            }
        });
    }
    pub(super) fn search_tool_buttons(&mut self, ui: &mut egui::Ui) {
        for label in self.saved_settings.search_tools() {
            if ui
                .add(
                    egui::Button::image_and_text(
                        crate::icons::image(
                            match label {
                                "Search" => crate::icons::Icon::Search,
                                "Clipboard" => crate::icons::Icon::Clipboard,
                                "Web" => crate::icons::Icon::Web,
                                _ => crate::icons::Icon::Robot,
                            },
                            self.accent(),
                            18.0,
                        ),
                        t(label),
                    )
                    .selected(self.tool == label),
                )
                .clicked()
            {
                self.switch_tool(label);
            }
        }
    }
    pub(super) fn web_settings(&mut self, ui: &mut egui::Ui) {
        self.settings_anchor(ui,"Web");
        ui.heading(t("Web"));
        ui.checkbox(&mut self.s.web.enabled, t("Enable web search"));
        ui.weak(t("Searches run only when you press Enter or Search."));
        ui.checkbox(&mut self.s.web.previews, "Show provider thumbnail previews");
        ui.weak(t("Off by default. Loads small images only when the provider supplies them; contacts the image host. No page screenshots or persistent cache."));
        ui.add_space(10.0);
        ui.add_enabled_ui(self.s.web.enabled, |ui| {
            ui.horizontal(|ui| {
                ui.label(t("Concurrent AI web searches"));
                self.s.web.ai_concurrent_searches=self.s.web.ai_concurrency();
                egui::ComboBox::from_id_salt("ai-web-concurrency").selected_text(self.s.web.ai_concurrent_searches.to_string()).show_ui(ui,|ui| {
                    for count in 1..=2 { ui.selectable_value(&mut self.s.web.ai_concurrent_searches,count,count.to_string()); }
                });
            });
            ui.weak(t("1 uses less server load; 2 can search two queries together. This does not limit how many searches the AI may request."));
        });
        ui.add_enabled_ui(self.s.web.enabled,|ui| {
            for provider in crate::web_search::PROVIDERS {
                ui.group(|ui| {
                    ui.set_min_width(ui.available_width());
                    ui.heading(provider);
                    if provider!="Brave" { ui.horizontal(|ui| {
                        ui.label(t("Instance URL"));
                        let value=if provider=="SearXNG" { &mut self.s.web.searxng_url } else { &mut self.s.web.degoog_url };
                        ui.add(egui::TextEdit::singleline(value).desired_width(440.0).hint_text(if provider=="SearXNG" { "https://searx.example.org" } else { "http://localhost:4444" }));
                    }); }
                    ui.horizontal(|ui| {
                        ui.label(t(if provider=="Brave" { "API key" } else { "API key (optional)" }));
                        let value=match provider { "SearXNG"=>&mut self.s.web.searxng_key,"Degoog"=>&mut self.s.web.degoog_key,_=>&mut self.s.web.brave_key };
                        ui.add(egui::TextEdit::singleline(value).password(true).desired_width(440.0));
                    });
                    ui.weak(t(match provider {
                        "SearXNG"=>"Enable JSON output in your SearXNG instance. An optional key is sent as a bearer token.",
                        "Degoog"=>"Use your Degoog instance URL. Add its API key if search routes are protected.",
                        _=>"Use a Brave Search API key with Web Search access.",
                    }));
                    ui.hyperlink_to(t("API documentation"),match provider {
                        "SearXNG"=>"https://docs.searxng.org/dev/search_api.html",
                        "Degoog"=>"https://degoog-org.github.io/docs/api.html",
                        _=>"https://api-dashboard.search.brave.com/documentation/services/web-search",
                    });
                });
                ui.add_space(8.0);
            }
            let providers=self.s.web.providers();
            if !providers.is_empty() {
                self.s.web.provider=self.s.web.selected(&self.s.web.provider);
                ui.horizontal(|ui| {
                    ui.label(t("Default web provider"));
                    egui::ComboBox::from_id_salt("default-web-provider").selected_text(&self.s.web.provider).show_ui(ui,|ui| {
                        for provider in &providers { ui.selectable_value(&mut self.s.web.provider,(*provider).into(),*provider); }
                    });
                });
                ui.horizontal(|ui| {
                    ui.label(t("AI web search provider"));
                    let selected = if self.s.web.ai_provider.is_empty() { t("Same as default web provider").to_owned() } else { self.s.web.ai_selected() };
                    egui::ComboBox::from_id_salt("ai-web-provider").selected_text(selected).show_ui(ui,|ui| {
                        ui.selectable_value(&mut self.s.web.ai_provider,String::new(),t("Same as default web provider"));
                        for provider in providers { ui.selectable_value(&mut self.s.web.ai_provider,provider.into(),provider); }
                    });
                });
            }
        });
    }
    pub(super) fn web_provider_picker(&mut self, ui: &mut egui::Ui, rect: egui::Rect) {
        let old = self.web.provider.clone();
        ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
            if self.smoke.is_some()
                && self.startup.elapsed() > Duration::from_millis(400)
                && std::env::args().any(|a| a == "--smoke-web-provider-picker")
                && !ui.ctx().data(|d| {
                    d.get_temp::<bool>(egui::Id::new("qa-web-provider-opened"))
                        .unwrap_or(false)
                })
            {
                egui::Popup::open_id(
                    ui.ctx(),
                    ui.make_persistent_id(egui::Id::new("web-provider-picker"))
                        .with("popup"),
                );
                ui.ctx()
                    .data_mut(|d| d.insert_temp(egui::Id::new("qa-web-provider-opened"), true));
            }
            egui::ComboBox::from_id_salt("web-provider-picker")
                .truncate()
                .width(rect.width())
                .selected_text(RichText::new(&self.web.provider).size(11.0))
                .show_ui(ui, |ui| {
                    for provider in self.saved_settings.web.providers() {
                        let response = ui.selectable_value(
                            &mut self.web.provider,
                            provider.into(),
                            RichText::new(provider).size(11.0),
                        );
                        if self.smoke.is_some() {
                            ui.ctx().data_mut(|d| {
                                d.insert_temp(
                                    egui::Id::new(format!("qa-web-provider-{provider}")),
                                    response.rect,
                                );
                                d.insert_temp(egui::Id::new("qa-web-provider-last"), response.rect);
                            });
                        }
                    }
                });
        });
        if self.web.provider != old {
            self.web.clear();
            self.changed();
        }
    }
    pub(super) fn ask_web(&mut self) {
        if self.web.busy
            || !self.saved_settings.web.enabled
            || self.dictation_busy
            || self.query.trim().is_empty()
        {
            return;
        }
        if self.saved_settings.web.providers().is_empty() {
            self.web.error = Some(t("Add a provider in Web settings").into());
            return;
        }
        self.web.provider = self.saved_settings.web.selected(&self.web.provider);
        self.reset_result_selection();
        self.web_thumbs.clear();
        self.web_thumb_pending.clear();
        let (id, cancel) = self.web.start(&self.query);
        let settings = self.saved_settings.web.clone();
        let provider = self.web.provider.clone();
        let query = self.web.submitted.clone();
        let tx = self.engine.tx.clone();
        std::thread::spawn(move || {
            let started = Instant::now();
            let result = crate::web_search::search(&settings, &provider, &query, &cancel)
                .map_err(|e| e.to_string());
            let _ = tx.send(Event::WebDone(
                id,
                result,
                started.elapsed().as_secs_f64() * 1000.0,
            ));
        });
    }
    pub(super) fn web_results(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if self.web.hits.is_empty() {
            if !self.web.busy {
                ui.weak(t(
                    if self.web.elapsed_ms.is_some() && self.web.error.is_none() {
                        "No web results"
                    } else {
                        "Press Enter to search the web"
                    },
                ));
            }
            return;
        }
        let mut activate = None;
        egui::ScrollArea::vertical()
            .id_salt("web-results")
            .min_scrolled_height(0.0)
            .max_height(ui.available_height().max(0.0))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                for (index, hit) in self.web.hits.iter().enumerate() {
                    let accent = self.accent();
                    let frame = egui::Frame::new()
                        .corner_radius(12)
                        .inner_margin(10)
                        .fill(if self.selected == index {
                            accent.gamma_multiply(0.23)
                        } else {
                            ui.visuals().faint_bg_color
                        })
                        .stroke(egui::Stroke::new(
                            1.0_f32,
                            if self.selected == index {
                                accent
                            } else {
                                Color32::TRANSPARENT
                            },
                        ))
                        .show(ui, |ui| {
                            ui.set_min_width(ui.available_width());
                            let thumb = if self.saved_settings.web.previews {
                                hit.thumbnail.as_ref()
                            } else {
                                None
                            };
                            let width = ui.available_width();
                            if let Some(url) = thumb {
                                if !self.web_thumbs.contains_key(url)
                                    && !self.web_thumb_pending.contains(url)
                                    && self.web_thumbs.len() + self.web_thumb_pending.len() < 24
                                    && self.web_thumb_pending.len() < 2
                                    && ui.is_rect_visible(egui::Rect::from_min_size(
                                        ui.cursor().min,
                                        egui::vec2(width, 100.0),
                                    ))
                                {
                                    self.web_thumb_pending.insert(url.clone());
                                    let url = url.clone();
                                    let tx = self.engine.tx.clone();
                                    std::thread::spawn(move || {
                                        let bytes = crate::web_search::thumbnail(&url).ok();
                                        let _ = tx.send(Event::WebThumbnail(url, bytes));
                                    });
                                }
                            }
                            let texture = thumb
                                .and_then(|url| self.web_thumbs.get(url))
                                .and_then(Option::as_ref);
                            let bounds = web_result_body(ui, hit, texture);
                            if self.smoke.is_some() && index == 0 {
                                ctx.data_mut(|d| {
                                    d.insert_temp(egui::Id::new("qa-web-columns"), bounds)
                                });
                            }
                        });
                    let response = ui
                        .interact(
                            frame.response.rect,
                            ui.id().with(("web-result", index)),
                            egui::Sense::click(),
                        )
                        .on_hover_cursor(egui::CursorIcon::PointingHand);
                    if self.smoke.is_some() && index == 0 {
                        ctx.data_mut(|d| {
                            d.insert_temp(egui::Id::new("qa-web-first"), response.rect)
                        });
                    }
                    if self.scroll_to_selection && self.selected == index {
                        response.scroll_to_me(Some(if index == 0 {
                            egui::Align::Min
                        } else {
                            egui::Align::Center
                        }));
                        self.scroll_to_selection = false;
                    }
                    if response.clicked() {
                        self.selected = index;
                        activate = Some(index);
                    }
                    ui.add_space(6.0);
                }
            });
        if let Some(index) = activate {
            self.open_web_result(ctx, index);
        }
    }
    pub(super) fn open_web_result(&mut self, ctx: &egui::Context, index: usize) {
        let Some(url) = self
            .web
            .hits
            .get(index)
            .and_then(|h| crate::web_search::result_url(&h.url))
        else {
            return;
        };
        let url = url.to_string();
        if self.smoke.is_some() && std::env::args().any(|a| a == "--smoke-web-no-browser") {
            ctx.data_mut(|d| d.insert_temp(egui::Id::new("qa-web-opened"), url));
        } else {
            crate::engine::dispatch_action(self.engine.tx.clone(), move || {
                crate::platform::open(&url)
            });
        }
        if self.quick {
            self.close_window(ctx);
        }
    }
    pub(super) fn web_footer(&self) -> String {
        if self.web.busy {
            return format!(
                "{} · {} · {:.1} s",
                t("Searching the web…"),
                self.web.provider,
                self.web.started.map_or(0.0, |t| t.elapsed().as_secs_f64())
            );
        }
        if let Some(error) = &self.web.error {
            return t(error).to_owned();
        }
        if let Some(ms) = self.web.elapsed_ms {
            return format!("{} {} · {:.1} ms", self.web.hits.len(), t("results"), ms);
        }
        t("Press Enter to search the web").into()
    }
    pub(super) fn web_smoke_closed_report(&self, ctx: &egui::Context) {
        if self.tool != "Web" || self.search_visible {
            return;
        }
        if let Some(path) = &self.smoke {
            let data = serde_json::json!({"tool":self.tool,"tools":self.saved_settings.search_tools(),"embedding_activity":false,"main_visible":crate::platform::main_window_visible(),"web":{
                "busy":self.web.busy,"results":self.web.hits,"provider":self.web.provider,"error":self.web.error,"opened":ctx.data(|d|d.get_temp::<String>(egui::Id::new("qa-web-opened"))),"window_visible":self.search_visible,
                "outside_during":ctx.data(|d|d.get_temp::<bool>(egui::Id::new("qa-web-outside-during")).unwrap_or(false)),"outside_after":ctx.data(|d|d.get_temp::<bool>(egui::Id::new("qa-web-outside-after")).unwrap_or(false))
            }});
            let _ = std::fs::write(path, serde_json::to_vec_pretty(&data).unwrap_or_default());
        }
    }
    pub(super) fn web_smoke_tick(&mut self, ctx: &egui::Context) {
        if self.smoke.is_none() || self.tool != "Web" {
            return;
        }
        let release =
            ctx.data_mut(|d| d.remove_temp::<egui::Pos2>(egui::Id::new("qa-web-release")));
        if let Some(pos) = release {
            self.qa_web_events.push(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            });
            ctx.request_repaint();
            return;
        }
        if self.startup.elapsed() < Duration::from_millis(850) {
            return;
        }
        let args = std::env::args().collect::<Vec<_>>();
        let pick = args
            .iter()
            .position(|a| a == "--smoke-web-pick")
            .and_then(|i| args.get(i + 1));
        let target = if let Some(provider) = pick {
            if !ctx.data(|d| {
                d.get_temp::<bool>(egui::Id::new("qa-web-picked"))
                    .unwrap_or(false)
            }) {
                ctx.data(|d| {
                    d.get_temp::<egui::Rect>(egui::Id::new(format!("qa-web-provider-{provider}")))
                })
                .map(|r| (r, "qa-web-picked"))
            } else {
                None
            }
        } else if args.iter().any(|a| a == "--smoke-web-open")
            && self.startup.elapsed() > Duration::from_secs(2)
            && !ctx.data(|d| {
                d.get_temp::<bool>(egui::Id::new("qa-web-row-clicked"))
                    .unwrap_or(false)
            })
        {
            ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::new("qa-web-first")))
                .map(|r| (r, "qa-web-row-clicked"))
        } else {
            None
        };
        if let Some((rect, id)) = target {
            ctx.data_mut(|d| d.insert_temp(egui::Id::new(id), true));
            self.qa_web_events
                .push(egui::Event::PointerMoved(rect.center()));
            self.qa_web_events.push(egui::Event::PointerButton {
                pos: rect.center(),
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            });
            ctx.data_mut(|d| d.insert_temp(egui::Id::new("qa-web-release"), rect.center()));
            ctx.request_repaint();
        }
    }
    pub(super) fn web_smoke_outside(&self, ctx: &egui::Context) -> bool {
        if self.smoke.is_none() || self.tool != "Web" {
            return false;
        }
        let during = self.web.busy
            && self.startup.elapsed() > Duration::from_millis(1300)
            && std::env::args().any(|a| a == "--smoke-web-outside-during");
        let after = !self.web.busy
            && self.web.elapsed_ms.is_some()
            && self.startup.elapsed() > Duration::from_secs(3)
            && std::env::args().any(|a| a == "--smoke-web-outside-after");
        let key = egui::Id::new(if during {
            "qa-web-outside-during"
        } else {
            "qa-web-outside-after"
        });
        if (during || after) && !ctx.data(|d| d.get_temp::<bool>(key).unwrap_or(false)) {
            ctx.data_mut(|d| d.insert_temp(key, true));
            return true;
        }
        false
    }
}

// Allocate text and image as siblings: shrinking a parent after setting its minimum
// width cannot reserve a thumbnail column and allowed snippets to paint over it.
fn web_result_body(
    ui: &mut egui::Ui,
    hit: &crate::web_search::Hit,
    texture: Option<&egui::TextureHandle>,
) -> (egui::Rect, Option<egui::Rect>) {
    let width = ui.available_width();
    let texture = texture.filter(|_| width >= 280.0);
    let image_width = if texture.is_some() { 136.0 } else { 0.0 };
    let gap = if texture.is_some() { 12.0 } else { 0.0 };
    let mut text_rect = egui::Rect::NOTHING;
    let mut image_rect = None;
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        let text = ui.allocate_ui_with_layout(
            egui::vec2((width - image_width - gap).max(1.0), 0.0),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.set_width((width - image_width - gap).max(1.0));
                ui.set_clip_rect(ui.clip_rect().intersect(egui::Rect::from_min_max(
                    ui.max_rect().min,
                    egui::pos2(ui.max_rect().right(), ui.clip_rect().bottom()),
                )));
                ui.add(egui::Label::new(RichText::new(&hit.title).strong().size(16.0)).truncate())
                    .on_hover_text(&hit.title);
                ui.add(
                    egui::Label::new(
                        RichText::new(&hit.url)
                            .small()
                            .color(ui.visuals().weak_text_color()),
                    )
                    .truncate(),
                )
                .on_hover_text(&hit.url);
                if !hit.snippet.is_empty() {
                    ui.add(
                        egui::Label::new(hit.snippet.chars().take(420).collect::<String>()).wrap(),
                    );
                }
                ui.add(egui::Label::new(RichText::new(&hit.source).weak()).truncate());
            },
        );
        text_rect = text.response.rect;
        if let Some(texture) = texture {
            image_rect = Some(
                ui.add(
                    egui::Image::new(texture)
                        .fit_to_exact_size(egui::vec2(image_width, 90.0))
                        .corner_radius(8),
                )
                .rect,
            );
        }
    });
    (text_rect, image_rect)
}

#[cfg(test)]
mod result_layout_tests {
    use super::*;
    #[test]
    fn long_web_results_reserve_a_separate_thumbnail_column() {
        let ctx = egui::Context::default();
        let texture = ctx.load_texture(
            "fixture",
            egui::ColorImage::filled([320, 180], Color32::LIGHT_BLUE),
            Default::default(),
        );
        let hit = crate::web_search::Hit {
            title: "A long result title ".repeat(20),
            url: format!("https://example.org/{}", "unbroken".repeat(100)),
            snippet: "A long result description that must wrap without covering the thumbnail. "
                .repeat(20),
            source: "Fixture".into(),
            thumbnail: None,
        };
        for width in [260.0, 360.0, 780.0, 1040.0] {
            for thumbnail in [None, Some(&texture)] {
                let _ = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(width, 900.0),
                        )),
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            let right = ui.max_rect().right();
                            let (text, image) = web_result_body(ui, &hit, thumbnail);
                            assert!(text.right() <= right + 1.0, "text exceeds panel: {text:?}");
                            if let Some(image) = image {
                                assert!(
                                    text.right() + 11.0 <= image.left(),
                                    "overlap: {text:?} {image:?}"
                                );
                                assert!(image.right() <= right + 1.0);
                            }
                        });
                    },
                );
            }
        }
    }
}
