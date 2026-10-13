use anyhow::{Context, Result};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};
#[derive(Clone, Debug)]
pub struct ReleaseNote {
    pub version: String,
    pub date: String,
    pub title: String,
    pub body: String,
    pub page: String,
}
#[derive(Clone, Debug)]
pub struct Release {
    pub changes: Vec<ReleaseNote>,
    pub version: String,
    pub page: String,
    pub url: String,
    pub digest: String,
    pub size: u64,
    pub name: String,
}
#[derive(Clone)]
pub enum Update {
    Checked(std::result::Result<Option<Release>, String>),
    Downloaded(std::result::Result<PathBuf, String>),
}
fn client() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(10)))
        .timeout_global(Some(Duration::from_secs(600)))
        .build()
        .into()
}
fn newer(a: &str, b: &str) -> bool {
    fn parts(v: &str) -> Option<(u32, u32, u32)> {
        let mut n = v.trim_start_matches('v').split(['.', '-']);
        Some((
            n.next()?.parse().ok()?,
            n.next()?.parse().ok()?,
            n.next()?.parse().ok()?,
        ))
    }
    matches!((parts(a),parts(b)),(Some(a),Some(b)) if a>b)
}
pub fn select(data: &Value, current: &str) -> Option<Release> {
    let mut releases = Vec::new();
    for r in data.as_array()? {
        if r["draft"].as_bool() == Some(true) {
            continue;
        }
        for asset in r["assets"].as_array().into_iter().flatten() {
            let name = asset["name"].as_str().unwrap_or("");
            let Some(version) = name.strip_prefix("VectorDash-Rust-Setup-").and_then(|v| {
                v.strip_suffix("-x64.exe")
                    .map(|v| v.strip_suffix("-preview").unwrap_or(v))
            }) else {
                continue;
            };
            if !newer(version, current) {
                continue;
            }
            let url = asset["browser_download_url"].as_str().unwrap_or("");
            let digest = asset["digest"]
                .as_str()
                .unwrap_or("")
                .strip_prefix("sha256:")
                .unwrap_or("");
            let size = asset["size"].as_u64().unwrap_or(0);
            if !url.starts_with("https://github.com/ampersandru/VectorDash/releases/download/")
                || digest.len() != 64
                || !digest.bytes().all(|b| b.is_ascii_hexdigit())
                || !(1..=300_000_000).contains(&size)
            {
                continue;
            }
            releases.push(Release {
                changes: vec![ReleaseNote {
                    version: version.into(),
                    date: r["published_at"]
                        .as_str()
                        .unwrap_or("")
                        .chars()
                        .take(10)
                        .collect(),
                    title: r["name"].as_str().unwrap_or(version).into(),
                    body: r["body"]
                        .as_str()
                        .unwrap_or("No release notes provided.")
                        .chars()
                        .take(20000)
                        .collect(),
                    page: r["html_url"].as_str().unwrap_or("").into(),
                }],
                version: version.into(),
                page: r["html_url"].as_str().unwrap_or("").into(),
                url: url.into(),
                digest: digest.into(),
                size,
                name: name.into(),
            });
        }
    }
    releases.sort_by(|a, b| {
        if newer(&a.version, &b.version) {
            std::cmp::Ordering::Greater
        } else if newer(&b.version, &a.version) {
            std::cmp::Ordering::Less
        } else {
            std::cmp::Ordering::Equal
        }
    });
    let mut changes = releases
        .iter()
        .rev()
        .flat_map(|r| r.changes.clone())
        .collect::<Vec<_>>();
    changes.dedup_by(|a, b| a.version == b.version);
    let mut latest = releases.pop()?;
    latest.changes = changes;
    Some(latest)
}
pub fn check() -> Result<Option<Release>> {
    let mut releases = Vec::new();
    let mut page = 1;
    loop {
        let data:Value=client().get(format!("https://api.github.com/repos/ampersandru/VectorDash/releases?per_page=100&page={page}")).header("User-Agent","VectorDash").header("Accept","application/vnd.github+json").call()?.body_mut().read_json()?;
        let items = data.as_array().context("Invalid release response")?;
        let count = items.len();
        let no_newer = select(&data, env!("CARGO_PKG_VERSION")).is_none();
        releases.extend(items.iter().cloned());
        if count < 100 || no_newer {
            break;
        }
        page += 1;
        anyhow::ensure!(
            page <= 50,
            "Release history is too large; open GitHub for full history"
        );
    }
    Ok(select(&Value::Array(releases), env!("CARGO_PKG_VERSION")))
}
pub fn download(release: &Release, home: &Path) -> Result<PathBuf> {
    anyhow::ensure!(
        release
            .url
            .starts_with("https://github.com/ampersandru/VectorDash/releases/download/")
            && !release.name.contains(['/', '\\']),
        "Invalid update source"
    );
    let directory = home.join("updates");
    fs::create_dir_all(&directory)?;
    let path = directory.join(&release.name);
    let part = directory.join(format!("update-{}.part", crate::config::random_key()));
    let result = (|| -> Result<PathBuf> {
        let mut response = client()
            .get(&release.url)
            .header("User-Agent", "VectorDash")
            .call()?;
        let mut input = response.body_mut().as_reader();
        let mut output = fs::File::create(&part)?;
        let mut hash = Sha256::new();
        let mut bytes = 0u64;
        let mut buf = [0u8; 65536];
        loop {
            let n = input.read(&mut buf)?;
            if n == 0 {
                break;
            }
            bytes += n as u64;
            anyhow::ensure!(
                bytes <= release.size && bytes <= 300_000_000,
                "Update larger than expected"
            );
            hash.update(&buf[..n]);
            output.write_all(&buf[..n])?;
        }
        anyhow::ensure!(
            bytes == release.size
                && format!("{:x}", hash.finalize()).eq_ignore_ascii_case(&release.digest),
            "Update SHA-256 verification failed"
        );
        output.sync_all()?;
        drop(output);
        if path.exists() {
            fs::remove_file(&path)?;
        }
        fs::rename(&part, &path)?;
        Ok(path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&part);
    }
    result
}
pub fn install(path: &Path) -> Result<()> {
    let mut c = std::process::Command::new(path);
    c.args(["/SP-", "/SILENT", "/NORESTART"]);
    crate::content::hidden(&mut c);
    c.spawn().context("Cannot launch verified installer")?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn picks_native_only_and_requires_verified_digest() {
        let data = serde_json::json!([{"assets":[{"name":"VectorDash-Setup-9.0.0.exe"}]},{"html_url":"https://github.com/ampersandru/VectorDash/releases/tag/v0.3.4-preview.1","assets":[{"name":"VectorDash-Rust-Setup-0.3.4-preview-x64.exe","size":100,"digest":format!("sha256:{}","a".repeat(64)),"browser_download_url":"https://github.com/ampersandru/VectorDash/releases/download/v0.3.4-preview.1/setup.exe"}]}]);
        assert_eq!(select(&data, "0.3.3").unwrap().version, "0.3.4");
        assert!(select(&data, "0.3.4").is_none());
        let mut unsafe_data = data.clone();
        unsafe_data[1]["assets"][0]["digest"] = Value::Null;
        assert!(select(&unsafe_data, "0.3.3").is_none());
    }
    #[test]
    fn versions_numeric() {
        assert!(newer("0.3.10", "0.3.9"));
        assert!(!newer("0.2.99", "0.3.0"));
    }
}

pub fn portable() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.join("portable.flag").exists()))
        .unwrap_or(false)
}

