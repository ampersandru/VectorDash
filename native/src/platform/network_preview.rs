//! Resolve mapped-drive preview reads without changing the user's mappings.
use std::path::PathBuf;
fn mapped_path(path: &str, remote: &str) -> Option<PathBuf> {
    let bytes = path.as_bytes();
    if bytes.len() < 3
        || !bytes[0].is_ascii_alphabetic()
        || bytes[1] != b':'
        || !matches!(bytes[2], b'\\' | b'/')
        || !remote.starts_with(r"\\")
    {
        return None;
    }
    Some(PathBuf::from(format!(
        "{}\\{}",
        remote.trim_end_matches(['\\', '/']),
        path[3..].replace('/', r"\")
    )))
}
pub fn preview_path(path: &str) -> PathBuf {
    #[cfg(windows)]
    if path.len() >= 3
        && path.as_bytes()[0].is_ascii_alphabetic()
        && path.as_bytes()[1] == b':'
        && matches!(path.as_bytes()[2], b'\\' | b'/')
    {
        if let Some(remote) = remote_root(path.as_bytes()[0]) {
            if let Some(path) = mapped_path(path, &remote) {
                return path;
            }
        }
    }
    PathBuf::from(path)
}
#[cfg(windows)]
fn remote_root(drive: u8) -> Option<String> {
    use windows::{
        Win32::System::Registry::*,
        core::{PCWSTR, w},
    };
    #[link(name = "mpr")]
    unsafe extern "system" {
        fn WNetGetConnectionW(local: *const u16, remote: *mut u16, length: *mut u32) -> u32;
    }
    let local = [drive.to_ascii_uppercase() as u16, b':' as u16, 0];
    let mut buffer = vec![0u16; 32768];
    let mut count = buffer.len() as u32;
    unsafe {
        if WNetGetConnectionW(local.as_ptr(), buffer.as_mut_ptr(), &mut count) == 0 {
            return Some(String::from_utf16_lossy(
                &buffer[..buffer.iter().position(|c| *c == 0)?],
            ));
        }
        // Elevated tokens have a separate DOS drive namespace. Persisted mappings
        // still identify the same share; UNC reads use normal Windows SMB access.
        let key = format!("Network\\{}\0", drive.to_ascii_uppercase() as char)
            .encode_utf16()
            .collect::<Vec<_>>();
        let mut bytes = (buffer.len() * 2) as u32;
        if RegGetValueW(
            HKEY_CURRENT_USER,
            PCWSTR(key.as_ptr()),
            w!("RemotePath"),
            RRF_RT_REG_SZ,
            None,
            Some(buffer.as_mut_ptr().cast()),
            Some(&mut bytes),
        )
        .is_ok()
        {
            return Some(String::from_utf16_lossy(
                &buffer[..buffer.iter().position(|c| *c == 0)?],
            ));
        }
    }
    None
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mapped_unc_join_preserves_names_and_rejects_relative_or_non_network_paths() {
        assert_eq!(
            mapped_path(r"T:\pets\policy packet.pdf", r"\\server\share\").unwrap(),
            PathBuf::from(r"\\server\share\pets\policy packet.pdf")
        );
        assert_eq!(
            mapped_path("t:/pets/猫.pdf", r"\\server\share").unwrap(),
            PathBuf::from(r"\\server\share\pets\猫.pdf")
        );
        for value in [
            r"T:relative.pdf",
            r"\\server\share\a.pdf",
            "https://example.org/a.pdf",
            "",
        ] {
            assert!(mapped_path(value, r"\\server\share").is_none());
        }
        assert!(mapped_path(r"C:\file.pdf", "local").is_none());
    }
}

#[cfg(all(test, windows))]
mod live_tests {
    #[test]
    #[ignore = "Requires an accessible mapped network PDF and bundled PDFium"]
    fn mapped_network_pdf_renders_from_its_unc_share() {
        let input = std::env::var("VECTORDASH_PREVIEW_PDF").expect("Set VECTORDASH_PREVIEW_PDF");
        let resolved = super::preview_path(&input);
        assert!(
            resolved.to_string_lossy().starts_with(r"\\"),
            "Expected a mapped network path: {}",
            resolved.display()
        );
        let jpeg = crate::pdf::preview(std::path::Path::new(&input), 1, "")
            .expect("Mapped PDF preview failed");
        let image = image::load_from_memory(&jpeg).unwrap();
        assert!(image.width() > 100 && image.height() > 100);
        println!(
            "Mapped network PDF rendered successfully: {} x {}",
            image.width(),
            image.height()
        );
    }
}
