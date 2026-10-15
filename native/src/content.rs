use crate::config::Settings;
use anyhow::{Context, Result};
use image::{DynamicImage, GenericImageView};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Cursor, Read},
    path::Path,
    process::Command,
};
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Segment {
    pub kind: String,
    pub text: String,
    pub page: u32,
    pub seconds: f32,
    pub line: u32,
    #[serde(with = "media_base64")]
    pub media: Vec<u8>,
}
impl Segment {
    pub fn text(kind: &str, text: String, page: u32, line: u32) -> Self {
        Self {
            kind: kind.into(),
            text,
            page,
            seconds: -1.0,
            line,
            media: vec![],
        }
    }
}
pub fn is_image(e: &str) -> bool {
    [
        "jpg", "jpeg", "png", "webp", "avif", "jxl", "jpegxl", "bmp", "tif", "tiff", "gif",
    ]
    .contains(&e)
}
pub fn is_audio(e: &str) -> bool {
    ["mp3", "wav", "flac", "m4a", "aac", "ogg", "opus", "wma"].contains(&e)
}
pub fn is_video(e: &str) -> bool {
    [
        "mp4", "mkv", "mov", "avi", "webm", "m4v", "wmv", "mpeg", "mpg",
    ]
    .contains(&e)
}
pub fn is_code(e: &str) -> bool {
    [
        "rs", "py", "js", "jsx", "ts", "tsx", "java", "kt", "go", "c", "h", "cpp", "hpp", "cs",
        "swift", "rb", "php", "sh", "ps1", "sql", "ipynb",
    ]
    .contains(&e)
}
pub fn supports_content(ext: &str, s: &Settings) -> bool {
    is_image(ext)
        || is_audio(ext)
        || is_video(ext)
        || [
            "pdf", "docx", "xlsx", "pptx", "zip", "txt", "md", "csv", "log", "html", "htm", "rtf",
            "json", "yaml", "yml", "toml",
        ]
        .contains(&ext)
        || s.code_search && is_code(ext)
}
pub fn chunks(text: &str, size: usize) -> Vec<(String, u32)> {
    let mut out = vec![];
    let chars: Vec<char> = text.chars().collect();
    let stride = size.saturating_sub(180).max(1);
    let mut previous = 0;
    let mut line = 1u32;
    for offset in (0..chars.len()).step_by(stride) {
        line += chars[previous..offset]
            .iter()
            .filter(|&&c| c == '\n')
            .count() as u32;
        previous = offset;
        let end = (offset + size).min(chars.len());
        out.push((chars[offset..end].iter().collect(), line));
        if end == chars.len() {
            break;
        }
    }
    out
}
pub fn ffmpeg(s: &Settings) -> Result<String> {
    if !s.ffmpeg.is_empty() {
        return Ok(s.ffmpeg.clone());
    }
    let bundled = std::env::current_exe()?.with_file_name("ffmpeg.exe");
    if bundled.exists() {
        return Ok(bundled.to_string_lossy().into());
    }
    Ok("ffmpeg.exe".into())
}
pub fn hidden(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
}
pub fn decode(path: &Path, s: &Settings) -> Result<DynamicImage> {
    let ext = path
        .extension()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();
    let image = if ["jxl", "jpegxl"].contains(&ext.as_str()) {
        let decoder = jxl_oxide::integration::JxlDecoder::new(fs::File::open(path)?)?;
        let (width, height) = image::ImageDecoder::dimensions(&decoder);
        anyhow::ensure!(
            width as u64 * height as u64 <= 50_000_000,
            "JPEG XL exceeds safe pixel limit"
        );
        DynamicImage::from_decoder(decoder)?
    } else if ext == "avif" {
        let mut c = Command::new(ffmpeg(s)?);
        c.args(["-nostdin", "-v", "error", "-i"]).arg(path).args([
            "-frames:v",
            "1",
            "-vf",
            "scale='min(2048,iw)':-2",
            "-f",
            "image2pipe",
            "-vcodec",
            "mjpeg",
            "-",
        ]);
        hidden(&mut c);
        let out = c.output()?;
        anyhow::ensure!(
            out.status.success(),
            "AVIF decoder failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        image::load_from_memory(&out.stdout)?
    } else {
        let mut reader = image::ImageReader::open(path)?.with_guessed_format()?;
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(20000);
        limits.max_image_height = Some(20000);
        limits.max_alloc = Some(256 * 1024 * 1024);
        reader.limits(limits);
        reader.decode()?
    };
    let (w, h) = image.dimensions();
    anyhow::ensure!(
        w as u64 * h as u64 <= 50_000_000,
        "Image exceeds safe pixel limit"
    );
    Ok(image)
}
pub fn jpeg(image: &DynamicImage, max: u32) -> Result<Vec<u8>> {
    let resized = image.thumbnail(max, max).to_rgb8();
    let mut out = vec![];
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 90).encode_image(&resized)?;
    Ok(out)
}
fn xml_text(data: &str) -> String {
    let mut reader = quick_xml::Reader::from_str(data);
    reader.config_mut().trim_text(true);
    let mut out = String::new();
    while let Ok(event) = reader.read_event() {
        match event {
            quick_xml::events::Event::Text(t) => {
                if let Ok(v) = t.xml_content() {
                    out.push_str(&v);
                    out.push(' ');
                }
            }
            quick_xml::events::Event::Eof => break,
            _ => {}
        }
    }
    out
}
pub fn visible_html(source: &str) -> String {
    let doc = scraper::Html::parse_document(source);
    let mut text = String::new();
    for node in doc.tree.root().descendants() {
        if let scraper::Node::Text(value) = node.value() {
            let hidden = node.ancestors().any(|ancestor| {
                if let scraper::Node::Element(element) = ancestor.value() {
                    ["script", "style", "template", "noscript"].contains(&element.name())
                        || element.attr("hidden").is_some()
                        || element.attr("aria-hidden") == Some("true")
                } else {
                    false
                }
            });
            if !hidden {
                let value = value.split_whitespace().collect::<Vec<_>>().join(" ");
                if !value.is_empty() {
                    text.push_str(&value);
                    text.push('\n');
                }
            }
        }
    }
    text
}
fn archive(path: &Path, ext: &str) -> Result<Vec<Segment>> {
    let mut z = zip::ZipArchive::new(fs::File::open(path)?)?;
    anyhow::ensure!(z.len() <= 100_000, "Archive has too many entries");
    let mut records = vec![];
    let mut names = vec![];
    let mut expanded = 0u64;
    for i in 0..z.len() {
        let mut f = z.by_index(i)?;
        anyhow::ensure!(f.size() <= 32 * 1024 * 1024, "Archive entry is too large");
        let n = f.name().to_string();
        names.push(n.clone());
        let read = match ext {
            "docx" => n == "word/document.xml",
            "xlsx" => n == "xl/sharedStrings.xml" || n.starts_with("xl/worksheets/sheet"),
            "pptx" => n.starts_with("ppt/slides/slide") && n.ends_with(".xml"),
            _ => false,
        };
        if read {
            expanded += f.size();
            anyhow::ensure!(expanded <= 128 * 1024 * 1024, "Archive expansion limit");
            let mut text = String::new();
            f.read_to_string(&mut text)?;
            anyhow::ensure!(
                !text.contains("<!DOCTYPE") && !text.contains("<!ENTITY"),
                "XML entities are not allowed"
            );
            let text = xml_text(&text);
            for (t, line) in chunks(&text, 1800) {
                records.push(Segment::text(
                    if ext == "pptx" {
                        "slides"
                    } else if ext == "xlsx" {
                        "spreadsheet"
                    } else {
                        "document"
                    },
                    t,
                    i as u32 + 1,
                    line,
                ));
            }
        }
    }
    if ext == "zip" {
        for (t, line) in chunks(&names.join("\n"), 1800) {
            records.push(Segment::text("archive", t, 0, line));
        }
    }
    Ok(records)
}
pub fn extract(
    path: &Path,
    s: &Settings,
    checkpoint: &dyn Fn() -> Result<()>,
) -> Result<Vec<Segment>> {
    let size = fs::metadata(path)?.len();
    anyhow::ensure!(
        size <= s.max_file_mib * 1024 * 1024
            || is_audio(&extension(path))
            || is_video(&extension(path)),
        "File exceeds configured parsing size limit"
    );
    let ext = extension(path);
    if is_image(&ext) {
        if !s.index_images && (!s.ocr_enabled || s.ocr.is_empty()) {
            return Ok(vec![]);
        }
        let image = decode(path, s)?;
        let mut records = vec![];
        if s.index_images {
            records.push(Segment {
                kind: "image".into(),
                text: String::new(),
                page: 0,
                seconds: -1.0,
                line: 0,
                media: jpeg(
                    &image,
                    if s.visual_detail == "Detailed" {
                        1536
                    } else {
                        1024
                    },
                )?,
            });
        }
        if s.ocr_enabled && !s.ocr.is_empty() {
            match ocr_bytes(&jpeg(&image, 2048)?, s) {
                Ok(text) => {
                    for (t, line) in chunks(&text, 1800) {
                        records.push(Segment::text("image", t, 0, line));
                    }
                }
                Err(error) => records.push(Segment::text(
                    "warning",
                    format!("OCR unavailable: {error}"),
                    0,
                    0,
                )),
            }
        }
        return Ok(records);
    }
    if ext == "pdf" {
        let pages = crate::pdf::text(path).or_else(|_| {
            let doc = lopdf::Document::load(path)?;
            Ok::<_, anyhow::Error>(
                doc.get_pages()
                    .keys()
                    .map(|page| doc.extract_text(&[*page]).unwrap_or_default())
                    .collect(),
            )
        })?;
        let mut out = vec![];
        for (i, text) in pages.into_iter().enumerate() {
            checkpoint()?;
            if text.trim().is_empty() && s.ocr_enabled && !s.ocr.is_empty() {
                let media = crate::pdf::preview(path, i as u32 + 1, "")?;
                let text = match ocr_bytes(&media, s) {
                    Ok(text) => text,
                    Err(error) => {
                        out.push(Segment::text(
                            "warning",
                            format!("PDF OCR unavailable: {error}"),
                            i as u32 + 1,
                            0,
                        ));
                        String::new()
                    }
                };
                for (t, line) in chunks(&text, 1800) {
                    out.push(Segment::text("pdf", t, i as u32 + 1, line));
                }
            }
            for (t, line) in chunks(&text, 1800) {
                out.push(Segment::text("pdf", t, i as u32 + 1, line));
            }
            if s.pdf_visuals {
                let media = crate::pdf::preview(path, i as u32 + 1, "")?;
                out.push(Segment {
                    kind: "pdf_image".into(),
                    text: String::new(),
                    page: i as u32 + 1,
                    seconds: -1.0,
                    line: 0,
                    media,
                });
            }
        }
        return Ok(out);
    }
    if ["docx", "xlsx", "pptx", "zip"].contains(&ext.as_str()) {
        return archive(path, &ext);
    }
    if is_audio(&ext) || is_video(&ext) {
        return media_segments(path, s, checkpoint);
    }
    if [
        "txt", "md", "csv", "log", "html", "htm", "rtf", "json", "yaml", "yml", "toml",
    ]
    .contains(&ext.as_str())
        || s.code_search && is_code(&ext)
    {
        let bytes = fs::read(path)?;
        let text = if bytes.starts_with(&[0xff, 0xfe]) {
            let u: Vec<u16> = bytes[2..]
                .chunks_exact(2)
                .map(|b| u16::from_le_bytes([b[0], b[1]]))
                .collect();
            String::from_utf16_lossy(&u)
        } else {
            String::from_utf8_lossy(&bytes).into_owned()
        };
        let text = if ["html", "htm"].contains(&ext.as_str()) {
            visible_html(&text)
        } else {
            text
        };
        return Ok(chunks(&text, 1800)
            .into_iter()
            .map(|(t, line)| Segment::text(if is_code(&ext) { "code" } else { "text" }, t, 0, line))
            .collect());
    }
    Ok(vec![])
}
pub fn extension(path: &Path) -> String {
    path.extension()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase()
}
fn media_segments(
    path: &Path,
    s: &Settings,
    checkpoint: &dyn Fn() -> Result<()>,
) -> Result<Vec<Segment>> {
    let mut records = vec![];
    stream(path, s, checkpoint, &mut |segment| {
        anyhow::ensure!(
            records.len() < 2000,
            "Media contains too many segments for one file"
        );
        records.push(segment);
        Ok(())
    })?;
    Ok(records)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unicode_chunks_preserve_text() {
        let input = "বাংলা العربية\n".repeat(1000);
        let parts = chunks(&input, 1800);
        assert!(parts.iter().all(|(p, _)| p.chars().count() <= 1800));
        assert!(parts.last().unwrap().0.ends_with("العربية\n"));
    }
    #[test]
    fn formats() {
        for e in ["webp", "avif", "jxl", "jpegxl"] {
            assert!(is_image(e));
        }
        assert!(!is_image("exe"));
    }
}

