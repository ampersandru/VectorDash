use std::{cell::Cell, collections::HashMap, sync::OnceLock};

pub const LANGUAGES: &[(&str, &str)] = &[
    ("system", "Use system language"),
    ("en", "English"),
    ("es", "Español"),
    ("pt-BR", "Português (Brasil)"),
    ("fr", "Français"),
    ("de", "Deutsch"),
    ("it", "Italiano"),
    ("ru", "Русский"),
    ("zh-CN", "简体中文"),
    ("zh-TW", "繁體中文"),
    ("ja", "日本語"),
    ("ko", "한국어"),
    ("hi", "हिन्दी"),
    ("vi", "Tiếng Việt"),
];
const CATALOGS: &[&str] = &[
    include_str!("../locales/en.json"),
    include_str!("../locales/es.json"),
    include_str!("../locales/pt-BR.json"),
    include_str!("../locales/fr.json"),
    include_str!("../locales/de.json"),
    include_str!("../locales/it.json"),
    include_str!("../locales/ru.json"),
    include_str!("../locales/zh-CN.json"),
    include_str!("../locales/zh-TW.json"),
    include_str!("../locales/ja.json"),
    include_str!("../locales/ko.json"),
    include_str!("../locales/hi.json"),
    include_str!("../locales/vi.json"),
];
static TRAY_LANGUAGE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
thread_local! { static ACTIVE: Cell<usize> = const { Cell::new(0) }; }
static MESSAGES: OnceLock<Vec<HashMap<String, String>>> = OnceLock::new();

fn catalogs() -> &'static Vec<HashMap<String, String>> {
    MESSAGES.get_or_init(|| {
        CATALOGS
            .iter()
            .map(|json| serde_json::from_str(json).expect("validated embedded locale catalog"))
            .collect()
    })
}

fn resolve(locale: &str) -> usize {
    let tag = locale.replace('_', "-").to_ascii_lowercase();
    let code = if tag == "zh" || tag.starts_with("zh-") {
        if tag.contains("hant") || tag.contains("-tw") || tag.contains("-hk") || tag.contains("-mo")
        {
            "zh-TW"
        } else {
            "zh-CN"
        }
    } else if tag == "pt" || tag.starts_with("pt-") {
        "pt-BR"
    } else {
        tag.split('-').next().unwrap_or("en")
    };
    LANGUAGES
        .iter()
        .skip(1)
        .position(|(key, _)| *key == code)
        .unwrap_or(0)
}

fn system_language() -> &'static str {
    static SYSTEM: OnceLock<String> = OnceLock::new();
    SYSTEM.get_or_init(|| {
        #[cfg(windows)]
        {
            // UI language, rather than the user's number/date regional format.
            #[link(name = "kernel32")]
            unsafe extern "system" {
                fn GetUserDefaultUILanguage() -> u16;
                fn LCIDToLocaleName(locale: u32, name: *mut u16, count: i32, flags: u32) -> i32;
            }
            let mut name = [0u16; 85];
            let count = unsafe {
                LCIDToLocaleName(
                    GetUserDefaultUILanguage() as u32,
                    name.as_mut_ptr(),
                    name.len() as i32,
                    0,
                )
            };
            if count > 1 {
                return String::from_utf16_lossy(&name[..count as usize - 1]);
            }
        }
        for key in ["LC_ALL", "LC_MESSAGES", "LANG"] {
            if let Ok(value) = std::env::var(key) {
                if !value.is_empty() {
                    return value.split('.').next().unwrap_or("en").into();
                }
            }
        }
        "en".into()
    })
}

pub fn set_language(code: &str) {
    ACTIVE.set(resolve(if code == "system" {
        system_language()
    } else {
        code
    }));
}

/// Only UI display strings pass through here; never translate persisted identifiers.
/// Untranslated messages and unsupported locales fall back to English.
pub fn t(message: &str) -> &str {
    catalogs()[ACTIVE.get()]
        .get(message)
        .map(String::as_str)
        .unwrap_or(message)
}

