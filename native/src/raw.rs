//! Optional camera RAW decoding. LibRaw only runs in a short-lived, budgeted helper.
//! No unencrypted derived-photo cache is written in this first preview release.
use crate::config::Settings;
use anyhow::{Context, Result};
use image::{DynamicImage, GenericImageView};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Mutex, MutexGuard, OnceLock},
    time::{Duration, Instant},
};

pub const RECIPE: &str = "libraw-0.22.2-preview-v1";
const OUTPUT: u64 = 48 * 1024 * 1024;
const PIXELS: u64 = 16_000_000;
static SERIAL: OnceLock<Mutex<()>> = OnceLock::new();

pub fn is_raw(ext: &str) -> bool {
    const EXTENSIONS: &[&str] = &[
        "arw", "cr2", "cr3", "dng", "raf", "nef", "nrw", "orf", "rw2", "pef", "srw", "sr2", "srf",
        "3fr", "erf", "kdc", "mos", "mrw", "rwl", "iiq",
    ];
    EXTENSIONS
        .iter()
        .any(|candidate| ext.eq_ignore_ascii_case(candidate))
}

pub fn helper() -> Result<PathBuf> {
    Ok(std::env::current_exe()?
        .with_file_name("raw-decoder")
        .join("vectordash-raw.exe"))
}
pub fn installed() -> bool {
    helper().is_ok_and(|p| p.is_file())
}

