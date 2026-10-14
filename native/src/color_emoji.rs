//! Offline artwork; no emoji text or AI response is sent to an image service.
use eframe::egui;
use std::{
    borrow::Cow,
    collections::HashSet,
    io::{Cursor, Read},
    sync::{Mutex, OnceLock},
};
use unicode_segmentation::UnicodeSegmentation;
struct Assets {
    zip: zip::ZipArchive<Cursor<&'static [u8]>>,
    names: HashSet<String>,
}
fn assets() -> &'static Mutex<Assets> {
    static ASSETS: OnceLock<Mutex<Assets>> = OnceLock::new();
    ASSETS.get_or_init(|| {
        let zip = zip::ZipArchive::new(Cursor::new(
            include_bytes!("../assets/twemoji.zip").as_slice(),
        ))
        .expect("bundled emoji archive");
        let names = zip.file_names().map(str::to_owned).collect();
        Mutex::new(Assets { zip, names })
    })
}
fn key(emoji: &str, names: &HashSet<String>) -> Option<String> {
    if emoji.contains('\u{fe0e}') {
        return None;
    }
    let exact = emoji
        .chars()
        .map(|c| format!("{:x}", c as u32))
        .collect::<Vec<_>>()
        .join("-")
        + ".svg";
    if names.contains(&exact) {
        return Some(exact);
    }
    let simple = emoji
        .chars()
        .filter(|c| *c != '\u{fe0f}')
        .map(|c| format!("{:x}", c as u32))
        .collect::<Vec<_>>()
        .join("-")
        + ".svg";
    names.contains(&simple).then_some(simple)
}
#[derive(Default)]
pub struct Cache {
    loaded: HashSet<String>,
}
impl Cache {
    pub fn uri(&mut self, ctx: &egui::Context, emoji: &str) -> Option<String> {
        let mut assets = assets().lock().ok()?;
        let name = key(emoji, &assets.names)?;
        let uri = format!("bytes://vectordash-emoji/{name}");
        if !self.loaded.contains(&uri) {
            let mut bytes = Vec::new();
            assets
                .zip
                .by_name(&name)
                .ok()?
                .take(128 * 1024)
                .read_to_end(&mut bytes)
                .ok()?;
            ctx.include_bytes(uri.clone(), bytes);
            self.loaded.insert(uri.clone());
        }
        Some(uri)
    }
    pub fn clear(&mut self, ctx: &egui::Context) {
        for uri in self.loaded.drain() {
            ctx.forget_image(&uri);
        }
    }
    /// Replace only visible Markdown text. Code, HTML and existing image URLs/alt text stay intact.
    pub fn markdown<'a>(&mut self, ctx: &egui::Context, text: &'a str) -> Cow<'a, str> {
        use pulldown_cmark::{Event, Parser, Tag, TagEnd};
        let mut protected = 0usize;
        let mut replacements = Vec::new();
        for (event, span) in
            Parser::new_ext(text, pulldown_cmark::Options::all()).into_offset_iter()
        {
            match event {
                Event::Start(Tag::CodeBlock(_) | Tag::Image { .. }) => protected += 1,
                Event::End(TagEnd::CodeBlock | TagEnd::Image) => {
                    protected = protected.saturating_sub(1)
                }
                Event::Text(_) if protected == 0 => {
                    for (offset, emoji) in text[span.clone()].grapheme_indices(true) {
                        // Most prose is ASCII: skip it without opening the archive.
                        if emoji.is_ascii() {
                            continue;
                        }
                        if let Some(uri) = self.uri(ctx, emoji) {
                            replacements.push((
                                span.start + offset,
                                span.start + offset + emoji.len(),
                                format!("![{emoji}]({uri})"),
                            ));
                        }
                    }
                }
                _ => {}
            }
        }
        if replacements.is_empty() {
            return Cow::Borrowed(text);
        }
        let mut result = String::with_capacity(text.len());
        let mut cursor = 0;
        for (start, end, image) in replacements {
            if start < cursor {
                continue;
            }
            result.push_str(&text[cursor..start]);
            result.push_str(&image);
            cursor = end;
        }
        result.push_str(&text[cursor..]);
        Cow::Owned(result)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selector_has_color_artwork_for_every_bundled_emoji() {
        let values: Vec<(String, String)> =
            serde_json::from_str(include_str!("../assets/emoji.json")).unwrap();
        let assets = assets().lock().unwrap();
        let missing = values
            .iter()
            .filter(|(emoji, _)| key(emoji, &assets.names).is_none())
            .collect::<Vec<_>>();
        assert!(missing.is_empty(), "Missing: {missing:?}");
    }
    #[test]
    fn markdown_preserves_code_links_and_unicode_copy_source() {
        let ctx = egui::Context::default();
        let mut cache = Cache::default();
        let text = "**Hello 😀** 👩🏽‍💻 🇺🇸 ❤️ 1️⃣\n`😀`\n```rust\nlet s = \"😀\";\n```\n![😀](https://example.org/😀.png)\n[site](https://example.org/😀)";
        let colored = cache.markdown(&ctx, text);
        assert!(colored.contains("**Hello ![😀](bytes://"));
        assert!(colored.contains("![👩🏽‍💻](bytes://"), "{colored}");
        assert!(colored.contains("![🇺🇸](bytes://"));
        assert!(colored.contains("`😀`"));
        assert!(colored.contains("let s = \"😀\";"));
        assert!(colored.contains("![😀](https://example.org/😀.png)"));
        assert!(colored.contains("[site](https://example.org/😀)"));
        assert_eq!(text.matches('😀').count(), 6);
        assert!(matches!(
            cache.markdown(&ctx, "plain prose"),
            Cow::Borrowed(_)
        ));
        cache.clear(&ctx);
        assert!(cache.loaded.is_empty());
    }
}
