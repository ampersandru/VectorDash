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
    title: String,
}
struct Snapshot {
    id: String,
    window: Window,
    rect: RECT,
    nodes: Vec<IUIAutomationElement>,
    image_size: Option<[u32; 2]>,
    fingerprints: Vec<(String, [i32; 4])>,
    use_dwm_bounds: bool,
    at: Instant,
}
#[derive(Default)]
pub struct Desktop {
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
            | "msedge.exe"
            | "chrome.exe"
            | "firefox.exe"
            | "brave.exe"
            | "vivaldi.exe"
            | "opera.exe"
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
        if state.windows.len() >= 64 {
            return BOOL(0);
        }
        if !IsWindowVisible(hwnd).as_bool() || IsIconic(hwnd).as_bool() {
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
        let action = args["action"].as_str().unwrap_or("");
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
                let id = format!("w{:016x}", rand::random::<u64>());
                windows.push(json!({"window_id":id,"title":window.title,"application":std::path::Path::new(&window.executable).file_name().unwrap_or_default().to_string_lossy()}));
                self.windows.insert(id, window);
            }
            return Ok(
                json!({"windows":windows,"note":"Protected/VectorDash/known terminal windows are excluded. Elevated windows may be inaccessible. Inspect a window_id before acting."}),
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
        let automation: IUIAutomation =
            unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)? };
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
                nodes,
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
        let node = args["element"]
            .as_u64()
            .and_then(|n| snapshot.nodes.get(n as usize));
        if let Some(node) = node {
            unsafe {
                let index = args["element"].as_u64().unwrap_or(0) as usize;
                let fingerprint = snapshot
                    .fingerprints
                    .get(index)
                    .context("Unknown element")?;
                let name = node
                    .CurrentName()?
                    .to_string()
                    .chars()
                    .take(240)
                    .collect::<String>();
                let rect = node.CurrentBoundingRectangle()?;
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
                    if automation.CompareElements(&ancestor, &root)?.as_bool() {
                        belongs = true;
                        break;
                    }
                    ancestor = walker.GetParentElement(&ancestor)?;
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
                            pattern.Invoke()?;
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
                        click(
                            &automation,
                            hwnd,
                            POINT {
                                x: current.left
                                    + (x * (current.right - current.left) as f64 / size[0] as f64)
                                        as i32,
                                y: current.top
                                    + (y * (current.bottom - current.top) as f64 / size[1] as f64)
                                        as i32,
                            },
                        )?;
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
        rows.push(json!({"id":id,"role":kind.chars().take(80).collect::<String>(),"name":name.chars().take(240).collect::<String>(),"value":value.chars().take(500).collect::<String>(),"text":text,"bounds":[rect.left,rect.top,rect.right,rect.bottom]}));
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
        let visibility = ForegroundGuard::hide(owner);
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
        Ok(visibility)
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