pub fn ocr_bytes(bytes: &[u8], s: &Settings) -> Result<String> {
    // Leptonica's Windows JPEG-memory reader uses a medium-integrity temp file.
    // PNG is decoded directly from memory, so low-integrity workers need no writable escape.
    let input_bytes = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        std::borrow::Cow::Borrowed(bytes)
    } else {
        use image::ImageEncoder;
        let image = image::load_from_memory(bytes)?.to_luma8();
        let mut png = vec![];
        image::codecs::png::PngEncoder::new_with_quality(
            &mut png,
            image::codecs::png::CompressionType::Fast,
            image::codecs::png::FilterType::Adaptive,
        )
        .write_image(
            image.as_raw(),
            image.width(),
            image.height(),
            image::ExtendedColorType::L8,
        )?;
        std::borrow::Cow::Owned(png)
    };
    use std::io::Write;
    use std::process::Stdio;
    let mut command = Command::new(&s.ocr);
    command
        .args(["stdin", "stdout", "-l", &s.ocr_languages])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    hidden(&mut command);
    let mut child = command.spawn().context("Cannot start Tesseract OCR")?;
    if let Some(mut input) = child.stdin.take() {
        input.write_all(&input_bytes)?;
    }
    let output = child.wait_with_output()?;
    anyhow::ensure!(
        output.status.success(),
        "OCR process failed (exit {:?}): {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
            .chars()
            .take(700)
            .collect::<String>()
    );
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}
pub fn video_frame(path: &Path, seconds: f32, s: &Settings) -> Result<Vec<u8>> {
    if s.video_decoder != "FFmpeg" {
        let result = crate::native_video::open(path, s)
            .and_then(|reader| reader.frame(seconds, &|| Ok(())))
            .and_then(|frame| {
                frame
                    .map(|(_, bytes)| bytes)
                    .context("No video frame at this time")
            });
        match result {
            Ok(bytes) => return Ok(bytes),
            Err(error) if !s.video_ffmpeg_fallback => {
                return Err(error.context(
                    "Windows video decoding failed; enable FFmpeg fallback or choose FFmpeg",
                ));
            }
            Err(_) => {}
        }
    }
    let mut c = Command::new(ffmpeg(s)?);
    crate::native_video::ffmpeg_device(&mut c, s);
    c.args([
        "-nostdin",
        "-v",
        "error",
        "-ss",
        &seconds.max(0.0).to_string(),
        "-i",
    ])
    .arg(path)
    .args([
        "-frames:v",
        "1",
        "-vf",
        "scale='min(1024,iw)':-2",
        "-f",
        "image2pipe",
        "-vcodec",
        "mjpeg",
        "-threads",
        "2",
        "-",
    ]);
    hidden(&mut c);
    let out = c.output()?;
    anyhow::ensure!(out.status.success(), "Video preview failed");
    Ok(out.stdout)
}
#[derive(serde::Serialize, serde::Deserialize)]
pub struct PreviewRequest {
    pub path: String,
    pub seconds: f32,
    pub settings: Settings,
}
pub fn stream(
    path: &Path,
    s: &Settings,
    checkpoint: &dyn Fn() -> Result<()>,
    emit: &mut dyn FnMut(Segment) -> Result<()>,
) -> Result<()> {
    let ext = extension(path);
    if !is_audio(&ext) && !is_video(&ext) {
        for segment in extract(path, s, checkpoint)? {
            checkpoint()?;
            emit(segment)?;
        }
        return Ok(());
    }
    use std::process::Stdio;
    let mut native_frames = false;
    let mut native_audio = None;
    if is_video(&ext) && s.index_video && s.video_decoder != "FFmpeg" {
        let mut emitted = 0;
        let result = (|| -> Result<()> {
            let reader = crate::native_video::open(path, s)?;
            native_audio = Some(reader.has_audio());
            let interval = s.video_interval.clamp(1.0, 60.0);
            for frame in 0..100000u32 {
                checkpoint()?;
                let seconds = frame as f32 * interval;
                if reader.duration().is_some_and(|d| seconds >= d) {
                    break;
                }
                let Some((seconds, media)) = reader.frame(seconds, checkpoint)? else {
                    break;
                };
                emit(Segment {
                    kind: "video".into(),
                    text: String::new(),
                    page: 0,
                    seconds,
                    line: 0,
                    media,
                })?;
                emitted += 1;
            }
            Ok(())
        })();
        match result {
            Ok(()) => native_frames = true,
            Err(error) if emitted == 0 && s.video_ffmpeg_fallback => {
                checkpoint()?;
                emit(Segment::text(
                    "warning",
                    format!("Windows video decoder unavailable; using FFmpeg fallback: {error}"),
                    0,
                    0,
                ))?;
            }
            Err(error) => {
                return Err(error.context(
                    "Windows video decoding failed; choose FFmpeg for unsupported codecs",
                ));
            }
        }
    }
    if is_video(&ext) && s.index_video && !native_frames {
        let mut c = Command::new(ffmpeg(s)?);
        crate::native_video::ffmpeg_device(&mut c, s);
        c.args(["-nostdin", "-v", "error", "-i"])
            .arg(path)
            .args([
                "-an",
                "-vf",
                &format!(
                    "fps=1/{},scale='min(1024,iw)':-2",
                    s.video_interval.clamp(1.0, 60.0)
                ),
                "-f",
                "image2pipe",
                "-vcodec",
                "mjpeg",
                "-threads",
                "2",
                "-",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        hidden(&mut c);
        let mut child = c.spawn()?;
        let mut output = child.stdout.take().context("Video stream unavailable")?;
        let mut pending = vec![];
        let mut buffer = [0u8; 65536];
        let mut frame = 0u32;
        loop {
            checkpoint()?;
            let n = output.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            pending.extend_from_slice(&buffer[..n]);
            anyhow::ensure!(pending.len() < 8 * 1024 * 1024, "Video frame exceeds limit");
            while let Some(end) = pending.windows(2).position(|b| b == [0xff, 0xd9]) {
                let rest = pending.split_off(end + 2);
                let media = std::mem::replace(&mut pending, rest);
                emit(Segment {
                    kind: "video".into(),
                    text: String::new(),
                    page: 0,
                    seconds: frame as f32 * s.video_interval.clamp(1.0, 60.0),
                    line: 0,
                    media,
                })?;
                frame += 1;
                anyhow::ensure!(frame <= 100000, "Video sampling limit reached");
            }
        }
        let status = child.wait()?;
        anyhow::ensure!(status.success(), "Video decoder failed");
    }
    // Silent videos are valid: do not discard their frames because no audio exists.
    let mut has_audio = native_audio.unwrap_or(true);
    if is_video(&ext) && s.index_audio && native_audio.is_none() {
        let decoder = ffmpeg(s)?;
        let probe = Path::new(&decoder).with_file_name("ffprobe.exe");
        if probe.exists() {
            let mut c = Command::new(probe);
            c.args([
                "-v",
                "error",
                "-select_streams",
                "a",
                "-show_entries",
                "stream=index",
                "-of",
                "csv=p=0",
            ])
            .arg(path);
            hidden(&mut c);
            let output = c.output()?;
            anyhow::ensure!(output.status.success(), "Media stream inspection failed");
            has_audio = !output.stdout.is_empty();
        }
    }
    if s.index_audio && has_audio {
        let mut c = Command::new(ffmpeg(s)?);
        c.args(["-nostdin", "-v", "error", "-i"])
            .arg(path)
            .args(["-vn", "-ac", "1", "-ar", "16000", "-f", "s16le", "-"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        hidden(&mut c);
        let mut child = c.spawn()?;
        let mut output = child.stdout.take().context("Audio stream unavailable")?;
        let mut block = vec![0u8; 30 * 16000 * 2];
        let mut chunk = 0;
        loop {
            checkpoint()?;
            let mut count = 0;
            while count < block.len() {
                let n = output.read(&mut block[count..])?;
                if n == 0 {
                    break;
                }
                count += n;
            }
            if count == 0 {
                break;
            }
            let mut media = Vec::with_capacity(44 + count);
            media.extend(b"RIFF");
            media.extend((count as u32 + 36).to_le_bytes());
            media.extend(b"WAVEfmt ");
            media.extend(16u32.to_le_bytes());
            media.extend(1u16.to_le_bytes());
            media.extend(1u16.to_le_bytes());
            media.extend(16000u32.to_le_bytes());
            media.extend(32000u32.to_le_bytes());
            media.extend(2u16.to_le_bytes());
            media.extend(16u16.to_le_bytes());
            media.extend(b"data");
            media.extend((count as u32).to_le_bytes());
            media.extend_from_slice(&block[..count]);
            emit(Segment {
                kind: if is_video(&ext) {
                    "video_audio"
                } else {
                    "audio"
                }
                .into(),
                text: String::new(),
                page: 0,
                seconds: chunk as f32 * 30.0,
                line: 0,
                media,
            })?;
            chunk += 1;
        }
        let status = child.wait()?;
        anyhow::ensure!(status.success(), "Audio decoder failed");
    }
    Ok(())
}
mod media_base64 {
    use base64::{Engine, engine::general_purpose::STANDARD};
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    pub fn serialize<S: Serializer>(data: &Vec<u8>, s: S) -> Result<S::Ok, S::Error> {
        STANDARD.encode(data).serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let data = String::deserialize(d)?;
        STANDARD.decode(data).map_err(serde::de::Error::custom)
    }
}
#[cfg(test)]
mod streaming_tests {
    use super::*;
    #[test]
    fn media_uses_bounded_base64_wire_format() {
        let mut segment = Segment::text("image", String::new(), 0, 0);
        segment.media = (0..255).collect();
        let value = serde_json::to_value(&segment).unwrap();
        assert!(value["media"].is_string());
        let decoded: Segment = serde_json::from_value(value).unwrap();
        assert_eq!(decoded.media, segment.media);
    }
}

#[cfg(test)]
mod html_tests {
    #[test]
    fn report_scripts_do_not_become_embeddings() {
        let source = format!(
            "<title>Benchmark</title><script>{}</script><style>.a{{color:red}}</style><h1>Frame timing</h1><p>Invoice INV-123 &amp; paid</p><div hidden>hidden secret</div>",
            "x".repeat(2_000_000)
        );
        let text = super::visible_html(&source);
        assert!(text.contains("Benchmark") && text.contains("INV-123 & paid"));
        assert!(!text.contains("hidden secret") && !text.contains("color:red"));
        assert_eq!(super::chunks(&text, 1800).len(), 1);
    }
}