/// Translate formatted interface messages while keeping substituted values unchanged.
pub fn tr(message: &str) -> String {
    if ACTIVE.get() == 0 {
        return message.to_owned();
    }
    thread_local! { static CACHE: std::cell::RefCell<HashMap<(usize, String), String>> = std::cell::RefCell::new(HashMap::new()); }
    let key = (ACTIVE.get(), message.to_owned());
    if let Some(value) = CACHE.with(|cache| cache.borrow().get(&key).cloned()) {
        return value;
    }
    let result = translate_formatted(message);
    CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.len() >= 128 {
            cache.clear();
        }
        cache.insert(key, result.clone());
    });
    result
}
fn translate_formatted(message: &str) -> String {
    if let Some(value) = catalogs()[ACTIVE.get()].get(message) {
        return value.clone();
    }
    static PATTERNS: OnceLock<Vec<(String, regex::Regex, Vec<String>)>> = OnceLock::new();
    let patterns = PATTERNS.get_or_init(|| {
        let slot = regex::Regex::new(r"\{[^{}]*\}").unwrap();
        catalogs()[0]
            .keys()
            .filter_map(|key| {
                let slots: Vec<_> = slot.find_iter(key).collect();
                if slots.is_empty() || slot.replace_all(key, "").len() < 5 {
                    return None;
                }
                let mut pattern = String::from("^");
                let mut last = 0;
                let mut names = Vec::new();
                for part in slots {
                    pattern.push_str(&regex::escape(&key[last..part.start()]));
                    pattern.push_str("(.*?)");
                    names.push(part.as_str().to_owned());
                    last = part.end();
                }
                pattern.push_str(&regex::escape(&key[last..]));
                pattern.push('$');
                Some((key.clone(), regex::Regex::new(&pattern).unwrap(), names))
            })
            .collect()
    });
    for (key, pattern, slots) in patterns {
        if let Some(found) = pattern.captures(message) {
            if let Some(translated) = catalogs()[ACTIVE.get()].get(key) {
                let mut output = translated.clone();
                for (i, slot) in slots.iter().enumerate() {
                    output = output.replacen(slot, t(found.get(i + 1).unwrap().as_str()), 1);
                }
                return output;
            }
        }
    }
    if message.contains(" · ") {
        return message.split(" · ").map(tr).collect::<Vec<_>>().join(" · ");
    }
    message.to_owned()
}

/// Native tray menus run on the Windows message thread, not the egui thread.
pub fn tray_text(message: &str) -> &str {
    catalogs()[TRAY_LANGUAGE.load(std::sync::atomic::Ordering::Relaxed)]
        .get(message)
        .map(String::as_str)
        .unwrap_or(message)
}

