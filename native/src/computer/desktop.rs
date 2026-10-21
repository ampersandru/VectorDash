//! UIA lives on this helper's MTA thread. Hung providers are terminated by the parent timeout.
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::{CloseHandle, HANDLE, HLOCAL, HWND, LPARAM, LocalFree, POINT, RECT},
        Security::*,
        System::{
            Com::{CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx},
            Threading::*,
        },
        UI::{
            Accessibility::*,
            Controls::{EM_GETSEL, EM_REPLACESEL, EM_SETSEL},
            Input::KeyboardAndMouse::*,
            WindowsAndMessaging::*,
        },
    },
    core::{BOOL, PCWSTR, PWSTR},
};
fn wide(v: &str) -> Vec<u16> {
    v.encode_utf16().chain(Some(0)).collect()
}
/// Drop enabled privileges and lower the helper to medium integrity before COM/capture/browser work.
pub fn prepare_process() -> Result<()> {
    unsafe {
        let _ = windows::Win32::UI::HiDpi::SetProcessDpiAwarenessContext(
            windows::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        );
        let mut token = HANDLE::default();
        OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_ADJUST_DEFAULT | TOKEN_QUERY | TOKEN_ADJUST_PRIVILEGES,
            &mut token,
        )?;
        let result = (|| -> Result<()> {
            AdjustTokenPrivileges(token, true, None, 0, None, None)?;
            // Never raise a low-integrity process. Normal desktop launches are medium/high.
            let mut bytes = vec![0u8; 256];
            let mut size = 0;
            GetTokenInformation(
                token,
                TokenIntegrityLevel,
                Some(bytes.as_mut_ptr().cast()),
                bytes.len() as u32,
                &mut size,
            )?;
            let current = &*(bytes.as_ptr() as *const TOKEN_MANDATORY_LABEL);
            let count = *GetSidSubAuthorityCount(current.Label.Sid) as u32;
            let level = *GetSidSubAuthority(current.Label.Sid, count - 1);
            if level > 8192 {
                let mut sid = PSID::default();
                Authorization::ConvertStringSidToSidW(
                    PCWSTR(wide("S-1-16-8192").as_ptr()),
                    &mut sid,
                )?;
                let label = TOKEN_MANDATORY_LABEL {
                    Label: SID_AND_ATTRIBUTES {
                        Sid: sid,
                        Attributes: 0x20,
                    },
                };
                let lowered = SetTokenInformation(
                    token,
                    TokenIntegrityLevel,
                    &label as *const _ as _,
                    std::mem::size_of_val(&label) as u32 + GetLengthSid(sid),
                );
                let _ = LocalFree(Some(HLOCAL(sid.0)));
                lowered?;
            }
            Ok(())
        })();
        let _ = CloseHandle(token);
        result?;
        CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
    }
    Ok(())
}
#[derive(Clone)]
struct Window {
    hwnd: isize,
    pid: u32,
    executable: String,
    application: String,
    background_only: bool,
    title: String,
}
struct Snapshot {
    id: String,
    window: Window,
    rect: RECT,
    image_size: Option<[u32; 2]>,
    fingerprints: Vec<(String, [i32; 4], String, i32)>,
    use_dwm_bounds: bool,
    at: Instant,
}
#[derive(Default)]
pub struct Desktop {
    automation: Option<IUIAutomation>,
    catalog: super::apps::Catalog,
    windows: HashMap<String, Window>,
    snapshot: Option<Snapshot>,
}
fn process_name(pid: u32) -> Result<String> {
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid)?;
        let mut text = vec![0u16; 32768];
        let mut size = text.len() as u32;
        let result = QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(text.as_mut_ptr()),
            &mut size,
        );
        let _ = CloseHandle(process);
        result?;
        Ok(String::from_utf16_lossy(&text[..size as usize]))
    }
}
fn blocked(executable: &str) -> bool {
    let name = std::path::Path::new(executable)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_ascii_lowercase();
    matches!(
        name.as_str(),
        "vectordash.exe"
            | "cmd.exe"
            | "powershell.exe"
            | "pwsh.exe"
            | "windowsterminal.exe"
            | "conhost.exe"
            | "consent.exe"
            | "credentialuibroker.exe"
            | "logonui.exe"
            | "1password.exe"
            | "keepass.exe"
            | "keepassxc.exe"
            | "bitwarden.exe"
    )
}
fn checked(w: &Window, owner: u32) -> Result<HWND> {
    unsafe {
        let hwnd = HWND(w.hwnd as *mut _);
        let mut pid = 0;
        ensure!(
            IsWindow(Some(hwnd)).as_bool() && IsWindowVisible(hwnd).as_bool(),
            "Target window closed or hidden; list windows again"
        );
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        ensure!(
            pid == w.pid
                && pid != owner
                && pid != std::process::id()
                && !blocked(&w.executable)
                && process_name(pid)? == w.executable,
            "Target identity changed or is protected"
        );
        let root = GetAncestor(hwnd, GA_ROOT);
        if root != hwnd {
            let mut root_pid = 0;
            GetWindowThreadProcessId(root, Some(&mut root_pid));
            let host = process_name(root_pid)?;
            ensure!(
                std::path::Path::new(&host).file_name().is_some_and(|v| v
                    .to_string_lossy()
                    .eq_ignore_ascii_case("ApplicationFrameHost.exe")),
                "Window host changed; list windows again"
            );
            return Ok(root);
        }
        Ok(hwnd)
    }
}
unsafe fn active_modal_window(mut window: Window, owner: u32) -> Result<Window> {
    unsafe {
        let original_root = GetAncestor(checked(&window, owner)?, GA_ROOTOWNER);
        for _ in 0..8 {
            let root = checked(&window, owner)?;
            let mut title = [0u16; 512];
            let length = GetWindowTextW(root, &mut title);
            window.title = String::from_utf16_lossy(&title[..length.max(0) as usize]);
            if IsWindowEnabled(root).as_bool() {
                return Ok(window);
            }
            let popup = GetLastActivePopup(root);
            ensure!(
                popup != root && IsWindowVisible(popup).as_bool(),
                "Selected window is blocked; inspect its dialog from windows"
            );
            let mut pid = 0;
            GetWindowThreadProcessId(popup, Some(&mut pid));
            ensure!(
                pid == window.pid && GetAncestor(popup, GA_ROOTOWNER) == original_root,
                "Blocking dialog is outside the selected application"
            );
            window.hwnd = popup.0 as isize;
        }
        anyhow::bail!("Modal chain is too deep; inspect the active dialog from windows")
    }
}
struct EnumState {
    owner: u32,
    windows: Vec<Window>,
}
unsafe extern "system" fn collect(hwnd: HWND, parameter: LPARAM) -> BOOL {
    unsafe {
        let state = &mut *(parameter.0 as *mut EnumState);
        if state.windows.len() >= 256 {
            return BOOL(0);
        }
        let mut class = [0u16; 128];
        let class_len = GetClassNameW(hwnd, &mut class);
        let class = String::from_utf16_lossy(&class[..class_len.max(0) as usize]);
        if !IsWindowVisible(hwnd).as_bool()
            || matches!(
                class.as_str(),
                "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd"
            )
        {
            return BOOL(1);
        }
        let mut cloaked = 0u32;
        if windows::Win32::Graphics::Dwm::DwmGetWindowAttribute(
            hwnd,
            windows::Win32::Graphics::Dwm::DWMWA_CLOAKED,
            &mut cloaked as *mut _ as _,
            std::mem::size_of::<u32>() as u32,
        )
        .is_ok()
            && cloaked != 0
            && class != "Windows.UI.Core.CoreWindow"
        {
            return BOOL(1);
        }
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == state.owner || pid == std::process::id() {
            return BOOL(1);
        }
        let mut title = [0u16; 512];
        let length = GetWindowTextW(hwnd, &mut title);
        if length <= 0 {
            return BOOL(1);
        }
        if let Ok(executable) = process_name(pid) {
            if !blocked(&executable) {
                state.windows.push(Window {
                    hwnd: hwnd.0 as isize,
                    pid,
                    application: std::path::Path::new(&executable)
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned(),
                    background_only: cloaked != 0,
                    executable,
                    title: String::from_utf16_lossy(&title[..length as usize]),
                });
            }
        }
        BOOL(1)
    }
}
impl Desktop {
    pub fn execute(&mut self, args: &Value, owner: u32, vision: bool) -> Result<Value> {
        if args["action"] == "applications" {
            return self.catalog.list(
                args["text"].as_str().unwrap_or(""),
                args["offset"].as_u64().unwrap_or(0).min(10000) as usize,
            );
        }
        if args["action"] == "launch" {
            if let Some(id) = args["app_id"].as_str() {
                return self.catalog.launch(id);
            }
        }
        if self.automation.is_none() {
            self.automation =
                Some(unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)? });
        }
        // Keep the UIA client alive as long as retained element references.
        let automation = self.automation.as_ref().unwrap().clone();
        let action = args["action"].as_str().unwrap_or("");
        if action == "launch" {
            let (app, file) = match args["application"].as_str() {
                Some("Calculator") => ("Calculator", "calc.exe"),
                Some("Notepad") => ("Notepad", "notepad.exe"),
                Some("Paint") => ("Paint", "mspaint.exe"),
                _ => anyhow::bail!("Choose Calculator, Notepad or Paint"),
            };
            let path = std::path::PathBuf::from(
                std::env::var_os("SystemRoot").context("Windows directory unavailable")?,
            )
            .join("System32")
            .join(file);
            ensure!(path.is_file(), "{app} is not installed");
            // Windows activation proxies can hand off to a packaged app. A successful
            // dispatch is not proof of launch; the model must observe a fresh window.
            let mut before = EnumState {
                owner,
                windows: vec![],
            };
            unsafe {
                let _ = EnumWindows(Some(collect), LPARAM(&mut before as *mut _ as isize));
            }
            let before = before
                .windows
                .into_iter()
                .map(|w| w.hwnd)
                .collect::<std::collections::HashSet<_>>();
            crate::platform::open_from_desktop(
                path.to_str().context("Invalid Windows application path")?,
            )?;
            self.snapshot = None;
            let start = Instant::now();
            let mut stable: Option<(isize, RECT, Instant)> = None;
            while start.elapsed() < Duration::from_secs(3) {
                let observed = self.execute(&json!({"action":"windows"}), owner, false)?;
                if let Some(window) = observed["windows"].as_array().and_then(|items| {
                    items
                        .iter()
                        .filter(|w| {
                            let exe = w["application"].as_str().unwrap_or("");
                            exe.eq_ignore_ascii_case(file)
                                || (app == "Calculator"
                                    && exe.eq_ignore_ascii_case("CalculatorApp.exe"))
                        })
                        .find(|w| {
                            self.windows
                                .get(w["window_id"].as_str().unwrap_or(""))
                                .is_some_and(|target| {
                                    !before.contains(&target.hwnd)
                                        || start.elapsed() >= Duration::from_secs(2)
                                })
                        })
                }) {
                    let target = self
                        .windows
                        .get(window["window_id"].as_str().unwrap_or(""))
                        .context("Activation window disappeared")?;
                    let mut rect = RECT::default();
                    unsafe {
                        GetWindowRect(HWND(target.hwnd as *mut _), &mut rect)?;
                    }
                    let settled = stable.as_ref().is_some_and(|(hwnd, bounds, at)| {
                        *hwnd == target.hwnd
                            && *bounds == rect
                            && at.elapsed() >= Duration::from_millis(400)
                    });
                    if settled {
                        return Ok(
                            json!({"launch_requested":true,"application":app,"window_id":window["window_id"],"window_present":true,"next":"Application window observed after activation (may be an existing instance). Inspect this window_id to verify its content before interacting."}),
                        );
                    }
                    if !stable
                        .as_ref()
                        .is_some_and(|(hwnd, bounds, _)| *hwnd == target.hwnd && *bounds == rect)
                    {
                        stable = Some((target.hwnd, rect, Instant::now()));
                    }
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            return Ok(
                json!({"launch_requested":true,"application":app,"window_present":false,"next":"No accessible window observed yet. Call windows once after activation. Do not claim launch succeeded without observing the app."}),
            );
        }
        if action == "windows" {
            let mut state = EnumState {
                owner,
                windows: vec![],
            };
            unsafe {
                let _ = EnumWindows(Some(collect), LPARAM(&mut state as *mut _ as isize));
            }
            self.windows.clear();
            self.snapshot = None;
            let mut windows = vec![];
            for window in state.windows {
                if windows.len() >= 64 {
                    break;
                }
                if window.background_only {
                    // UWP activation leaves cloaked splash/stale CoreWindows behind.
                    // Keep useful background accessibility, not empty placeholders.
                    let ready = unsafe {
                        (|| -> Result<bool> {
                            let automation: IUIAutomation =
                                CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)?;
                            let root = automation.ElementFromHandle(HWND(window.hwnd as *mut _))?;
                            Ok(!root.CurrentName()?.is_empty()
                                && automation
                                    .ControlViewWalker()?
                                    .GetFirstChildElement(&root)
                                    .is_ok())
                        })()
                        .unwrap_or(false)
                    };
                    if !ready {
                        continue;
                    }
                }
                let id = format!("w{:016x}", rand::random::<u64>());
                windows.push(json!({"window_id":id,"title":window.title,"background_accessibility_only":window.background_only,"minimized":unsafe{IsIconic(HWND(window.hwnd as *mut _)).as_bool()},"application":window.application}));
                self.windows.insert(id, window);
            }
            return Ok(
                json!({"windows":windows,"note":"Filtered app windows, including minimized and existing browsers. Desktop/taskbar, protected/VectorDash/known terminal windows are excluded; elevated windows may be inaccessible. Packaged CoreWindows marked background_accessibility_only support UIA element actions (e.g. Calculator buttons), not foreground/pixel input. Empty results do NOT mean no apps are running. Use launch for Calculator/Notepad/Paint; focus a minimized target before inspect. Screenshot coordinates are per-window pixels, not desktop coordinates."}),
            );
        }
        let id = args["window_id"]
            .as_str()
            .context("Provide a window_id from windows")?;
        let window = self
            .windows
            .get(id)
            .cloned()
            .context("Unknown window_id; list windows first")?;
        let window = unsafe { active_modal_window(window, owner)? };
        let hwnd = checked(&window, owner)?;
        if action == "focus" {
            ensure!(
                !window.background_only,
                "Hosted app exposes background accessibility only. Use inspect and UIA element actions; for foreground interaction choose its visible ApplicationFrameHost window instead."
            );
            unsafe {
                let _guard = foreground(hwnd, owner)?;
            }
            self.snapshot = None;
            std::thread::sleep(Duration::from_millis(200));
            return match self.execute(&json!({"action":"inspect","window_id":id}), owner, vision) {
                Ok(mut observed) => {
                    observed["focused"] = json!(true);
                    Ok(observed)
                }
                Err(error) => Ok(
                    json!({"focused":true,"window_id":id,"observation_error":error.to_string(),"next":"Inspect this window before interacting."}),
                ),
            };
        }
        ensure!(
            !unsafe { IsIconic(hwnd).as_bool() },
            "Window is minimized; use focus before inspecting or interacting"
        );
        if matches!(action, "inspect" | "screenshot") {
            ensure!(
                action != "screenshot" || vision,
                "Enable vision support for the selected AI model to send screenshots"
            );
            let root = unsafe { automation.ElementFromHandle(hwnd)? };
            let walker = unsafe { automation.ControlViewWalker()? };
            let mut nodes = vec![];
            let mut rows = vec![];
            let start = Instant::now();
            let offset = args["offset"].as_u64().unwrap_or(0).min(4095) as usize;
            let limit = (offset + 121).min(4096);
            unsafe {
                walk(
                    &root,
                    &walker,
                    0,
                    &mut nodes,
                    &mut rows,
                    start,
                    owner,
                    offset,
                    limit,
                    args["text_offset"].as_u64().unwrap_or(0).min(1_000_000) as i32,
                );
            }
            let mut rect = RECT::default();
            unsafe {
                GetWindowRect(hwnd, &mut rect)?;
            }
            let nonce = format!("d{:016x}", rand::random::<u64>());
            let mut image_size = None;
            let mut use_dwm_bounds = false;
            let fingerprints: Vec<(String, [i32; 4], String, i32)> = rows
                .iter()
                .enumerate()
                .map(|(index, row)| {
                    let v = if index >= offset && index < offset + 120 {
                        row
                    } else {
                        &Value::Null
                    };
                    (
                        v["name"].as_str().unwrap_or("").to_owned(),
                        [
                            v["bounds"][0].as_i64().unwrap_or(0) as i32,
                            v["bounds"][1].as_i64().unwrap_or(0) as i32,
                            v["bounds"][2].as_i64().unwrap_or(0) as i32,
                            v["bounds"][3].as_i64().unwrap_or(0) as i32,
                        ],
                        v["automation_id"].as_str().unwrap_or("").to_owned(),
                        v["control_type"].as_i64().unwrap_or(0) as i32,
                    )
                })
                .collect();
            let mut identity_counts = HashMap::<String, usize>::new();
            for row in &rows {
                if let Some(id) = row["automation_id"].as_str().filter(|id| !id.is_empty()) {
                    *identity_counts.entry(id.to_owned()).or_default() += 1;
                }
            }
            let total = rows.len();
            let rows = rows
                .into_iter()
                .skip(offset)
                .take(120)
                .map(|mut row| {
                    if let Some(fields) = row.as_object_mut() {
                        for key in ["automation_id", "value", "text"] {
                            if fields.get(key).is_some_and(|v| v.as_str() == Some("")) {
                                fields.remove(key);
                            }
                        }
                        if fields
                            .get("automation_id")
                            .and_then(Value::as_str)
                            .is_some_and(|id| identity_counts.get(id).copied().unwrap_or(0) > 1)
                        {
                            fields.remove("automation_id");
                        }
                        if action != "screenshot" {
                            fields.remove("bounds");
                        }
                    }
                    row
                })
                .collect::<Vec<_>>();
            let timed_out = start.elapsed() >= Duration::from_millis(1200);
            let end = offset + rows.len();
            let next_page = if end > offset && end < 4096 && (total > end || timed_out) {
                Some(end)
            } else {
                None
            };
            let mut value = json!({"offset":offset,"total_elements":total,"next_offset":next_page,"window_id":id,"snapshot_id":nonce,"title":window.title,"elements":rows,"truncated":offset+rows.len()<total||nodes.len()>=limit||start.elapsed()>Duration::from_millis(1200),"total_is_lower_bound":nodes.len()>=limit||start.elapsed()>Duration::from_millis(1200),"coordinate_space":"Screenshot pixels; window must not move before clicking","elapsed_ms":start.elapsed().as_millis()});
            if value["truncated"] == true && value["next_offset"].is_null() {
                value["inspection_limited"] = json!(true);
                value["inspection_note"] = json!(
                    "Traversal hit a provider/time/depth or 4096-element bound. Repeating this offset cannot reveal more; use a focused content pane or screenshot."
                );
            }
            if self
                .windows
                .get(id)
                .is_some_and(|original| original.hwnd != window.hwnd)
            {
                value["modal_dialog"] = json!(true);
                value["note"] = json!(
                    "The selected application is blocked by this owned dialog. These elements, title and screenshot refer to the dialog; finish or cancel it before editing its parent."
                );
            }
            if action == "screenshot" {
                if let Ok(image) = fast_capture(hwnd) {
                    let max = image.width().max(image.height()).min(1600);
                    let resized = image.thumbnail(max, max);
                    image_size = Some([resized.width(), resized.height()]);
                    let bytes = crate::content::jpeg(&image, max)?;
                    ensure!(
                        bytes.len() <= 2 * 1024 * 1024,
                        "Screenshot exceeds size limit"
                    );
                    use base64::Engine as _;
                    value["image_width"] = json!(resized.width());
                    value["image_height"] = json!(resized.height());
                    value["image_url"] = json!(format!(
                        "data:image/jpeg;base64,{}",
                        base64::engine::general_purpose::STANDARD.encode(bytes)
                    ));
                } else {
                    let bytes = crate::capture::grab(&crate::capture::Target {
                        id: window.hwnd as u32,
                        window: true,
                        label: window.title.clone(),
                    })?;
                    let source = image::load_from_memory(&bytes)?;
                    let max = source.width().max(source.height()).min(1600);
                    let image = source.thumbnail(max, max);
                    image_size = Some([image.width(), image.height()]);
                    value["image_width"] = json!(image.width());
                    value["image_height"] = json!(image.height());
                    use base64::Engine as _;
                    value["image_url"] = json!(format!(
                        "data:image/jpeg;base64,{}",
                        base64::engine::general_purpose::STANDARD
                            .encode(crate::content::jpeg(&source, max)?)
                    ));
                    unsafe {
                        let mut bounds = RECT::default();
                        if windows::Win32::Graphics::Dwm::DwmGetWindowAttribute(
                            hwnd,
                            windows::Win32::Graphics::Dwm::DWMWA_EXTENDED_FRAME_BOUNDS,
                            &mut bounds as *mut _ as _,
                            std::mem::size_of::<RECT>() as u32,
                        )
                        .is_ok()
                        {
                            rect = bounds;
                            use_dwm_bounds = true;
                        }
                    }
                }
            }
            use std::hash::{Hash, Hasher};
            let mut view_state = std::collections::hash_map::DefaultHasher::new();
            fingerprints.hash(&mut view_state);
            value["view_state"] = json!(view_state.finish());
            value["window_bounds"] = json!([rect.left, rect.top, rect.right, rect.bottom]);
            value["element_bounds_space"] =
                json!("Physical desktop pixels; image_bounds uses screenshot pixels");
            if let Some(size) = image_size {
                for row in value["elements"].as_array_mut().unwrap() {
                    if let Some(bounds) = row["bounds"].as_array() {
                        let b = bounds
                            .iter()
                            .map(|v| v.as_i64().unwrap_or(0) as f64)
                            .collect::<Vec<_>>();
                        if rect.right > rect.left && rect.bottom > rect.top {
                            let coords = [
                                (b[0] - rect.left as f64) * size[0] as f64
                                    / (rect.right - rect.left) as f64,
                                (b[1] - rect.top as f64) * size[1] as f64
                                    / (rect.bottom - rect.top) as f64,
                                (b[2] - rect.left as f64) * size[0] as f64
                                    / (rect.right - rect.left) as f64,
                                (b[3] - rect.top as f64) * size[1] as f64
                                    / (rect.bottom - rect.top) as f64,
                            ];
                            row["image_bounds"] = json!(coords.map(|v| (v * 10.0).round() / 10.0));
                        }
                    }
                }
            }
            self.snapshot = Some(Snapshot {
                id: nonce,
                window,
                rect,
                image_size,
                fingerprints,
                use_dwm_bounds,
                at: Instant::now(),
            });
            return Ok(value);
        }
        // Resolve only a unique identity present in the current observation.
        // This avoids asking models to count changing numeric button positions.
        let mut resolved_args = args.clone();
        if let Some(identity) = args["automation_id"].as_str() {
            let snapshot = self
                .snapshot
                .as_ref()
                .context("Inspect the target before acting")?;
            let matches = snapshot
                .fingerprints
                .iter()
                .enumerate()
                .filter(|(_, f)| f.2 == identity)
                .map(|(i, _)| i)
                .collect::<Vec<_>>();
            let index = if let Some(number) = args["element"].as_u64() {
                let index = number as usize;
                ensure!(
                    matches.contains(&index),
                    "Element number does not match automation_id; no action performed"
                );
                index
            } else {
                ensure!(
                    matches.len() == 1,
                    "Automation identity is missing or ambiguous; use the numbered element from this snapshot"
                );
                matches[0]
            };
            resolved_args["element"] = json!(index);
        }
        if let Some(name) = args["element_name"].as_str() {
            let snapshot = self
                .snapshot
                .as_ref()
                .context("Inspect the target before acting")?;
            let matches = snapshot
                .fingerprints
                .iter()
                .enumerate()
                .filter(|(_, f)| !f.0.is_empty() && f.0 == name)
                .map(|(i, _)| i)
                .collect::<Vec<_>>();
            ensure!(
                matches.len() == 1,
                "Observed element name is missing or ambiguous; use its numbered element"
            );
            if let Some(index) = resolved_args["element"].as_u64() {
                ensure!(
                    index as usize == matches[0],
                    "Element number does not match element_name; no action performed"
                );
            }
            resolved_args["element"] = json!(matches[0]);
        }
        let args = &resolved_args;
        let snapshot = self
            .snapshot
            .as_ref()
            .context("Inspect the target before acting")?;
        ensure!(
            snapshot.window.hwnd == window.hwnd
                && snapshot.id == args["snapshot_id"].as_str().unwrap_or("")
                && snapshot.at.elapsed() < Duration::from_secs(90),
            "Stale desktop snapshot; inspect again"
        );
        // Resolve a fresh provider reference, not a COM proxy held across LLM turns.
        // Its automation identity, type, name, bounds and ancestry must still match.
        let fresh_node = if let Some(index) = args["element"].as_u64() {
            let fingerprint = snapshot
                .fingerprints
                .get(index as usize)
                .context("Unknown element")?;
            let (property, value) = if !fingerprint.2.is_empty() {
                (UIA_AutomationIdPropertyId, fingerprint.2.as_str())
            } else {
                (UIA_NamePropertyId, fingerprint.0.as_str())
            };
            ensure!(
                !value.is_empty(),
                "Element lacks a stable name/automation identity; inspect again"
            );
            unsafe {
                let root = automation.ElementFromHandle(hwnd)?;
                let condition = automation.CreatePropertyCondition(
                    property,
                    &windows::Win32::System::Variant::VARIANT::from(value),
                )?;
                let candidates = root.FindAll(TreeScope_Subtree, &condition)?;
                let mut matching = Vec::new();
                for i in 0..candidates.Length()?.min(512) {
                    let candidate = candidates.GetElement(i)?;
                    let bounds = candidate.CurrentBoundingRectangle()?;
                    let name = candidate
                        .CurrentName()?
                        .to_string()
                        .chars()
                        .take(240)
                        .collect::<String>();
                    if candidate.CurrentAutomationId()?.to_string() == fingerprint.2
                        && candidate.CurrentControlType()?.0 == fingerprint.3
                        && name == fingerprint.0
                        && stable_control_bounds(
                            [bounds.left, bounds.top, bounds.right, bounds.bottom],
                            fingerprint.1,
                        )
                    {
                        matching.push(candidate);
                    }
                }
                ensure!(
                    matching.len() == 1,
                    "Observed element no longer has a unique matching identity, type, name and bounds; inspect again"
                );
                matching.pop()
            }
        } else {
            None
        };
        let node = fresh_node.as_ref();
        if let Some(node) = node {
            unsafe {
                let index = args["element"].as_u64().unwrap_or(0) as usize;
                let fingerprint = snapshot
                    .fingerprints
                    .get(index)
                    .context("Unknown element")?;
                ensure!(
                    node.CurrentAutomationId()?.to_string() == fingerprint.2
                        && node.CurrentControlType()?.0 == fingerprint.3,
                    "Element identity/type changed; inspect again"
                );
                let name = node
                    .CurrentName()
                    .context("Revalidating element name failed")?
                    .to_string()
                    .chars()
                    .take(240)
                    .collect::<String>();
                let rect = node
                    .CurrentBoundingRectangle()
                    .context("Revalidating element bounds failed")?;
                ensure!(
                    name == fingerprint.0
                        && stable_control_bounds(
                            [rect.left, rect.top, rect.right, rect.bottom],
                            fingerprint.1
                        ),
                    "Element changed since observation; inspect again"
                );
                let root = automation.ElementFromHandle(hwnd)?;
                let walker = automation.ControlViewWalker()?;
                let mut ancestor = node.clone();
                let mut belongs = false;
                for _ in 0..64 {
                    if automation
                        .CompareElements(&ancestor, &root)
                        .context("Revalidating element ancestry failed")?
                        .as_bool()
                    {
                        belongs = true;
                        break;
                    }
                    ancestor = walker
                        .GetParentElement(&ancestor)
                        .context("Reading element ancestry failed")?;
                }
                ensure!(belongs, "Element is no longer in the selected window");
                ensure!(
                    !node.CurrentIsPassword()?.as_bool()
                        && node.CurrentProcessId()? as u32 != owner,
                    "Protected element"
                );
                ensure!(node.CurrentIsEnabled()?.as_bool(), "Element is disabled");
            }
        }
        let root = unsafe { automation.ElementFromHandle(hwnd)? };
        let mut scroll_report = None;
        unsafe {
            match action {
                "click" => {
                    if let Some(node) = node {
                        if let Some(child) = native_control(node, hwnd, &["button"]) {
                            ensure!(
                                SendMessageTimeoutW(
                                    child,
                                    BM_CLICK,
                                    windows::Win32::Foundation::WPARAM(0),
                                    LPARAM(0),
                                    SMTO_ABORTIFHUNG | SMTO_BLOCK,
                                    500,
                                    None
                                )
                                .0 != 0,
                                "Button did not respond"
                            );
                        } else if let Ok(pattern) = node
                            .GetCurrentPatternAs::<IUIAutomationInvokePattern>(UIA_InvokePatternId)
                        {
                            pattern
                                .Invoke()
                                .context("UI Automation button invocation failed")?;
                        } else if let Ok(pattern) = node
                            .GetCurrentPatternAs::<IUIAutomationTogglePattern>(UIA_TogglePatternId)
                        {
                            pattern.Toggle()?;
                        } else if let Ok(pattern) = node
                            .GetCurrentPatternAs::<IUIAutomationSelectionItemPattern>(
                                UIA_SelectionItemPatternId,
                            )
                        {
                            pattern.Select()?;
                        } else {
                            let rect = node.CurrentBoundingRectangle()?;
                            ensure!(
                                rect.right > rect.left && rect.bottom > rect.top,
                                "Element has no clickable bounds"
                            );
                            let _visibility = foreground(hwnd, owner)?;
                            click(
                                &automation,
                                hwnd,
                                POINT {
                                    x: (rect.left + rect.right) / 2,
                                    y: (rect.top + rect.bottom) / 2,
                                },
                            )?;
                        }
                    } else {
                        ensure!(
                            !window.background_only,
                            "This hosted window supports background UIA actions only; screenshot coordinates do not target its visible host. Use an element id."
                        );
                        let size = snapshot
                            .image_size
                            .context("Coordinates require a screenshot snapshot")?;
                        let x = args["x"]
                            .as_f64()
                            .context("Provide element id or screenshot x/y")?;
                        let y = args["y"].as_f64().context("Provide screenshot y")?;
                        ensure!(
                            x.is_finite()
                                && y.is_finite()
                                && x >= 0.0
                                && y >= 0.0
                                && x < size[0] as f64
                                && y < size[1] as f64,
                            "Coordinates outside screenshot"
                        );
                        let mut current = RECT::default();
                        GetWindowRect(hwnd, &mut current)?;
                        let mut bounds = RECT::default();
                        if snapshot.use_dwm_bounds
                            && windows::Win32::Graphics::Dwm::DwmGetWindowAttribute(
                                hwnd,
                                windows::Win32::Graphics::Dwm::DWMWA_EXTENDED_FRAME_BOUNDS,
                                &mut bounds as *mut _ as _,
                                std::mem::size_of::<RECT>() as u32,
                            )
                            .is_ok()
                        {
                            current = bounds;
                        }
                        ensure!(
                            current == snapshot.rect,
                            "Window moved or resized; capture a fresh screenshot"
                        );
                        let _visibility = foreground(hwnd, owner)?;
                        click(&automation, hwnd, screenshot_point(current, size, x, y)?)?;
                    }
                }
                "type" => {
                    let node = node.context("Typing requires an editable element id")?;
                    let text = args["text"].as_str().context("Provide text")?;
                    let mode = args["text_mode"]
                        .as_str()
                        .context("Choose an explicit text_mode")?;
                    if mode == "replace_text" {
                        let find = args["find_text"]
                            .as_str()
                            .context("Provide find_text for exact replacement")?;
                        if let Some(child) =
                            native_control(node, hwnd, &["edit", "richedit20w", "richedit50w"])
                        {
                            let mut length = 0usize;
                            ensure!(
                                SendMessageTimeoutW(
                                    child,
                                    WM_GETTEXTLENGTH,
                                    windows::Win32::Foundation::WPARAM(0),
                                    LPARAM(0),
                                    SMTO_ABORTIFHUNG | SMTO_BLOCK,
                                    500,
                                    Some(&mut length)
                                )
                                .0 != 0
                                    && length <= 1_000_000,
                                "Editor text is unavailable or too large for native exact selection; use Find/Replace"
                            );
                            let mut buffer = vec![0u16; length + 1];
                            let mut read = 0usize;
                            ensure!(
                                SendMessageTimeoutW(
                                    child,
                                    WM_GETTEXT,
                                    windows::Win32::Foundation::WPARAM(buffer.len()),
                                    LPARAM(buffer.as_mut_ptr() as isize),
                                    SMTO_ABORTIFHUNG | SMTO_BLOCK,
                                    500,
                                    Some(&mut read)
                                )
                                .0 != 0
                                    && read <= length,
                                "Could not read editor text; no text changed"
                            );
                            let current = String::from_utf16_lossy(&buffer[..read]);
                            let offset = current
                                .find(find)
                                .context("Exact text not found; no text changed")?;
                            let start = current[..offset].encode_utf16().count();
                            let end = start + find.encode_utf16().count();
                            ensure!(
                                SendMessageTimeoutW(
                                    child,
                                    EM_SETSEL,
                                    windows::Win32::Foundation::WPARAM(start),
                                    LPARAM(end as isize),
                                    SMTO_ABORTIFHUNG | SMTO_BLOCK,
                                    500,
                                    None
                                )
                                .0 != 0,
                                "Could not select exact text; no text changed"
                            );
                        } else {
                            let _visibility = foreground(hwnd, owner)?;
                            focus_element(&automation, node, &root, owner)?;
                            let pattern = node.GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId)
                            .context("This editor does not expose exact text selection; use its Find/Replace UI instead. No text was changed")?;
                            let range = pattern
                                .DocumentRange()?
                                .FindText(&windows::core::BSTR::from(find), false, false)
                                .context("Exact text not found; no text was changed")?;
                            ensure!(
                                range.GetText(-1)?.to_string() == find,
                                "Exact text changed before selection; no text was changed"
                            );
                            range.Select()?;
                        }
                    }
                    let mode = if mode == "replace_text" {
                        "replace_selection"
                    } else {
                        mode
                    };
                    if mode != "replace_all"
                        && native_control(node, hwnd, &["edit", "richedit20w", "richedit50w"])
                            .is_some()
                    {
                        let child =
                            native_control(node, hwnd, &["edit", "richedit20w", "richedit50w"])
                                .unwrap();
                        ensure!(
                            GetWindowLongPtrW(child, GWL_STYLE) & 0x0800 == 0,
                            "Field is read-only"
                        );
                        let mut start = 0u32;
                        let mut end = 0u32;
                        ensure!(
                            SendMessageTimeoutW(
                                child,
                                EM_GETSEL,
                                windows::Win32::Foundation::WPARAM(&mut start as *mut _ as usize),
                                LPARAM(&mut end as *mut _ as isize),
                                SMTO_ABORTIFHUNG | SMTO_BLOCK,
                                500,
                                None
                            )
                            .0 != 0,
                            "Could not read selection; no text changed"
                        );
                        ensure!(
                            mode != "replace_selection" || start != end,
                            "No nonempty selection in the intended editor; no text changed"
                        );
                        let value = wide(text);
                        ensure!(
                            SendMessageTimeoutW(
                                child,
                                EM_REPLACESEL,
                                windows::Win32::Foundation::WPARAM(1),
                                LPARAM(value.as_ptr() as isize),
                                SMTO_ABORTIFHUNG | SMTO_BLOCK,
                                500,
                                None
                            )
                            .0 != 0,
                            "Selection replacement did not respond; inspect before retrying"
                        );
                    } else if mode != "replace_all" {
                        let _visibility = foreground(hwnd, owner)?;
                        focus_element(&automation, node, &root, owner)?;
                        if mode == "replace_selection" {
                            let mut selected = false;
                            if let Some(child) =
                                native_control(node, hwnd, &["edit", "richedit20w", "richedit50w"])
                            {
                                let mut start = 0u32;
                                let mut end = 0u32;
                                ensure!(
                                    SendMessageTimeoutW(
                                        child,
                                        EM_GETSEL,
                                        windows::Win32::Foundation::WPARAM(
                                            &mut start as *mut _ as usize
                                        ),
                                        LPARAM(&mut end as *mut _ as isize),
                                        SMTO_ABORTIFHUNG | SMTO_BLOCK,
                                        500,
                                        None
                                    )
                                    .0 != 0,
                                    "Could not read selection; no text changed"
                                );
                                selected = start != end;
                            } else if let Ok(pattern) = node
                                .GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId)
                            {
                                let ranges = pattern.GetSelection()?;
                                for i in 0..ranges.Length()? {
                                    if !ranges.GetElement(i)?.GetText(1)?.is_empty() {
                                        selected = true;
                                        break;
                                    }
                                }
                            }
                            ensure!(
                                selected,
                                "No nonempty selection in the intended editor; no text changed. Select text first or use insert."
                            );
                        }
                        if mode == "replace_selection" && text.is_empty() {
                            keys(&super::key_codes("Delete")?)?;
                        } else {
                            insert_text(text, &automation, node, hwnd)?;
                        }
                    } else if let Some(child) =
                        native_control(node, hwnd, &["edit", "richedit20w", "richedit50w"])
                    {
                        ensure!(
                            GetWindowLongPtrW(child, GWL_STYLE) & 0x0800 == 0,
                            "Field is read-only"
                        );
                        ensure!(
                            SendMessageTimeoutW(
                                child,
                                EM_SETSEL,
                                windows::Win32::Foundation::WPARAM(0),
                                LPARAM(-1),
                                SMTO_ABORTIFHUNG | SMTO_BLOCK,
                                500,
                                None
                            )
                            .0 != 0,
                            "Could not select field text; no text changed"
                        );
                        let value = wide(text);
                        ensure!(
                            SendMessageTimeoutW(
                                child,
                                EM_REPLACESEL,
                                windows::Win32::Foundation::WPARAM(1),
                                LPARAM(value.as_ptr() as isize),
                                SMTO_ABORTIFHUNG | SMTO_BLOCK,
                                500,
                                None
                            )
                            .0 != 0,
                            "Text edit did not respond; inspect before retrying"
                        );
                    } else if let Ok(pattern) =
                        node.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
                    {
                        ensure!(
                            !pattern.CurrentIsReadOnly()?.as_bool(),
                            "Field is read-only"
                        );
                        pattern.SetValue(&windows::core::BSTR::from(text))?;
                    } else {
                        ensure!(
                            matches!(
                                node.CurrentControlType()?,
                                UIA_EditControlTypeId | UIA_DocumentControlTypeId
                            ),
                            "Element is not an editable field"
                        );
                        let _visibility = foreground(hwnd, owner)?;
                        focus_element(&automation, node, &root, owner)?;
                        keys(&super::key_codes("Ctrl+A")?)?;
                        insert_text(text, &automation, node, hwnd)?;
                    }
                }
                "key" => {
                    ensure!(
                        !window.background_only,
                        "Hosted app supports background UIA actions only; choose its visible host for keyboard input."
                    );
                    let _visibility = foreground(hwnd, owner)?;
                    if let Some(node) = node {
                        focus_element(&automation, node, &root, owner)?;
                    }
                    let focused = automation.GetFocusedElement()?;
                    ensure!(
                        is_descendant(&automation, &focused, &root)?,
                        "Focused control is outside the selected window; no keys sent"
                    );
                    ensure!(
                        !focused.CurrentIsPassword()?.as_bool()
                            && focused.CurrentProcessId()? as u32 != owner,
                        "Password/protected controls require manual input"
                    );
                    keys(&super::key_codes(args["keys"].as_str().unwrap_or(""))?)?;
                }
                "scroll" => {
                    let delta = args["delta"].as_i64().context("Provide scroll delta")?;
                    if let Some((container, pattern)) =
                        scroll_container(&automation, node, &root, owner)?
                    {
                        let before = pattern.CurrentVerticalScrollPercent()?;
                        let steps = delta.unsigned_abs().div_ceil(120).clamp(1, 25);
                        for _ in 0..steps {
                            let position = pattern.CurrentVerticalScrollPercent()?;
                            if (delta > 0 && position >= 100.0) || (delta < 0 && position <= 0.0) {
                                break;
                            }
                            pattern.Scroll(
                                ScrollAmount_NoAmount,
                                if delta > 0 {
                                    ScrollAmount_SmallIncrement
                                } else {
                                    ScrollAmount_SmallDecrement
                                },
                            )?;
                        }
                        std::thread::sleep(Duration::from_millis(100));
                        let after = pattern.CurrentVerticalScrollPercent()?;
                        scroll_report = Some(
                            json!({"method":"uia","moved":(after-before).abs()>0.001,"before_percent":before,"after_percent":after,"delta":delta}),
                        );
                        let _ = container;
                    } else {
                        let _visibility = foreground(hwnd, owner)?;
                        let target = node.unwrap_or(&root);
                        let rect = target.CurrentBoundingRectangle()?;
                        move_cursor(
                            hwnd,
                            POINT {
                                x: (rect.left + rect.right) / 2,
                                y: (rect.top + rect.bottom) / 2,
                            },
                        )?;
                        send(&[mouse_input(0, 0, (-delta as i32) as u32, MOUSEEVENTF_WHEEL)])?;
                        scroll_report = Some(
                            json!({"method":"wheel","moved":Value::Null,"delta":delta,"note":"Wheel input sent; movement is unverified. If unchanged, target the content pane or use focused PageDown/PageUp rather than repeating."}),
                        );
                    }
                }
                _ => anyhow::bail!("Unsupported computer action"),
            }
        }
        let observation_offset = args["element"].as_u64().unwrap_or(0) / 120 * 120;
        self.snapshot = None;
        // Let asynchronous UI dispatch/layout finish before observing the result.
        // In particular, WinUI navigation panes animate after Invoke returns.
        std::thread::sleep(Duration::from_millis(200));
        // Never repeat a mutation if its verification fails. The old snapshot
        // stays invalid, while a successful read supplies the next fresh nonce.
        match self.execute(
            &json!({"action":"inspect","window_id":id,"offset":observation_offset}),
            owner,
            vision,
        ) {
            Ok(mut observed) => {
                observed["performed"] = json!(true);
                if let Some(report) = &scroll_report {
                    observed["scroll_result"] = report.clone();
                    if report["moved"] == false {
                        observed["progress_warning"] = json!(
                            "No scroll movement measured. The pane may be at its boundary; choose another container or focused PageDown/PageUp. Do not repeat unchanged scrolling."
                        );
                    }
                }
                observed["next"] = json!(
                    "Use this fresh snapshot for the next action; inspect again only if more information is needed."
                );
                Ok(observed)
            }
            Err(error) => Ok(
                json!({"performed":true,"observation_error":error.to_string(),"next":"Action was sent. Inspect to verify; do not repeat the action blindly."}),
            ),
        }
    }
}
// UIA providers can round fractional layout positions differently by a pixel.
// Identity/type/name/ancestry checks still apply; screenshot geometry stays exact.
fn stable_control_bounds(current: [i32; 4], observed: [i32; 4]) -> bool {
    current
        .into_iter()
        .zip(observed)
        .all(|(a, b)| a.abs_diff(b) <= 2)
}
#[cfg(test)]
mod control_bounds_tests {
    #[test]
    fn fractional_layout_rounding_is_not_a_moved_target() {
        assert!(super::stable_control_bounds(
            [785, 637, 882, 694],
            [784, 637, 880, 693]
        ));
        assert!(!super::stable_control_bounds(
            [805, 637, 902, 694],
            [784, 637, 880, 693]
        ));
        assert!(!super::stable_control_bounds(
            [i32::MIN, 0, 1, 1],
            [i32::MAX, 0, 1, 1]
        ));
    }
}