// All parent-process RAW requests (index extraction, thumbnails and Quick Look)
// share this permit. An extraction subprocess has its own permit, never the UI's.
pub fn serial(check: &dyn Fn() -> Result<()>) -> Result<MutexGuard<'static, ()>> {
    let mutex = SERIAL.get_or_init(|| Mutex::new(()));
    let start = Instant::now();
    loop {
        check()?;
        match mutex.try_lock() {
            Ok(guard) => return Ok(guard),
            Err(std::sync::TryLockError::Poisoned(e)) => return Ok(e.into_inner()),
            Err(std::sync::TryLockError::WouldBlock) => {}
        }
        anyhow::ensure!(
            start.elapsed() < Duration::from_secs(120),
            "RAW decoder queue busy; retry the preview later"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}
pub fn stamp(s: &Settings) -> String {
    format!(
        "|raw:{RECIPE}:{}:{}:{}",
        s.index_raw, s.raw_develop, s.raw_max_mib
    )
}
fn orient(image: DynamicImage, flip: u32) -> Result<DynamicImage> {
    Ok(match flip {
        0 => image,
        1 => image.fliph(),
        2 => image.flipv(),
        3 => image.rotate180(),
        4 => image.rotate90().fliph(),
        5 => image.rotate270(),
        6 => image.rotate90(),
        7 => image.rotate90().flipv(),
        _ => anyhow::bail!("Invalid RAW orientation"),
    })
}
fn parse(bytes: &[u8]) -> Result<DynamicImage> {
    anyhow::ensure!(
        bytes.len() >= 24 && bytes.len() as u64 <= OUTPUT + 24,
        "Invalid RAW image message"
    );
    let field = |i| u32::from_le_bytes(bytes[i..i + 4].try_into().unwrap());
    anyhow::ensure!(
        field(0) == 0x31524456 && field(20) as usize == bytes.len() - 24,
        "Invalid RAW image header"
    );
    let image = match field(4) {
        1 => {
            let mut reader = image::ImageReader::with_format(
                std::io::Cursor::new(&bytes[24..]),
                image::ImageFormat::Jpeg,
            );
            let mut limits = image::Limits::default();
            limits.max_alloc = Some(96 * 1024 * 1024);
            limits.max_image_width = Some(16000);
            limits.max_image_height = Some(16000);
            // Inspect dimensions before allocating decoded pixels.
            let (w, h) = image::ImageReader::with_format(
                std::io::Cursor::new(&bytes[24..]),
                image::ImageFormat::Jpeg,
            )
            .into_dimensions()?;
            anyhow::ensure!(
                u64::from(w) * u64::from(h) <= PIXELS,
                "RAW preview exceeds the 16MP budget"
            );
            reader.limits(limits);
            reader.decode()?
        }
        2 => {
            let (w, h) = (field(8), field(12));
            anyhow::ensure!(
                w > 0
                    && h > 0
                    && u64::from(w) * u64::from(h) <= PIXELS
                    && u64::from(w) * u64::from(h) * 3 == field(20) as u64,
                "Invalid RAW bitmap dimensions"
            );
            DynamicImage::ImageRgb8(
                image::RgbImage::from_raw(w, h, bytes[24..].to_vec())
                    .context("Invalid RAW bitmap")?,
            )
        }
        _ => anyhow::bail!("Unsupported RAW preview format"),
    };
    orient(image, field(16))
}
fn invoke(path: &Path, operation: &str) -> Result<DynamicImage> {
    let executable = helper()?;
    anyhow::ensure!(
        executable.is_file(),
        "RAW decoder is missing; reinstall the RAW-preview build"
    );
    let mut command = Command::new(executable);
    command
        .arg(path)
        .arg(operation)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    crate::content::hidden(&mut command);
    let mut child = command.spawn().context("Starting RAW decoder")?;
    let job = match crate::platform::limit_worker_memory(&child, 512) {
        Ok(job) => job,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
    };
    let result = (|| -> Result<DynamicImage> {
        let output = child.stdout.take().context("RAW output")?;
        let error = child.stderr.take().context("RAW diagnostics")?;
        let (tx, rx) = crossbeam_channel::bounded(1);
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let result = output
                .take(OUTPUT + 25)
                .read_to_end(&mut bytes)
                .map(|_| bytes);
            let _ = tx.send(result);
        });
        let diagnostics = std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let _ = error.take(16384).read_to_end(&mut bytes);
            bytes
        });
        child
            .stdin
            .take()
            .context("RAW startup gate")?
            .write_all(b"G")?;
        let bytes = rx
            .recv_timeout(Duration::from_secs(45))
            .context("RAW decoder exceeded 45-second limit")??;
        // stdout closes only after the helper exits (or is killed by its Job).
        let status = child.wait()?;
        let message = String::from_utf8_lossy(&diagnostics.join().unwrap_or_default()).into_owned();
        anyhow::ensure!(
            status.success(),
            "RAW {operation} failed: {}",
            if message.trim().is_empty() {
                "worker exited or exceeded its 512MiB memory budget"
            } else {
                message.trim()
            }
        );
        parse(&bytes)
    })();
    let _ = child.kill();
    let _ = child.wait();
    drop(job);
    result
}
pub fn decode(path: &Path, s: &Settings) -> Result<DynamicImage> {
    anyhow::ensure!(s.index_raw, "Camera RAW support is disabled in Sources");
    anyhow::ensure!(
        std::fs::metadata(path)?.len() <= s.raw_max_mib.clamp(16, 1024) * 1024 * 1024,
        "RAW file exceeds its separate input size limit"
    );
    let _permit = serial(&|| Ok(()))?;
    let image = match invoke(path, "preview") {
        Ok(image) => image,
        Err(error) if s.raw_develop => invoke(path, "develop").with_context(|| {
            format!("Embedded preview unavailable ({error}); development also failed")
        })?,
        Err(error) => return Err(error),
    };
    let (w, h) = image.dimensions();
    Ok(if w.max(h) > 2048 {
        image.thumbnail(2048, 2048)
    } else {
        image
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn raw_opt_in_and_extension_case() {
        let s = Settings::default();
        assert!(!s.index_raw);
        assert!(!s.raw_develop);
        for ext in ["ARW", "cr2", "DNG", "RaF"] {
            assert!(is_raw(ext));
        }
        assert!(!is_raw("jpg"));
        assert!(crate::content::is_image("ARW"));
        assert!(!crate::content::supports_content("arw", &s));
        let enabled = Settings {
            index_raw: true,
            ..s.clone()
        };
        assert!(crate::content::supports_content("arw", &enabled));
        assert!(
            decode(Path::new("not-read.ARW"), &s)
                .unwrap_err()
                .to_string()
                .contains("disabled")
        );
        let old: Settings = serde_json::from_str("{}").unwrap();
        assert!(!old.index_raw);
    }
    #[test]
    fn bounded_messages_and_orientation() {
        assert!(parse(b"bad").is_err());
        let mut bytes = Vec::new();
        for v in [0x31524456u32, 2, 2, 1, 6, 6] {
            bytes.extend(v.to_le_bytes());
        }
        bytes.extend([255, 0, 0, 0, 255, 0]);
        let image = parse(&bytes).unwrap();
        assert_eq!(image.dimensions(), (1, 2));
        bytes[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(parse(&bytes).is_err());
        for flip in 0..8 {
            let image = orient(DynamicImage::new_rgb8(2, 3), flip).unwrap();
            assert_eq!(image.dimensions(), if flip >= 4 { (3, 2) } else { (2, 3) });
        }
    }
    #[test]
    fn queued_raw_work_observes_cancellation() {
        let _held = serial(&|| Ok(())).unwrap();
        let wait = std::thread::spawn(|| serial(&|| anyhow::bail!("Indexing cancelled")).is_err());
        assert!(wait.join().unwrap());
    }
    #[test]
    fn raw_settings_are_fingerprinted() {
        let mut s = Settings::default();
        let disabled = stamp(&s);
        s.index_raw = true;
        let enabled = stamp(&s);
        assert_ne!(disabled, enabled);
        s.raw_develop = true;
        assert_ne!(enabled, stamp(&s));
    }
}