pub fn verify(path: &Path, release: &Release) -> Result<()> {
    let mut file = fs::File::open(path)?;
    anyhow::ensure!(
        file.metadata()?.len() == release.size,
        "Update size changed; download again"
    );
    let mut hash = Sha256::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
    }
    anyhow::ensure!(
        format!("{:x}", hash.finalize()).eq_ignore_ascii_case(&release.digest),
        "Update checksum changed; download again"
    );
    Ok(())
}
#[cfg(test)]
mod integrity_tests {
    use super::*;
    #[test]
    fn refuses_modified_download_before_launch() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("setup.exe");
        fs::write(&path, b"verified").unwrap();
        let r = Release {
            changes: vec![],
            version: "9.0.0".into(),
            page: String::new(),
            url: String::new(),
            name: "setup.exe".into(),
            size: 8,
            digest: format!("{:x}", Sha256::digest(b"verified")),
        };
        assert!(verify(&path, &r).is_ok());
        fs::write(&path, b"tampered").unwrap();
        assert!(verify(&path, &r).is_err());
    }
    #[test]
    #[ignore]
    fn github_installer_download_verifies_sha256() {
        let data: Value = client()
            .get("https://api.github.com/repos/ampersandru/VectorDash/releases?per_page=30")
            .header("User-Agent", "VectorDash-test")
            .call()
            .unwrap()
            .body_mut()
            .read_json()
            .unwrap();
        let release = select(&data, "0.0.0").expect("Published native release with SHA-256 digest");
        let home = tempfile::tempdir().unwrap();
        let path = download(&release, home.path()).unwrap();
        verify(&path, &release).unwrap();
        println!(
            "Verified GitHub installer {} ({} bytes)",
            release.version, release.size
        );
    }
}
#[cfg(test)]
mod changelog_tests {
    use super::*;
    #[test]
    fn includes_each_newer_native_release_newest_first() {
        let releases=serde_json::Value::Array(["0.3.4","0.3.6","0.3.5"].iter().map(|version|serde_json::json!({"name":format!("Version {version}"),"published_at":"2026-10-12T00:00:00Z","body":format!("Changes for {version}"),"assets":[{"name":format!("VectorDash-Rust-Setup-{version}-preview-x64.exe"),"size":100,"digest":format!("sha256:{}","a".repeat(64)),"browser_download_url":format!("https://github.com/ampersandru/VectorDash/releases/download/v{version}/setup.exe")}]})).collect());
        let release = select(&releases, "0.3.4").unwrap();
        assert_eq!(release.version, "0.3.6");
        assert_eq!(
            release
                .changes
                .iter()
                .map(|r| r.version.as_str())
                .collect::<Vec<_>>(),
            vec!["0.3.6", "0.3.5"]
        );
        assert_eq!(release.changes[1].body, "Changes for 0.3.5");
    }
}
