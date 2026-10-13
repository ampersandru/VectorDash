use anyhow::{Context, Result};
use base64::{Engine as _, engine::general_purpose::STANDARD};
#[derive(Clone, Debug)]
pub struct Target {
    pub id: u32,
    pub window: bool,
    pub label: String,
}
pub fn targets() -> Result<Vec<Target>> {
    let mut result = Vec::new();
    for m in xcap::Monitor::all()? {
        result.push(Target {
            id: m.id()?,
            window: false,
            label: format!(
                "Monitor: {}",
                m.friendly_name().unwrap_or_else(|_| "Display".into())
            ),
        });
    }
    for w in xcap::Window::all()?.into_iter().take(200) {
        if !w.is_minimized().unwrap_or(true)
            && w.width().unwrap_or(0) > 0
            && w.height().unwrap_or(0) > 0
        {
            let title = w.title().unwrap_or_default();
            if !title.is_empty() {
                result.push(Target {
                    id: w.id()?,
                    window: true,
                    label: format!("Window: {title}"),
                });
            }
        }
    }
    Ok(result)
}
pub fn grab(target: &Target) -> Result<Vec<u8>> {
    let image = if target.window {
        xcap::Window::all()?
            .into_iter()
            .find(|w| w.id().ok() == Some(target.id))
            .context("Window closed")?
            .capture_image()?
    } else {
        xcap::Monitor::all()?
            .into_iter()
            .find(|m| m.id().ok() == Some(target.id))
            .context("Monitor disconnected")?
            .capture_image()?
    };
    anyhow::ensure!(
        image.width() as u64 * image.height() as u64 <= 32_000_000,
        "Screenshot is too large"
    );
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image).write_to(&mut bytes, image::ImageFormat::Png)?;
    anyhow::ensure!(
        bytes.get_ref().len() <= 32 * 1024 * 1024,
        "Screenshot exceeds size limit"
    );
    Ok(bytes.into_inner())
}
pub fn vision(bytes: &[u8], crop: Option<[u32; 4]>) -> Result<String> {
    let reader = image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format()?;
    let mut reader = reader;
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(160 * 1024 * 1024);
    reader.limits(limits);
    let mut img = reader.decode()?;
    if let Some([x, y, w, h]) = crop {
        anyhow::ensure!(
            w > 0
                && h > 0
                && x < img.width()
                && y < img.height()
                && w <= img.width() - x
                && h <= img.height() - y,
            "Invalid screenshot selection"
        );
        img = img.crop_imm(x, y, w, h);
    }
    img = img.resize(1600, 1600, image::imageops::FilterType::Triangle);
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 85)
        .encode_image(&img.to_rgb8())?;
    anyhow::ensure!(
        jpeg.len() <= 2 * 1024 * 1024,
        "Screenshot is too large to attach"
    );
    Ok(format!("data:image/jpeg;base64,{}", STANDARD.encode(jpeg)))
}
pub fn rgba(bytes: &[u8]) -> Result<image::RgbaImage> {
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(160 * 1024 * 1024);
    reader.limits(limits);
    Ok(reader.decode()?.into_rgba8())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn screenshot_resize_and_crop() {
        let image = image::RgbaImage::from_pixel(20, 10, image::Rgba([20, 30, 40, 255]));
        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        let url = vision(png.get_ref(), Some([2, 2, 10, 5])).unwrap();
        assert!(url.starts_with("data:image/jpeg;base64,"));
        assert!(vision(png.get_ref(), Some([0, 0, 30, 10])).is_err());
    }
}
