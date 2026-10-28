use super::*;
#[derive(Clone, Copy, Debug)]
pub(super) struct Entry {
    pub page: usize,
    pub section: &'static str,
    pub title: &'static str,
    pub aliases: &'static str,
}
include!("settings_catalog.rs");
fn normalized(text: &str) -> String {
    text.chars()
        .flat_map(char::to_lowercase)
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
fn matches(query: &str, current: usize, translate: impl Fn(&str) -> String) -> Vec<Entry> {
    let query = normalized(query);
    if query.is_empty() {
        return vec![];
    }
    let tokens = query.split_whitespace().collect::<Vec<_>>();
    let mut ranked = ENTRIES
        .iter()
        .enumerate()
        .filter_map(|(i, entry)| {
            let title = normalized(&format!("{} {}", entry.title, translate(entry.title)));
            let hay = normalized(&format!(
                "{title} {} {}",
                entry.aliases,
                translate(entry.section)
            ));
            tokens.iter().all(|q| hay.contains(q)).then_some((
                entry.page != current,
                entry.page,
                if normalized(entry.title) == query || normalized(&translate(entry.title)) == query
                {
                    0
                } else if title.contains(&query) {
                    1
                } else {
                    2
                },
                i,
                *entry,
            ))
        })
        .collect::<Vec<_>>();
    ranked.sort_by_key(|v| (v.0, v.1, v.2, v.3));
    ranked.into_iter().map(|v| v.4).collect()
}
fn panel(page: usize) -> &'static str {
    match page {
        1 => "Sources",
        2 => "Hardware",
        3 => "Scheduling",
        4 => "Settings",
        5 => "Models and AI",
        6 => "Logs",
        8 => "Web",
        9 => "Clipboard",
        _ => "About",
    }
}
impl App {
    pub(super) fn choose_setting(&mut self, entry: Entry) {
        self.page = entry.page;
        self.settings_search_open = false;
        self.settings_jump = Some((entry.page, entry.section.into()));
        self.settings_match = None;
    }
    pub(super) fn settings_anchor(&mut self, ui: &mut egui::Ui, section: &str) {
        if self
            .settings_jump
            .as_ref()
            .is_some_and(|(page, key)| *page == self.page && key == section)
        {
            ui.scroll_to_rect(
                egui::Rect::from_min_size(ui.cursor().min, egui::vec2(ui.available_width(), 32.0)),
                Some(egui::Align::TOP),
            );
            self.settings_match = Some(section.into());
            self.settings_jump = None;
            ui.ctx().request_repaint();
        }
    }
    pub(super) fn settings_search_ui(&mut self, ui: &mut egui::Ui) {
        let mut enter = false;
        ui.horizontal(|ui| {
            let field = ui.add(
                egui::TextEdit::singleline(&mut self.settings_query)
                    .char_limit(256)
                    .id_salt("settings-search")
                    .hint_text(t("Search settings"))
                    .desired_width((ui.available_width() - 65.0).max(120.0)),
            );
            enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if field.changed() || field.clicked() {
                self.settings_search_open = !self.settings_query.trim().is_empty();
            }
            if !self.settings_query.is_empty() && ui.button(t("Clear")).clicked() {
                self.settings_query.clear();
                self.settings_search_open = false;
                field.request_focus();
            }
        });
        if enter {
            if let Some(entry) = matches(&self.settings_query, self.page, |s| t(s).into())
                .first()
                .copied()
            {
                self.choose_setting(entry);
            }
        }
        if self.settings_search_open && !self.settings_query.trim().is_empty() {
            let found = matches(&self.settings_query, self.page, |s| t(s).into());
            if found.is_empty() {
                ui.weak(t("No matching settings"));
            } else {
                let current = self.page;
                let mut choose = None;
                egui::ScrollArea::vertical()
                    .id_salt("settings-search-results")
                    .max_height(240.0)
                    .show(ui, |ui| {
                        let mut previous = None;
                        for entry in &found {
                            if previous != Some(entry.page) {
                                ui.label(
                                    RichText::new(tr(&format!(
                                        "{} · {}",
                                        if entry.page == current {
                                            t("This panel")
                                        } else {
                                            t("Other panels")
                                        },
                                        t(panel(entry.page))
                                    )))
                                    .strong(),
                                );
                                previous = Some(entry.page);
                            }
                            let text = format!("{}   › {}", t(entry.title), t(entry.section));
                            if ui
                                .add(egui::Button::new(text).frame(false).wrap())
                                .clicked()
                            {
                                choose = Some(*entry);
                            }
                        }
                    });
                if let Some(entry) = choose {
                    self.choose_setting(entry);
                }
            }
        }
        ui.separator();
    }
    pub(super) fn settings_search_report(&self) -> serde_json::Value {
        let mut value = json_report(
            &self.settings_query,
            self.page,
            self.settings_match.as_deref(),
            self.settings_jump.is_some(),
        );
        value["modified"] = serde_json::json!(self.s != self.saved_settings);
        value
    }
}
fn json_report(
    query: &str,
    page: usize,
    section: Option<&str>,
    pending: bool,
) -> serde_json::Value {
    serde_json::json!({"query":query,"page":page,"section":section,"pending":pending,"results":matches(query,page,|s|t(s).into()).iter().map(|e|serde_json::json!({"page":e.page,"title":e.title,"section":e.section})).collect::<Vec<_>>()})
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn panel_matches_always_precede_other_panels() {
        let results = matches("speech", 2, str::to_owned);
        assert!(!results.is_empty());
        assert_eq!(results[0].page, 2);
        let split = results.iter().position(|e| e.page != 2).unwrap();
        assert!(results[..split].iter().all(|e| e.page == 2));
        assert!(results[split..].iter().all(|e| e.page != 2));
        assert!(results.iter().any(|e| e.page == 5));
    }
    #[test]
    fn localized_words_and_exact_titles_are_searchable() {
        let results = matches("lectura", 4, |s| {
            if s == "Read AI responses aloud by default" {
                "Lectura automática".into()
            } else {
                s.into()
            }
        });
        assert!(
            results
                .iter()
                .any(|e| e.title == "Read AI responses aloud by default")
        );
        let results = matches("Enable TTS streaming", 5, str::to_owned);
        assert_eq!(results[0].title, "Enable TTS streaming");
        assert_eq!(results[0].section, "Voice Model");
        assert!(matches("   ", 2, str::to_owned).is_empty());
        assert!(matches("unknownsettingzz", 2, str::to_owned).is_empty());
    }
    #[test]
    fn every_settings_panel_has_searchable_entries() {
        for page in 1..=9 {
            assert!(ENTRIES.iter().any(|e| e.page == page));
        }
    }
}