unsafe fn walk(
    node: &IUIAutomationElement,
    walker: &IUIAutomationTreeWalker,
    depth: usize,
    nodes: &mut Vec<IUIAutomationElement>,
    rows: &mut Vec<Value>,
    start: Instant,
    owner: u32,
    page_start: usize,
    limit: usize,
    text_offset: i32,
) {
    unsafe {
        if nodes.len() >= limit || depth > 32 || start.elapsed() > Duration::from_millis(1200) {
            return;
        }
        if node
            .CurrentIsPassword()
            .map(|v| v.as_bool())
            .unwrap_or(true)
            || node.CurrentProcessId().ok() == Some(owner as i32)
        {
            return;
        }
        let id = nodes.len();
        nodes.push(node.clone());
        if id < page_start {
            rows.push(Value::Null);
        } else {
            let name = node
                .CurrentName()
                .map(|v| v.to_string())
                .unwrap_or_default();
            let automation_id = node
                .CurrentAutomationId()
                .map(|v| v.to_string())
                .unwrap_or_default();
            let control_type = node.CurrentControlType().map(|v| v.0).unwrap_or(0);
            let kind = node
                .CurrentLocalizedControlType()
                .map(|v| v.to_string())
                .unwrap_or_default();
            let rect = node.CurrentBoundingRectangle().unwrap_or_default();
            let value = node
                .GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
                .and_then(|p| p.CurrentValue())
                .map(|v| v.to_string())
                .unwrap_or_default();
            let text = if depth == 0 || control_type == UIA_DocumentControlTypeId.0 {
                node.GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId)
                    .and_then(|p| p.DocumentRange())
                    .and_then(|r| {
                        if text_offset > 0 {
                            r.MoveEndpointByUnit(
                                TextPatternRangeEndpoint_Start,
                                TextUnit_Character,
                                text_offset,
                            )?;
                        }
                        r.GetText(1600)
                    })
                    .map(|v| v.to_string())
                    .unwrap_or_default()
            } else {
                String::new()
            };
            let value = if !text.is_empty() {
                String::new()
            } else {
                value
            };
            rows.push(json!({"id":id,"automation_id":automation_id,"control_type":control_type,"role":kind.chars().take(80).collect::<String>(),"name":name.chars().take(240).collect::<String>(),"value":value.chars().take(500).collect::<String>(),"text":&text,"bounds":[rect.left,rect.top,rect.right,rect.bottom]}));
            if text.chars().count() >= 1600 {
                rows.last_mut().unwrap()["text_truncated"] = json!(true);
                rows.last_mut().unwrap()["next_text_offset"] = json!(text_offset + 1600);
            }
            if node
                .CurrentHasKeyboardFocus()
                .map(|v| v.as_bool())
                .unwrap_or(false)
            {
                rows.last_mut().unwrap()["focused"] = json!(true);
            }
            if let Ok(state) = node
                .GetCurrentPatternAs::<IUIAutomationTogglePattern>(UIA_TogglePatternId)
                .and_then(|p| p.CurrentToggleState())
            {
                rows.last_mut().unwrap()["toggle_state"] = json!(match state.0 {
                    0 => "off",
                    1 => "on",
                    _ => "indeterminate",
                });
            }

            if let Ok(pattern) =
                node.GetCurrentPatternAs::<IUIAutomationScrollPattern>(UIA_ScrollPatternId)
            {
                if pattern
                    .CurrentVerticallyScrollable()
                    .map(|v| v.as_bool())
                    .unwrap_or(false)
                {
                    rows.last_mut().unwrap()["scroll_percent"] =
                        json!(pattern.CurrentVerticalScrollPercent().ok());
                    rows.last_mut().unwrap()["scrollable"] = json!(true);
                }
            }
        }
        let mut child = walker.GetFirstChildElement(node);
        let mut siblings = 0;
        while let Ok(element) = child {
            if nodes.len() >= limit
                || siblings >= 4096
                || start.elapsed() > Duration::from_millis(1200)
            {
                break;
            }
            walk(
                &element,
                walker,
                depth + 1,
                nodes,
                rows,
                start,
                owner,
                page_start,
                limit,
                text_offset,
            );
            siblings += 1;
            child = walker.GetNextSiblingElement(&element);
        }
    }
}
unsafe fn scroll_container(
    automation: &IUIAutomation,
    target: Option<&IUIAutomationElement>,
    root: &IUIAutomationElement,
    owner: u32,
) -> Result<Option<(IUIAutomationElement, IUIAutomationScrollPattern)>> {
    unsafe {
        let walker = automation.ControlViewWalker()?;
        let mut current = target.unwrap_or(root).clone();
        for _ in 0..64 {
            if let Ok(p) =
                current.GetCurrentPatternAs::<IUIAutomationScrollPattern>(UIA_ScrollPatternId)
            {
                if p.CurrentVerticallyScrollable()?.as_bool() {
                    return Ok(Some((current, p)));
                }
            }
            if automation.CompareElements(&current, root)?.as_bool() {
                break;
            }
            current = walker.GetParentElement(&current)?;
            ensure!(
                is_descendant(automation, &current, root)?,
                "Scroll target left selected window"
            );
        }
        // With no explicit pane, prefer the largest visible scrollable descendant.
        if target.is_none() {
            let condition = automation.CreatePropertyCondition(
                UIA_IsScrollPatternAvailablePropertyId,
                &windows::Win32::System::Variant::VARIANT::from(true),
            )?;
            let candidates = root.FindAll(TreeScope_Descendants, &condition)?;
            let mut best = None;
            let mut largest = 0i64;
            for i in 0..candidates.Length()?.min(128) {
                let n = candidates.GetElement(i)?;
                if n.CurrentProcessId()? as u32 == owner
                    || n.CurrentProcessId()? as u32 == std::process::id()
                    || n.CurrentIsPassword()?.as_bool()
                    || n.CurrentIsOffscreen()?.as_bool()
                    || !n.CurrentIsEnabled()?.as_bool()
                {
                    continue;
                }
                let p = n.GetCurrentPatternAs::<IUIAutomationScrollPattern>(UIA_ScrollPatternId)?;
                if !p.CurrentVerticallyScrollable()?.as_bool() {
                    continue;
                }
                let r = n.CurrentBoundingRectangle()?;
                let area = (r.right - r.left).max(0) as i64 * (r.bottom - r.top).max(0) as i64;
                if area > largest {
                    largest = area;
                    best = Some((n, p));
                }
            }
            return Ok(best);
        }
        Ok(None)
    }
}
unsafe fn is_descendant(
    automation: &IUIAutomation,
    node: &IUIAutomationElement,
    root: &IUIAutomationElement,
) -> Result<bool> {
    unsafe {
        let walker = automation.ControlViewWalker()?;
        let mut current = node.clone();
        for _ in 0..64 {
            if automation.CompareElements(&current, root)?.as_bool() {
                return Ok(true);
            }
            let Ok(parent) = walker.GetParentElement(&current) else {
                return Ok(false);
            };
            current = parent;
        }
        Ok(false)
    }
}
unsafe fn focus_element(
    automation: &IUIAutomation,
    node: &IUIAutomationElement,
    root: &IUIAutomationElement,
    owner: u32,
) -> Result<()> {
    unsafe {
        ensure!(
            !node.CurrentIsPassword()?.as_bool()
                && node.CurrentProcessId()? as u32 != owner
                && is_descendant(automation, node, root)?,
            "Protected or unrelated editor; no input sent"
        );
        let current = automation.GetFocusedElement()?;
        if !automation.CompareElements(node, &current)?.as_bool() {
            node.SetFocus()?;
        }
        let focused = automation.GetFocusedElement()?;
        ensure!(
            automation.CompareElements(node, &focused)?.as_bool()
                && !focused.CurrentIsPassword()?.as_bool(),
            "Could not verify focus on the intended editor; no input sent"
        );
        Ok(())
    }
}
// Prefer an atomic provider edit when both the document and selection can be
// read unambiguously. Never guess a caret or replace unrelated document text.
unsafe fn insert_text(
    text: &str,
    automation: &IUIAutomation,
    node: &IUIAutomationElement,
    hwnd: HWND,
) -> Result<()> {
    unsafe {
        let expected = (|| -> Result<(String, String)> {
            let pattern =
                node.GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId)?;
            let document = pattern.DocumentRange()?;
            let current = document.GetText(1_000_001)?.to_string();
            ensure!(
                current.encode_utf16().count() <= 1_000_000,
                "Editor too large"
            );
            let selections = pattern.GetSelection()?;
            ensure!(selections.Length()? == 1, "Ambiguous selection");
            let selection = selections.GetElement(0)?;
            let prefix = document.Clone()?;
            prefix.MoveEndpointByRange(
                TextPatternRangeEndpoint_End,
                &selection,
                TextPatternRangeEndpoint_Start,
            )?;
            let suffix = document.Clone()?;
            suffix.MoveEndpointByRange(
                TextPatternRangeEndpoint_Start,
                &selection,
                TextPatternRangeEndpoint_End,
            )?;
            let before = prefix.GetText(-1)?.to_string();
            let selected = selection.GetText(-1)?.to_string();
            let after = suffix.GetText(-1)?.to_string();
            ensure!(
                format!("{before}{selected}{after}") == current,
                "Selection does not match document"
            );
            Ok((current, format!("{before}{text}{after}")))
        })()
        .ok();
        if let Some((current, expected)) = &expected {
            if let Ok(value) =
                node.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
            {
                ensure!(!value.CurrentIsReadOnly()?.as_bool(), "Field is read-only");
                ensure!(
                    value.CurrentValue()?.to_string() == *current,
                    "Editor value and text observations disagree; inspect before changing text"
                );
                value.SetValue(&windows::core::BSTR::from(expected))?;
            } else {
                unicode_text(text, automation, node, hwnd)?;
            }
            // A successful dispatch is not proof that an asynchronous editor accepted it.
            let started = Instant::now();
            loop {
                let actual = node
                    .GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId)?
                    .DocumentRange()?
                    .GetText(1_000_001)?
                    .to_string();
                if actual.replace("\r\n", "\n") == expected.replace("\r\n", "\n") {
                    break;
                }
                ensure!(
                    started.elapsed() < Duration::from_millis(700),
                    "Text verification failed after input; do not repeat or rewrite the document blindly. Inspect the existing text and repair only the affected selection"
                );
                std::thread::sleep(Duration::from_millis(30));
            }
        } else {
            unicode_text(text, automation, node, hwnd)?;
        }
        Ok(())
    }
}
// Input is asynchronous in WinUI editors. Small batches keep focus checked and
// avoid restoring the owner while a large stream is still being consumed.
unsafe fn unicode_text(
    text: &str,
    automation: &IUIAutomation,
    node: &IUIAutomationElement,
    hwnd: HWND,
) -> Result<()> {
    unsafe {
        ensure!(
            !text.contains('\0'),
            "NUL is not supported in keyboard text"
        );
        let text_pattern = node
            .GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId)
            .ok();
        let mut input = Vec::new();
        let mut characters = text.chars().peekable();
        while let Some(character) = characters.next() {
            let before = text_pattern
                .as_ref()
                .and_then(|p| p.DocumentRange().ok()?.GetText(1_000_001).ok())
                .map(|s| s.to_string());
            ensure!(character != '\0', "NUL is not supported in keyboard text");
            ensure!(
                !input.is_empty()
                    || (GetForegroundWindow() == hwnd
                        && automation
                            .CompareElements(node, &automation.GetFocusedElement()?)?
                            .as_bool()),
                "Focus changed during typing; input stopped. Inspect existing text before retrying"
            );
            if character == '\r' || character == '\n' {
                if !input.is_empty() {
                    send(&input)?;
                    input.clear();
                    std::thread::sleep(Duration::from_millis(12));
                }
                if character == '\r' && characters.peek() == Some(&'\n') {
                    characters.next();
                }
                keys(&[VK_RETURN.0])?;
                std::thread::sleep(Duration::from_millis(20));
            } else {
                let mut units = [0u16; 2];
                for unit in character.encode_utf16(&mut units) {
                    input.push(key_input(0, *unit, KEYEVENTF_UNICODE));
                    input.push(key_input(0, *unit, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP));
                }
                if text_pattern.is_some() || input.len() >= 16 {
                    send(&input)?;
                    input.clear();
                    std::thread::sleep(Duration::from_millis(12));
                }
            }
            if let (Some(pattern), Some(before)) = (&text_pattern, before) {
                let started = Instant::now();
                loop {
                    let actual = pattern.DocumentRange()?.GetText(1_000_001)?.to_string();
                    if actual != before {
                        break;
                    }
                    ensure!(
                        started.elapsed() < Duration::from_millis(700),
                        "Editor did not acknowledge the last character; stopped before sending more text. Inspect existing text before retrying"
                    );
                    std::thread::sleep(Duration::from_millis(8));
                }
            }
        }
        if !input.is_empty() {
            send(&input)?;
        }
        std::thread::sleep(Duration::from_millis(100));
        Ok(())
    }
}
unsafe fn foreground(hwnd: HWND, owner: u32) -> Result<ForegroundGuard> {
    unsafe {
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindowAsync(hwnd, SW_RESTORE);
        }
        let _ = SetForegroundWindow(hwnd);
        let start = Instant::now();
        while GetForegroundWindow() != hwnd && start.elapsed() < Duration::from_millis(350) {
            std::thread::sleep(Duration::from_millis(10));
        }
        ensure!(
            GetForegroundWindow() == hwnd,
            "Windows blocked foreground input. Bring the target window to the front and retry"
        );
        ensure!(
            [VK_CONTROL, VK_SHIFT, VK_MENU, VK_LWIN, VK_RWIN]
                .iter()
                .all(|v| GetAsyncKeyState(v.0 as i32) >= 0),
            "Release modifier keys before allowing input"
        );
        // Transfer focus before hiding the parent; hiding the foreground owner
        // first can invalidate a freshly delegated foreground permission.
        Ok(ForegroundGuard::hide(owner))
    }
}
unsafe fn move_cursor(hwnd: HWND, p: POINT) -> Result<()> {
    unsafe {
        let mut rect = RECT::default();
        GetWindowRect(hwnd, &mut rect)?;
        ensure!(
            p.x >= rect.left && p.x < rect.right && p.y >= rect.top && p.y < rect.bottom,
            "Point outside target window"
        );
        // Prevent an overlapping approval/security/other window from receiving the click.
        ensure!(
            GetAncestor(WindowFromPoint(p), GA_ROOT) == hwnd,
            "Target is covered by another window; inspect again"
        );
        let x = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let y = GetSystemMetrics(SM_YVIRTUALSCREEN);
        let width = GetSystemMetrics(SM_CXVIRTUALSCREEN);
        let height = GetSystemMetrics(SM_CYVIRTUALSCREEN);
        ensure!(width > 1 && height > 1, "No interactive desktop");
        send(&[mouse_input(
            ((p.x - x) as i64 * 65535 / (width - 1) as i64) as i32,
            ((p.y - y) as i64 * 65535 / (height - 1) as i64) as i32,
            0,
            MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
        )])
    }
}
unsafe fn click(automation: &IUIAutomation, hwnd: HWND, p: POINT) -> Result<()> {
    unsafe {
        let element = automation.ElementFromPoint(p)?;
        ensure!(
            !element.CurrentIsPassword()?.as_bool(),
            "Password fields require manual input"
        );
        move_cursor(hwnd, p)?;
        send(&[
            mouse_input(0, 0, 0, MOUSEEVENTF_LEFTDOWN),
            mouse_input(0, 0, 0, MOUSEEVENTF_LEFTUP),
        ])
    }
}
fn mouse_input(x: i32, y: i32, data: u32, flags: MOUSE_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: x,
                dy: y,
                mouseData: data,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}
