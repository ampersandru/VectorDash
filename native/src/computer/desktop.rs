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
        UI::{Accessibility::*, Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
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
            return Ok(
                json!({"focused":true,"window_id":id,"next":"Inspect this window_id for fresh elements before interacting."}),
            );
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
            unsafe {
                walk(&root, &walker, 0, &mut nodes, &mut rows, start, owner);
            }
            let mut rect = RECT::default();
            unsafe {
                GetWindowRect(hwnd, &mut rect)?;
            }
            let nonce = format!("d{:016x}", rand::random::<u64>());
            let mut image_size = None;
            let mut use_dwm_bounds = false;
            let fingerprints = rows
                .iter()
                .map(|v| {
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
            let offset = args["offset"].as_u64().unwrap_or(0).min(160) as usize;
            let total = rows.len();
            let rows = rows.into_iter().skip(offset).take(60).collect::<Vec<_>>();
            let mut value = json!({"offset":offset,"total_elements":total,"next_offset":(offset+rows.len()).min(total),"window_id":id,"snapshot_id":nonce,"title":window.title,"elements":rows,"truncated":nodes.len()>=160||start.elapsed()>Duration::from_millis(1200),"coordinate_space":"Screenshot pixels; window must not move before clicking","elapsed_ms":start.elapsed().as_millis()});
            if action == "screenshot" {
                if let Ok(image) = fast_capture(hwnd) {
                    let resized = image.thumbnail(1600, 1600);
                    image_size = Some([resized.width(), resized.height()]);
                    let bytes = crate::content::jpeg(&image, 1600)?;
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
                    let image = image::load_from_memory(&bytes)?.thumbnail(1600, 1600);
                    image_size = Some([image.width(), image.height()]);
                    value["image_width"] = json!(image.width());
                    value["image_height"] = json!(image.height());
                    value["image_url"] = json!(crate::capture::vision(&bytes, None)?);
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
            value["window_bounds"] = json!([rect.left, rect.top, rect.right, rect.bottom]);
            value["element_bounds_space"] = json!("Physical desktop pixels");
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
                Some(
                    root.FindFirst(TreeScope_Subtree, &condition)
                        .context("Element no longer exists; inspect again")?,
                )
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
                        && [rect.left, rect.top, rect.right, rect.bottom] == fingerprint.1,
                    "Element changed since observation; inspect again"
                );
                let root = automation.ElementFromHandle(hwnd)?;
                let walker = automation.ControlViewWalker()?;
                let mut ancestor = node.clone();
                let mut belongs = false;
                for _ in 0..24 {
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
                    if let Some(child) =
                        native_control(node, hwnd, &["edit", "richedit20w", "richedit50w"])
                    {
                        ensure!(
                            GetWindowLongPtrW(child, GWL_STYLE) & 0x0800 == 0,
                            "Field is read-only"
                        );
                        let mut result = 0usize;
                        let value = wide(text);
                        ensure!(
                            SendMessageTimeoutW(
                                child,
                                WM_SETTEXT,
                                windows::Win32::Foundation::WPARAM(0),
                                LPARAM(value.as_ptr() as isize),
                                SMTO_ABORTIFHUNG | SMTO_BLOCK,
                                500,
                                Some(&mut result)
                            )
                            .0 != 0
                                && result != 0,
                            "Text field did not respond"
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
                        node.SetFocus()?;
                        let focused = automation.GetFocusedElement()?;
                        ensure!(
                            automation.CompareElements(node, &focused)?.as_bool()
                                && !focused.CurrentIsPassword()?.as_bool(),
                            "Could not focus the selected field"
                        );
                        keys(&super::key_codes("Ctrl+A")?)?;
                        let mut input = Vec::new();
                        for unit in text.encode_utf16() {
                            input.push(key_input(0, unit, KEYEVENTF_UNICODE));
                            input.push(key_input(0, unit, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP));
                        }
                        send(&input)?;
                    }
                }
                "key" => {
                    ensure!(
                        !window.background_only,
                        "Hosted app supports background UIA actions only; choose its visible host for keyboard input."
                    );
                    let _visibility = foreground(hwnd, owner)?;
                    let focused = automation.GetFocusedElement()?;
                    ensure!(
                        !focused.CurrentIsPassword()?.as_bool()
                            && focused.CurrentProcessId()? as u32 != owner,
                        "Password/protected controls require manual input"
                    );
                    keys(&super::key_codes(args["keys"].as_str().unwrap_or(""))?)?;
                }
                "scroll" => {
                    let root = automation.ElementFromHandle(hwnd)?;
                    let node = node.unwrap_or(&root);
                    if let Ok(pattern) =
                        node.GetCurrentPatternAs::<IUIAutomationScrollPattern>(UIA_ScrollPatternId)
                    {
                        pattern.Scroll(
                            ScrollAmount_NoAmount,
                            if args["delta"].as_i64().unwrap_or(0) > 0 {
                                ScrollAmount_LargeIncrement
                            } else {
                                ScrollAmount_LargeDecrement
                            },
                        )?;
                    } else {
                        let _visibility = foreground(hwnd, owner)?;
                        let rect = node.CurrentBoundingRectangle()?;
                        let point = POINT {
                            x: (rect.left + rect.right) / 2,
                            y: (rect.top + rect.bottom) / 2,
                        };
                        move_cursor(hwnd, point)?;
                        send(&[mouse_input(
                            0,
                            0,
                            (-(args["delta"].as_i64().unwrap_or(120)) as i32) as u32,
                            MOUSEEVENTF_WHEEL,
                        )])?;
                    }
                }
                _ => anyhow::bail!("Unsupported computer action"),
            }
        }
        self.snapshot = None;
        Ok(
            json!({"performed":true,"next":"Inspect the window again to verify the result before another action."}),
        )
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
) {
    unsafe {
        if nodes.len() >= 160 || depth > 8 || start.elapsed() > Duration::from_millis(1200) {
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
        let text = if depth == 0 {
            node.GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId)
                .and_then(|p| p.DocumentRange())
                .and_then(|r| r.GetText(6000))
                .map(|v| v.to_string())
                .unwrap_or_default()
        } else {
            String::new()
        };
        nodes.push(node.clone());
        rows.push(json!({"id":id,"automation_id":automation_id,"control_type":control_type,"role":kind.chars().take(80).collect::<String>(),"name":name.chars().take(240).collect::<String>(),"value":value.chars().take(500).collect::<String>(),"text":text,"bounds":[rect.left,rect.top,rect.right,rect.bottom]}));
        let mut child = walker.GetFirstChildElement(node);
        let mut siblings = 0;
        while let Ok(element) = child {
            if nodes.len() >= 160
                || siblings >= 160
                || start.elapsed() > Duration::from_millis(1200)
            {
                break;
            }
            walk(&element, walker, depth + 1, nodes, rows, start, owner);
            siblings += 1;
            child = walker.GetNextSiblingElement(&element);
        }
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
