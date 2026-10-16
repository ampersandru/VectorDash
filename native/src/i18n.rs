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