fn key_input(key: u16, scan: u16, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(key),
                wScan: scan,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}
unsafe fn keys(codes: &[u16]) -> Result<()> {
    unsafe {
        let mut input = Vec::new();
        for key in codes {
            input.push(key_input(*key, 0, KEYBD_EVENT_FLAGS(0)));
        }
        for key in codes.iter().rev() {
            input.push(key_input(*key, 0, KEYEVENTF_KEYUP));
        }
        let result = send(&input);
        // Always release injected keys even if Windows accepted only part of the batch.
        let releases = codes
            .iter()
            .rev()
            .map(|v| key_input(*v, 0, KEYEVENTF_KEYUP))
            .collect::<Vec<_>>();
        if result.is_err() {
            let _ = SendInput(&releases, std::mem::size_of::<INPUT>() as i32);
        }
        result
    }
}
unsafe fn send(input: &[INPUT]) -> Result<()> {
    unsafe {
        ensure!(
            SendInput(input, std::mem::size_of::<INPUT>() as i32) == input.len() as u32,
            "Windows blocked input (possibly an elevated target)"
        );
        Ok(())
    }
}

unsafe fn native_control(
    node: &IUIAutomationElement,
    root: HWND,
    classes: &[&str],
) -> Option<HWND> {
    unsafe {
        let hwnd = node.CurrentNativeWindowHandle().ok()?;
        if hwnd.is_invalid() || GetAncestor(hwnd, GA_ROOT) != root {
            return None;
        }
        let mut class = [0u16; 128];
        let length = GetClassNameW(hwnd, &mut class);
        if length <= 0 {
            return None;
        }
        let class = String::from_utf16_lossy(&class[..length as usize]).to_ascii_lowercase();
        classes.contains(&class.as_str()).then_some(hwnd)
    }
}
fn fast_capture(hwnd: HWND) -> Result<image::DynamicImage> {
    unsafe {
        use windows::Win32::{
            Graphics::Gdi::*,
            Storage::Xps::{PRINT_WINDOW_FLAGS, PrintWindow},
        };
        let mut rect = RECT::default();
        GetWindowRect(hwnd, &mut rect)?;
        let width = (rect.right - rect.left).max(0) as u32;
        let height = (rect.bottom - rect.top).max(0) as u32;
        ensure!(
            width > 0 && height > 0 && width as u64 * height as u64 <= 32_000_000,
            "Invalid window bounds"
        );
        let screen = GetDC(None);
        ensure!(!screen.is_invalid(), "No display context");
        let memory = CreateCompatibleDC(Some(screen));
        if memory.is_invalid() {
            ReleaseDC(None, screen);
            anyhow::bail!("No capture context");
        }
        let bitmap = CreateCompatibleBitmap(screen, width as i32, height as i32);
        if bitmap.is_invalid() {
            let _ = DeleteDC(memory);
            ReleaseDC(None, screen);
            anyhow::bail!("No capture bitmap");
        }
        let old = SelectObject(memory, HGDIOBJ(bitmap.0));
        let printed = PrintWindow(hwnd, memory, PRINT_WINDOW_FLAGS(PW_RENDERFULLCONTENT)).as_bool();
        SelectObject(memory, old);
        let mut pixels = vec![0u8; width as usize * height as usize * 4];
        let mut info = BITMAPINFO::default();
        info.bmiHeader = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width as i32,
            biHeight: -(height as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        };
        let read = if printed {
            GetDIBits(
                memory,
                bitmap,
                0,
                height,
                Some(pixels.as_mut_ptr().cast()),
                &mut info,
                DIB_RGB_COLORS,
            )
        } else {
            0
        };
        let _ = DeleteObject(HGDIOBJ(bitmap.0));
        let _ = DeleteDC(memory);
        ReleaseDC(None, screen);
        ensure!(
            read == height as i32
                && pixels
                    .chunks_exact(4)
                    .any(|p| p[0] > 8 || p[1] > 8 || p[2] > 8),
            "Window does not support fast capture"
        );
        for p in pixels.chunks_exact_mut(4) {
            p.swap(0, 2);
            p[3] = 255;
        }
        Ok(image::DynamicImage::ImageRgba8(
            image::RgbaImage::from_raw(width, height, pixels).context("Invalid captured pixels")?,
        ))
    }
}