pub fn language_name(code: &str) -> &'static str {
    if code == "system" {
        return t("Use system language");
    }
    LANGUAGES[resolve(code) + 1].1
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn catalogs_have_complete_core_keys_and_nonempty_values() {
        let english = &catalogs()[0];
        assert_eq!(CATALOGS.len(), LANGUAGES.len() - 1);
        for catalog in catalogs() {
            assert_eq!(catalog.len(), english.len());
            for key in english.keys() {
                assert!(!catalog[key].trim().is_empty(), "missing {key}");
            }
        }
    }
    #[test]
    fn all_catalogs_preserve_formatted_values_and_have_no_translation_tokens() {
        let slot = regex::Regex::new(r"\{[^{}]*\}").unwrap();
        for catalog in catalogs() {
            for (key, value) in catalog {
                let mut expected = slot.find_iter(key).map(|m| m.as_str()).collect::<Vec<_>>();
                let mut actual = slot
                    .find_iter(value)
                    .map(|m| m.as_str())
                    .collect::<Vec<_>>();
                expected.sort();
                actual.sort();
                assert_eq!(expected, actual, "{key}: {value}");
                assert!(!value.contains("ZXQ"), "{key}");
            }
        }
    }
    #[test]
    fn menus_and_settings_have_no_unmarked_literal_labels() {
        let source = include_str!("app.rs").split("#[cfg(test)]").next().unwrap();
        let raw=regex::Regex::new(r#"\.(?:label|weak|strong|heading|button|small_button|checkbox|collapsing|text|hint_text|on_hover_text|selected_text)\(\s*"([^"]+)""#).unwrap();
        for found in raw.captures_iter(source) {
            assert!(
                !found[1].chars().any(|c| c.is_ascii_alphabetic()),
                "Unmarked UI label: {}",
                &found[1]
            );
        }
        let marked = regex::Regex::new(r#"\bt\(\s*("(?:\\.|[^"\\])*")"#).unwrap();
        for found in marked.captures_iter(source) {
            let key: String = serde_json::from_str(&found[1]).unwrap();
            assert!(
                catalogs()[0].contains_key(&key),
                "Missing catalog message {key}"
            );
        }
    }
    #[test]
    fn formatted_translation_keeps_model_ids_and_paths_and_switches_language() {
        set_language("es");
        let message = "Installed: model-ID_123";
        assert_eq!(
            tr(message),
            t("Installed: {}").replace("{}", "model-ID_123")
        );
        let spanish = tr(message);
        set_language("ja");
        assert_ne!(tr(message), spanish);
        assert_eq!(tr(r"C:\Users\name\Photo.ARW"), r"C:\Users\name\Photo.ARW");
        set_language("en");
        assert_eq!(tr(message), message);
    }
    #[test]
    fn regional_locales_and_unknown_languages_resolve_safely() {
        for (tag, code) in [
            ("es-MX", "es"),
            ("pt_PT", "pt-BR"),
            ("zh-Hant-HK", "zh-TW"),
            ("zh-SG", "zh-CN"),
            ("ko-KR", "ko"),
            ("ar-SA", "en"),
        ] {
            assert_eq!(LANGUAGES[resolve(tag) + 1].0, code);
        }
    }
    #[test]
    fn language_preview_switch_and_english_fallback() {
        set_language("vi");
        assert_eq!(t("Search"), "Tìm kiếm");
        assert_eq!(
            t("Untranslated technical detail"),
            "Untranslated technical detail"
        );
        set_language("ja");
        assert_eq!(t("Search"), "検索");
        set_language("unknown");
        assert_eq!(t("Search"), "Search");
    }
    #[test]
    fn settings_preserve_language_and_canonical_search_values() {
        let old: crate::config::Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(old.language, "system");
        let signature = crate::engine::source_signature(&old);
        let mut settings = old;
        settings.language = "de".into();
        let restored: crate::config::Settings =
            serde_json::from_str(&serde_json::to_string(&settings).unwrap()).unwrap();
        assert_eq!(restored.language, "de");
        assert_eq!(crate::engine::source_signature(&restored), signature);
        assert_eq!(restored.default_search_mode, "Both");
        assert_eq!(restored.search_tools()[0], "Search");
    }
}

/// Font files are read once per actual locale change, never on each repaint.
pub fn apply(ctx: &eframe::egui::Context, code: &str) {
    use eframe::egui;
    set_language(code);
    TRAY_LANGUAGE.store(ACTIVE.get(), std::sync::atomic::Ordering::Relaxed);
    let key = egui::Id::new("interface-font-language");
    let changed = ctx.data_mut(|d| {
        let changed = d.get_temp::<usize>(key) != Some(ACTIVE.get());
        if changed {
            d.insert_temp(key, ACTIVE.get());
        }
        changed
    });
    if !changed {
        return;
    }
    let mut fonts = egui::FontDefinitions::default();
    let system = std::path::PathBuf::from(
        std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into()),
    )
    .join("Fonts");
    let mut names = vec![
        "segoeui.ttf",
        "seguiemj.ttf",
        "Nirmala.ttc",
        "Nirmala.ttf",
        "msyh.ttc",
        "malgun.ttf",
    ];
    // Prefer regional CJK glyph shapes only when that interface language is active.
    if ACTIVE.get() == resolve("ja") {
        names.insert(2, "YuGothR.ttc");
    }
    if ACTIVE.get() == resolve("zh-TW") {
        names.insert(2, "msjh.ttc");
    }
    for name in names {
        if let Ok(bytes) = std::fs::read(system.join(name)) {
            fonts
                .font_data
                .insert(name.into(), egui::FontData::from_owned(bytes).into());
            for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                fonts.families.entry(family).or_default().push(name.into());
            }
        }
    }
    if let Ok(bytes) = std::fs::read(system.join("segoeuib.ttf")) {
        fonts
            .font_data
            .insert("bold".into(), egui::FontData::from_owned(bytes).into());
        fonts
            .families
            .insert(egui::FontFamily::Name("bold".into()), vec!["bold".into()]);
    }
    if fonts.font_data.contains_key("segoeui.ttf") {
        if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
            family.retain(|name| name != "segoeui.ttf");
            family.insert(0, "segoeui.ttf".into());
        }
    }
    let fallback = fonts.families[&egui::FontFamily::Proportional].clone();
    fonts
        .families
        .entry(egui::FontFamily::Name("bold".into()))
        .or_default()
        .extend(fallback);
    ctx.set_fonts(fonts);
}

#[cfg(all(test, windows))]
mod font_tests {
    use super::*;
    #[test]
    fn installed_windows_fonts_cover_all_core_translations() {
        use eframe::egui;
        let system = std::path::PathBuf::from(
            std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into()),
        )
        .join("Fonts");
        if ["Nirmala.ttc", "msyh.ttc", "malgun.ttf"]
            .iter()
            .any(|name| !system.join(name).is_file())
        {
            eprintln!(
                "Optional Windows language fonts are missing; full glyph coverage must be checked on a machine with those font features installed"
            );
            return;
        }
        let ctx = egui::Context::default();
        apply(&ctx, "en");
        let _ = ctx.run(Default::default(), |ctx| {
            ctx.fonts_mut(|fonts| {
                let id = egui::FontId::proportional(14.0);
                for (code, catalog) in LANGUAGES.iter().skip(1).zip(catalogs()) {
                    for text in catalog.values() {
                        for c in text.chars().filter(|c| !c.is_whitespace()) {
                            assert!(fonts.has_glyph(&id, c), "missing glyph {c} in {}", code.0);
                        }
                    }
                }
            });
        });
    }
}
