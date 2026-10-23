//! Installed-app capabilities: opaque IDs, never model-supplied commands or paths.
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
#[derive(Clone)]
struct Entry {
    id: String,
    name: String,
    path: String,
    target: String,
}
#[derive(Default)]
pub struct Catalog {
    entries: Option<Vec<Entry>>,
}
fn blocked(name: &str) -> bool {
    let name = name.to_lowercase();
    [
        "powershell",
        "command prompt",
        "terminal",
        "git bash",
        "1password",
        "keepass",
        "bitwarden",
        "vectordash",
    ]
    .iter()
    .any(|v| name.contains(v))
}
fn permitted_executable(path: &str) -> bool {
    let name = std::path::Path::new(path)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_ascii_lowercase();
    name.ends_with(".exe")
        && !blocked(&name)
        && !matches!(
            name.as_str(),
            "cmd.exe"
                | "pwsh.exe"
                | "wsl.exe"
                | "bash.exe"
                | "python.exe"
                | "pythonw.exe"
                | "node.exe"
                | "mshta.exe"
                | "rundll32.exe"
                | "regsvr32.exe"
                | "wscript.exe"
                | "cscript.exe"
                | "wt.exe"
                | "regedit.exe"
                | "mmc.exe"
        )
}
fn shortcut_target(path: &str) -> Result<String> {
    use windows::{
        Win32::{
            System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, IPersistFile, STGM_READ},
            UI::Shell::{IShellLinkW, SLGP_RAWPATH, ShellLink},
        },
        core::{Interface, PCWSTR},
    };
    unsafe {
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
        let file: IPersistFile = link.cast()?;
        let wide = path.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
        file.Load(PCWSTR(wide.as_ptr()), STGM_READ)?;
        let mut target = vec![0u16; 32768];
        link.GetPath(&mut target, std::ptr::null_mut(), SLGP_RAWPATH.0 as u32)?;
        Ok(String::from_utf16_lossy(
            &target[..target.iter().position(|v| *v == 0).unwrap_or(target.len())],
        ))
    }
}
fn load() -> Result<Vec<Entry>> {
    // Shell shortcut/store enumeration owns its STA; the control worker remains MTA.
    std::thread::spawn(|| -> Result<Vec<Entry>> {
        use windows::Win32::System::Com::{
            COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize,
        };
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
        }
        struct Apartment;
        impl Drop for Apartment {
            fn drop(&mut self) {
                unsafe {
                    CoUninitialize();
                }
            }
        }
        let _apartment = Apartment;
        let mut out = Vec::new();
        for hit in crate::platform::installed_programs() {
            if hit.text.starts_with("ms-resource:") || blocked(&hit.text) || blocked(&hit.path) {
                continue;
            }
            let target = if hit.path.starts_with("shell:AppsFolder\\") {
                // IDs are discovered by Windows, not accepted from tool arguments.
                hit.path.clone()
            } else if hit.path.to_lowercase().ends_with(".lnk") {
                let Ok(target) = shortcut_target(&hit.path) else {
                    continue;
                };
                if !permitted_executable(&target) {
                    continue;
                }
                target
            } else {
                if !permitted_executable(&hit.path) {
                    continue;
                }
                hit.path.clone()
            };
            out.push(Entry {
                id: format!("a{:016x}", rand::random::<u64>()),
                name: hit.text,
                path: hit.path,
                target,
            });
        }
        out.sort_by_cached_key(|v| {
            (
                v.name.to_lowercase(),
                v.path.starts_with("shell:AppsFolder\\"),
            )
        });
        out.dedup_by(|a, b| a.name.eq_ignore_ascii_case(&b.name));
        out.truncate(10000);
        Ok(out)
    })
    .join()
    .map_err(|_| anyhow::anyhow!("Application discovery thread stopped"))?
}
impl Catalog {
    fn entries(&mut self) -> Result<&[Entry]> {
        if self.entries.is_none() {
            self.entries = Some(load()?);
        }
        Ok(self.entries.as_deref().unwrap())
    }
    pub fn list(&mut self, query: &str, offset: usize) -> Result<Value> {
        let query = query.to_lowercase();
        let all = self.entries()?;
        let matched = all
            .iter()
            .filter(|app| {
                query
                    .split_whitespace()
                    .all(|part| app.name.to_lowercase().contains(part))
            })
            .collect::<Vec<_>>();
        Ok(
            json!({"applications":matched.iter().skip(offset).take(40).map(|app|json!({"app_id":app.id,"name":app.name})).collect::<Vec<_>>(),"total":matched.len(),"next_offset":(offset+40).min(matched.len()),"note":"Installed applications available to launch. This is not a list of running windows. Launch uses an app_id from this session; then call windows to verify the app appeared. Some protected/terminal applications are excluded."}),
        )
    }
    pub fn launch(&mut self, id: &str) -> Result<Value> {
        let app = self
            .entries()?
            .iter()
            .find(|app| app.id == id)
            .cloned()
            .context("Unknown app_id; call applications again")?;
        if app.path.to_lowercase().ends_with(".lnk") {
            let target = shortcut_target(&app.path)?;
            ensure!(
                target.eq_ignore_ascii_case(&app.target) && permitted_executable(&target),
                "Application shortcut changed; start a new chat to rediscover it"
            );
        }
        // No tool argument is passed to ShellExecute: only a discovered, checked entry.
        // Explicitly user-approved launch policy: Explorer owns the normal
        // desktop token/lifecycle; AI Stop terminates control, not the user's app.
        crate::platform::open_from_desktop(&app.path)?;
        Ok(
            json!({"launch_requested":true,"application":app.name,"verified":false,"next":"Windows accepted the application launch request. Call windows, then inspect its window_id to verify it opened. If no controllable window appears yet, do not claim that no apps are running; activation/accessibility can be delayed or unsupported."}),
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn normal_gui_apps_are_allowed_and_command_hosts_excluded() {
        for v in [
            "chrome.exe",
            "firefox.exe",
            "notepad.exe",
            "calc.exe",
            "Code.exe",
        ] {
            assert!(permitted_executable(v));
        }
        for v in [
            "cmd.exe",
            "pwsh.exe",
            "python.exe",
            "mshta.exe",
            "wt.exe",
            "program.ps1",
            "rundll32.exe",
        ] {
            assert!(!permitted_executable(v));
        }
    }
    #[test]
    fn launch_ids_are_not_paths_or_commands() {
        let mut c = Catalog {
            entries: Some(vec![]),
        };
        for v in [
            "calc.exe",
            "https://example.com",
            "C:\\Windows\\calc.exe",
            "a0000000000000000",
        ] {
            assert!(c.launch(v).is_err());
        }
    }
    #[test]
    fn discovery_is_searchable_and_exposes_no_launch_paths() {
        let mut c = Catalog {
            entries: Some(vec![Entry {
                id: "a0000000000000001".into(),
                name: "Example Editor".into(),
                path: "private.exe".into(),
                target: "private.exe".into(),
            }]),
        };
        let result = c.list("editor example", 0).unwrap();
        assert_eq!(result["total"], 1);
        assert_eq!(result["applications"][0]["name"], "Example Editor");
        assert!(!result.to_string().contains("private.exe"));
        assert_eq!(c.list("browser", 0).unwrap()["total"], 0);
    }
}