struct ForegroundGuard {
    hidden: Vec<HWND>,
    owner: u32,
}
impl ForegroundGuard {
    unsafe fn hide(owner: u32) -> Self {
        unsafe {
            struct Own {
                owner: u32,
                windows: Vec<HWND>,
            }
            unsafe extern "system" fn collect(hwnd: HWND, param: LPARAM) -> BOOL {
                unsafe {
                    let own = &mut *(param.0 as *mut Own);
                    let mut pid = 0;
                    GetWindowThreadProcessId(hwnd, Some(&mut pid));
                    if pid == own.owner
                        && IsWindowVisible(hwnd).as_bool()
                        && !IsIconic(hwnd).as_bool()
                        && own.windows.len() < 8
                    {
                        own.windows.push(hwnd);
                    }
                    BOOL(1)
                }
            }
            let mut own = Own {
                owner,
                windows: vec![],
            };
            let _ = EnumWindows(Some(collect), LPARAM(&mut own as *mut _ as isize));
            for hwnd in &own.windows {
                let _ = ShowWindowAsync(*hwnd, SW_HIDE);
            }
            std::thread::sleep(Duration::from_millis(40));
            Self {
                hidden: own.windows,
                owner,
            }
        }
    }
}
impl Drop for ForegroundGuard {
    fn drop(&mut self) {
        unsafe {
            for hwnd in &self.hidden {
                let mut pid = 0;
                GetWindowThreadProcessId(*hwnd, Some(&mut pid));
                if pid == self.owner {
                    let _ = ShowWindowAsync(*hwnd, SW_SHOWNOACTIVATE);
                }
            }
        }
    }
}

// A window screenshot is scaled independently of the virtual desktop layout/DPI.
fn screenshot_point(rect: RECT, size: [u32; 2], x: f64, y: f64) -> Result<POINT> {
    ensure!(
        size[0] > 0 && size[1] > 0 && rect.right > rect.left && rect.bottom > rect.top,
        "Invalid screenshot bounds"
    );
    ensure!(
        x.is_finite()
            && y.is_finite()
            && x >= 0.0
            && y >= 0.0
            && x < size[0] as f64
            && y < size[1] as f64,
        "Coordinates outside screenshot"
    );
    Ok(POINT {
        x: rect.left + (x * (rect.right - rect.left) as f64 / size[0] as f64) as i32,
        y: rect.top + (y * (rect.bottom - rect.top) as f64 / size[1] as f64) as i32,
    })
}
#[cfg(test)]
mod regression_tests {
    use super::*;
    #[test]
    fn window_image_pixels_map_to_negative_monitor_origins_and_scaled_windows() {
        let p = screenshot_point(
            RECT {
                left: -2560,
                top: -1080,
                right: 0,
                bottom: 360,
            },
            [1600, 900],
            800.0,
            450.0,
        )
        .unwrap();
        assert_eq!((p.x, p.y), (-1280, -360));
        let p = screenshot_point(
            RECT {
                left: 1920,
                top: 200,
                right: 3120,
                bottom: 1100,
            },
            [800, 600],
            400.0,
            300.0,
        )
        .unwrap();
        assert_eq!((p.x, p.y), (2520, 650));
        assert!(
            screenshot_point(
                RECT {
                    left: 0,
                    top: 0,
                    right: 100,
                    bottom: 100
                },
                [100, 100],
                100.0,
                0.0
            )
            .is_err()
        );
        assert!(
            screenshot_point(
                RECT {
                    left: 0,
                    top: 0,
                    right: 100,
                    bottom: 100
                },
                [100, 100],
                f64::NAN,
                0.0
            )
            .is_err()
        );
    }
    #[test]
    fn normal_browsers_are_available_but_security_and_terminals_remain_blocked() {
        for exe in ["msedge.exe", "chrome.exe", "firefox.exe"] {
            assert!(!blocked(exe));
        }
        for exe in [
            "consent.exe",
            "powershell.exe",
            "vectordash.exe",
            "keepass.exe",
        ] {
            assert!(blocked(exe));
        }
    }
}
