use crate::{config::Settings, db::Hit};
use anyhow::{Context, Result};
use std::{
    path::Path,
    process::{Child, Command},
    time::{Duration, Instant},
};
pub struct Job(#[cfg(windows)] windows::Win32::Foundation::HANDLE);
#[cfg(windows)]
impl Drop for Job {
    fn drop(&mut self) {
        unsafe {
            let _ = windows::Win32::Foundation::CloseHandle(self.0);
        }
    }
}
// Windows job handles are kernel objects; they can be closed from any thread.
unsafe impl Send for Job {}
#[cfg(windows)]
pub fn owned_process(child: &Child) -> Result<Job> {
    use windows::Win32::{
        Foundation::CloseHandle,
        System::{
            JobObjects::*,
            Threading::{OpenProcess, PROCESS_SET_QUOTA, PROCESS_TERMINATE},
        },
    };
    unsafe {
        let handle = CreateJobObjectW(None, None)?;
        let job = Job(handle);
        let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        SetInformationJobObject(
            handle,
            JobObjectExtendedLimitInformation,
            &info as *const _ as *const _,
            std::mem::size_of_val(&info) as u32,
        )?;
        let process = OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, false, child.id())?;
        let result = AssignProcessToJobObject(handle, process);
        let _ = CloseHandle(process);
        result?;
        Ok(job)
    }
}
#[cfg(not(windows))]
pub fn owned_process(_: &Child) -> Result<Job> {
    Ok(Job())
}
#[cfg(windows)]
pub fn limit_worker(child: &Child) -> Result<Job> {
    use windows::Win32::{
        Foundation::CloseHandle,
        System::{
            JobObjects::*,
            Threading::{OpenProcess, PROCESS_SET_QUOTA, PROCESS_TERMINATE},
        },
    };
    unsafe {
        let job = CreateJobObjectW(None, None)?;
        let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
            | JOB_OBJECT_LIMIT_PROCESS_MEMORY
            | JOB_OBJECT_LIMIT_ACTIVE_PROCESS;
        info.ProcessMemoryLimit = 1024 * 1024 * 1024;
        info.BasicLimitInformation.ActiveProcessLimit = 4;
        SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &info as *const _ as *const _,
            std::mem::size_of_val(&info) as u32,
        )?;
        let process = OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, false, child.id())?;
        let result = AssignProcessToJobObject(job, process);
        let _ = CloseHandle(process);
        if result.is_err() {
            let _ = CloseHandle(job);
            anyhow::bail!("Could not isolate parser process");
        }
        Ok(Job(job))
    }
}
#[cfg(not(windows))]
pub fn limit_worker(_: &Child) -> Result<Job> {
    Ok(Job())
}
#[cfg(windows)]
pub fn open(path: &str) -> Result<()> {
    use windows::{
        Win32::{
            Foundation::HWND,
            UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL},
        },
        core::PCWSTR,
    };
    let file = wide(path);
    let verb = wide("open");
    unsafe {
        let result = ShellExecuteW(
            None,
            PCWSTR(verb.as_ptr()),
            PCWSTR(file.as_ptr()),
            None,
            None,
            SW_SHOWNORMAL,
        );
        anyhow::ensure!(
            result.0 as usize > 32,
            "Windows could not open the selected item"
        );
    }
    Ok(())
}
#[cfg(not(windows))]
pub fn open(_: &str) -> Result<()> {
    Ok(())
}
#[cfg(windows)]
pub fn reveal(path: &str) -> Result<()> {
    use windows::{
        Win32::{
            System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoTaskMemFree},
            UI::Shell::*,
        },
        core::PCWSTR,
    };
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let mut pidl = std::ptr::null_mut();
        SHParseDisplayName(PCWSTR(wide(path).as_ptr()), None, &mut pidl, 0, None)?;
        let result = SHOpenFolderAndSelectItems(pidl, None, 0);
        CoTaskMemFree(Some(pidl as *const _));
        result?;
        Ok(())
    }
}
#[cfg(not(windows))]
pub fn reveal(_: &str) -> Result<()> {
    Ok(())
}
pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
#[cfg(windows)]
pub fn properties(path: &str) -> Result<()> {
    use windows::{Win32::UI::Shell::*, core::PCWSTR};
    unsafe {
        let p = wide(path);
        let v = wide("properties");
        let mut info = SHELLEXECUTEINFOW {
            cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
            fMask: SEE_MASK_INVOKEIDLIST,
            lpVerb: PCWSTR(v.as_ptr()),
            lpFile: PCWSTR(p.as_ptr()),
            nShow: 1,
            ..Default::default()
        };
        ShellExecuteExW(&mut info)?;
    }
    Ok(())
}
#[cfg(not(windows))]
pub fn properties(_: &str) -> Result<()> {
    Ok(())
}
#[cfg(windows)]
pub fn recycle(path: &str) -> Result<()> {
    use windows::{Win32::UI::Shell::*, core::PCWSTR};
    let mut p = wide(path);
    p.push(0);
    let mut op = SHFILEOPSTRUCTW {
        wFunc: FO_DELETE,
        pFrom: PCWSTR(p.as_ptr()),
        fFlags: (FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_SILENT).0 as u16,
        ..Default::default()
    };
    unsafe {
        let code = SHFileOperationW(&mut op);
        anyhow::ensure!(
            code == 0 && !op.fAnyOperationsAborted.as_bool(),
            "Windows did not move the file to the Recycle Bin"
        );
    }
    Ok(())
}
#[cfg(not(windows))]
pub fn recycle(_: &str) -> Result<()> {
    Ok(())
}
pub fn remove(path: &str) -> Result<()> {
    let p = Path::new(path);
    if p.is_dir() {
        anyhow::bail!("Permanent deletion of folders is disabled; use the Recycle Bin");
    }
    std::fs::remove_file(p)?;
    Ok(())
}
pub fn actions(q: &str, s: &Settings) -> Vec<Hit> {
    let mut out = vec![];
    let query = q.to_lowercase();
    for (name, uri) in [
        ("bluetooth", "ms-settings:bluetooth"),
        ("sound", "ms-settings:sound"),
        ("display", "ms-settings:display"),
        ("wifi", "ms-settings:network-wifi"),
        ("windows update", "ms-settings:windowsupdate"),
        ("storage", "ms-settings:storagesense"),
        ("apps", "ms-settings:appsfeatures"),
        ("power", "ms-settings:powersleep"),
        ("privacy", "ms-settings:privacy"),
    ] {
        if name.contains(query.trim_start_matches("settings: ")) {
            out.push(Hit {
                path: uri.into(),
                kind: "settings".into(),
                text: name.into(),
                score: 1.8,
                reason: "Windows settings shortcut".into(),
                page: 0,
                seconds: -1.0,
                line: 0,
            });
        }
    }
    if s.include_programs {
        let all = PROGRAMS.get_or_init(scan_programs);
        for hit in all.iter() {
            if hit
                .text
                .to_lowercase()
                .contains(query.trim_start_matches("app: "))
            {
                out.push(hit.clone());
            }
        }
    }

    out
}
#[cfg(windows)]
pub fn hotkeys(
    tx: crate::engine::NotifySender,
    shutdown: std::sync::Arc<std::sync::atomic::AtomicBool>,
    s: Settings,
) {
    std::thread::spawn(move || unsafe {
        use windows::Win32::UI::{Input::KeyboardAndMouse::*, WindowsAndMessaging::*};
        let tray = tray_window(tx.clone()).ok();
        match shortcut(&s.hotkey) {
            Ok((mods, key)) => {
                if RegisterHotKey(None, 777, mods, key).is_err() {
                    let _=tx.try_send(crate::engine::Event::Log("Warning".into(),"Global shortcut is already in use. Change it in Settings; double Ctrl and tray still work.".into()));
                }
            }
            Err(e) => {
                let _ = tx.try_send(crate::engine::Event::Log("Warning".into(), e.to_string()));
            }
        }

        let mut message = MSG::default();
        let mut down = false;
        let mut last = Instant::now() - Duration::from_secs(1);
        while !shutdown.load(std::sync::atomic::Ordering::Relaxed) {
            while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                if message.hwnd.0 as usize != 0 {
                    let _ = TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
                if message.message == WM_HOTKEY {
                    let hwnd = GetForegroundWindow();
                    let _ = tx.send(crate::engine::Event::Summon(hwnd.0 as isize, String::new()));
                }
            }
            let pressed = GetAsyncKeyState(VK_CONTROL.0 as i32) < 0;
            if pressed && !down {
                if last.elapsed() < Duration::from_millis(350) {
                    let hwnd = GetForegroundWindow();
                    let mut class = [0u16; 128];
                    let n = GetClassNameW(hwnd, &mut class);
                    let name = String::from_utf16_lossy(&class[..n.max(0) as usize]);
                    if s.explorer_integration && name == "CabinetWClass"
                        || s.dialog_integration && name == "#32770"
                    {
                        let _ =
                            tx.send(crate::engine::Event::Summon(hwnd.0 as isize, String::new()));
                    }
                }
                last = Instant::now();
            }
            down = pressed;
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = UnregisterHotKey(None, 777);
        if let Some((hwnd, icon)) = tray {
            let _ = windows::Win32::UI::Shell::Shell_NotifyIconW(
                windows::Win32::UI::Shell::NIM_DELETE,
                &icon,
            );
            let _ = DestroyWindow(hwnd);
        }
    });
}
#[cfg(not(windows))]
pub fn hotkeys(
    _: crate::engine::NotifySender,
    _: std::sync::Arc<std::sync::atomic::AtomicBool>,
    _: Settings,
) {
}
#[cfg(windows)]
pub fn send_to_dialog(hwnd: isize, path: &str) -> Result<bool> {
    use windows::{
        Win32::{
            Foundation::{HWND, LPARAM, WPARAM},
            UI::WindowsAndMessaging::*,
        },
        core::PCWSTR,
    };
    unsafe {
        let target = HWND(hwnd as *mut _);
        let mut class = [0u16; 128];
        let n = GetClassNameW(target, &mut class);
        if String::from_utf16_lossy(&class[..n.max(0) as usize]) != "#32770" {
            return Ok(false);
        }
        let control = GetDlgItem(Some(target), 1148)
            .ok()
            .or_else(|| GetDlgItem(Some(target), 1152).ok())
            .context("This is not a supported file-name dialog. Use Copy path instead.")?;
        let mut name = [0u16; 64];
        let n = GetClassNameW(control, &mut name);
        let kind = String::from_utf16_lossy(&name[..n.max(0) as usize]);
        let edit = if kind == "ComboBoxEx32" {
            let value = SendMessageW(control, WM_USER + 7, Some(WPARAM(0)), Some(LPARAM(0)));
            anyhow::ensure!(value.0 != 0, "Dialog file-name field unavailable");
            HWND(value.0 as *mut _)
        } else if kind == "ComboBox" {
            FindWindowExW(Some(control), None, PCWSTR(wide("Edit").as_ptr()), None)?
        } else {
            anyhow::ensure!(kind == "Edit", "Custom file dialog; use Copy path");
            control
        };
        let text = wide(path);
        let sent = SendMessageTimeoutW(
            edit,
            WM_SETTEXT,
            WPARAM(0),
            LPARAM(text.as_ptr() as isize),
            SMTO_ABORTIFHUNG,
            1000,
            None,
        );
        anyhow::ensure!(
            sent.0 != 0,
            "Dialog rejected the path (possibly elevated). Use Copy path instead."
        );
        let _ = SetForegroundWindow(target);
        Ok(true)
    }
}
#[cfg(not(windows))]
pub fn send_to_dialog(_: isize, _: &str) -> Result<bool> {
    Ok(false)
}
pub fn local_ip() -> String {
    std::net::UdpSocket::bind("0.0.0.0:0")
        .and_then(|s| {
            s.connect("192.0.2.1:80")?;
            s.local_addr()
        })
        .map(|a| a.ip().to_string())
        .unwrap_or_else(|_| "127.0.0.1".into())
}
#[cfg(windows)]
pub fn copy_file(path: &str) -> Result<()> {
    use windows::Win32::{
        Foundation::{GlobalFree, HANDLE},
        System::{
            DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData},
            Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock},
        },
    };
    unsafe {
        let mut words = wide(path);
        words.push(0);
        let size = 20 + words.len() * 2;
        let memory = GlobalAlloc(GMEM_MOVEABLE, size)?;
        let ptr = GlobalLock(memory) as *mut u8;
        if ptr.is_null() {
            let _ = GlobalFree(Some(memory));
            anyhow::bail!("Clipboard allocation failed");
        }
        std::ptr::write_bytes(ptr, 0, size);
        std::ptr::copy_nonoverlapping(20u32.to_le_bytes().as_ptr(), ptr, 4);
        std::ptr::copy_nonoverlapping(1u32.to_le_bytes().as_ptr(), ptr.add(16), 4);
        std::ptr::copy_nonoverlapping(words.as_ptr() as *const u8, ptr.add(20), words.len() * 2);
        let _ = GlobalUnlock(memory);
        if let Err(e) = OpenClipboard(None) {
            let _ = GlobalFree(Some(memory));
            return Err(e.into());
        }
        let result = (|| -> Result<()> {
            EmptyClipboard()?;
            SetClipboardData(15, Some(HANDLE(memory.0)))?;
            Ok(())
        })();
        let _ = CloseClipboard();
        if result.is_err() {
            let _ = GlobalFree(Some(memory));
        }
        result
    }
}
#[cfg(not(windows))]
pub fn copy_file(_: &str) -> Result<()> {
    Ok(())
}
#[cfg(windows)]
pub fn startup(enabled: bool) -> Result<()> {
    use windows::{Win32::System::Registry::*, core::PCWSTR};
    unsafe {
        let mut key = HKEY::default();
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(wide("Software\\Microsoft\\Windows\\CurrentVersion\\Run").as_ptr()),
            None,
            None,
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            None,
            &mut key,
            None,
        )
        .ok()?;
        let name = wide("VectorDashRust");
        let result = if enabled {
            let command = format!("\"{}\" --startup", std::env::current_exe()?.display());
            let value = wide(&command);
            RegSetValueExW(
                key,
                PCWSTR(name.as_ptr()),
                None,
                REG_SZ,
                Some(std::slice::from_raw_parts(
                    value.as_ptr() as *const u8,
                    value.len() * 2,
                )),
            )
            .ok()
        } else {
            let _ = RegDeleteValueW(key, PCWSTR(name.as_ptr()));
            Ok(())
        };
        let _ = RegCloseKey(key);
        result?;
        Ok(())
    }
}
#[cfg(not(windows))]
pub fn startup(_: bool) -> Result<()> {
    Ok(())
}
#[cfg(windows)]
pub fn idle_seconds() -> u64 {
    use windows::Win32::{
        System::SystemInformation::GetTickCount,
        UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO},
    };
    unsafe {
        let mut info = LASTINPUTINFO {
            cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
            ..Default::default()
        };
        if GetLastInputInfo(&mut info).as_bool() {
            GetTickCount().wrapping_sub(info.dwTime) as u64 / 1000
        } else {
            0
        }
    }
}
#[cfg(not(windows))]
pub fn idle_seconds() -> u64 {
    0
}
#[cfg(windows)]
fn shortcut(
    text: &str,
) -> Result<(
    windows::Win32::UI::Input::KeyboardAndMouse::HOT_KEY_MODIFIERS,
    u32,
)> {
    use windows::Win32::UI::Input::KeyboardAndMouse::*;
    let mut modifiers = MOD_NOREPEAT;
    let mut key = None;
    for part in text.split('+') {
        match part.to_lowercase().as_str() {
            "ctrl" => modifiers |= MOD_CONTROL,
            "shift" => modifiers |= MOD_SHIFT,
            "alt" => modifiers |= MOD_ALT,
            "win" => modifiers |= MOD_WIN,
            "space" => key = Some(0x20),
            "arrowup" => key = Some(0x26),
            "arrowdown" => key = Some(0x28),
            "arrowleft" => key = Some(0x25),
            "arrowright" => key = Some(0x27),
            "escape" => key = Some(0x1b),
            "delete" => key = Some(0x2e),
            "backspace" => key = Some(8),
            "enter" => key = Some(0x0d),
            "tab" => key = Some(0x09),
            other => {
                let other = other.strip_prefix("num").unwrap_or(other);
                if other.len() == 1 {
                    key = Some(other.to_uppercase().as_bytes()[0] as u32);
                } else if let Some(f) = other
                    .strip_prefix('f')
                    .and_then(|v| v.parse::<u32>().ok())
                    .filter(|n| (1..=24).contains(n))
                {
                    key = Some(0x70 + f - 1);
                } else {
                    anyhow::bail!("Unsupported shortcut key");
                }
            }
        }
    }
    Ok((modifiers, key.context("Shortcut needs a key")?))
}
#[cfg(windows)]
static TRAY_TX: std::sync::OnceLock<crate::engine::NotifySender> = std::sync::OnceLock::new();
#[cfg(windows)]
unsafe extern "system" fn tray_proc(
    hwnd: windows::Win32::Foundation::HWND,
    msg: u32,
    w: windows::Win32::Foundation::WPARAM,
    l: windows::Win32::Foundation::LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    use windows::Win32::{
        Foundation::{LRESULT, POINT},
        UI::WindowsAndMessaging::*,
    };
    unsafe {
        if msg == WM_APP + 9 {
            if l.0 as u32 == WM_LBUTTONDBLCLK {
                if let Some(tx) = TRAY_TX.get() {
                    let _ = tx.try_send(crate::engine::Event::Full);
                }
            }
            if l.0 as u32 == WM_RBUTTONUP {
                if let Ok(menu) = CreatePopupMenu() {
                    let _ = AppendMenuW(
                        menu,
                        MF_STRING,
                        1,
                        windows::core::PCWSTR(wide("Open VectorDash").as_ptr()),
                    );
                    let _ = AppendMenuW(
                        menu,
                        MF_STRING,
                        2,
                        windows::core::PCWSTR(wide("Exit VectorDash").as_ptr()),
                    );
                    let mut point = POINT::default();
                    let _ = GetCursorPos(&mut point);
                    let _ = SetForegroundWindow(hwnd);
                    let choice =
                        TrackPopupMenu(menu, TPM_RETURNCMD, point.x, point.y, Some(0), hwnd, None);
                    if let Some(tx) = TRAY_TX.get() {
                        if choice.0 == 1 {
                            let _ = tx.try_send(crate::engine::Event::Full);
                        } else if choice.0 == 2 {
                            let _ = tx.try_send(crate::engine::Event::Quit);
                        }
                    }
                    let _ = DestroyMenu(menu);
                }
            }
            return LRESULT(0);
        }
        DefWindowProcW(hwnd, msg, w, l)
    }
}
#[cfg(windows)]
fn tray_window(
    tx: crate::engine::NotifySender,
) -> Result<(
    windows::Win32::Foundation::HWND,
    windows::Win32::UI::Shell::NOTIFYICONDATAW,
)> {
    use windows::{
        Win32::{
            Foundation::HINSTANCE,
            UI::{Shell::*, WindowsAndMessaging::*},
        },
        core::PCWSTR,
    };
    unsafe {
        let _ = TRAY_TX.set(tx);
        let class = wide("VectorDashRustTray");
        let registration = WNDCLASSW {
            lpfnWndProc: Some(tray_proc),
            lpszClassName: PCWSTR(class.as_ptr()),
            hInstance: HINSTANCE::default(),
            ..Default::default()
        };
        RegisterClassW(&registration);
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            PCWSTR(class.as_ptr()),
            PCWSTR(class.as_ptr()),
            WINDOW_STYLE::default(),
            0,
            0,
            0,
            0,
            None,
            None,
            None,
            None,
        )?;
        let mut icon = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: 1,
            uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
            uCallbackMessage: WM_APP + 9,
            hIcon: LoadIconW(None, IDI_APPLICATION)?,
            ..Default::default()
        };
        for (dst, src) in icon
            .szTip
            .iter_mut()
            .zip("VectorDash — double-click to open".encode_utf16())
        {
            *dst = src;
        }
        let _ = Shell_NotifyIconW(NIM_ADD, &icon);
        Ok((hwnd, icon))
    }
}
#[cfg(windows)]
pub fn lower_integrity() -> Result<()> {
    use windows::{
        Win32::{
            Foundation::{CloseHandle, HANDLE, HLOCAL, LocalFree},
            Security::{Authorization::ConvertStringSidToSidW, *},
            System::Threading::{GetCurrentProcess, OpenProcessToken},
        },
        core::PCWSTR,
    };
    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_ADJUST_DEFAULT | TOKEN_QUERY | TOKEN_ADJUST_PRIVILEGES,
            &mut token,
        )?;
        let _ = AdjustTokenPrivileges(token, true, None, 0, None, None);
        let mut sid = PSID::default();
        ConvertStringSidToSidW(PCWSTR(wide("S-1-16-4096").as_ptr()), &mut sid)?;
        let label = TOKEN_MANDATORY_LABEL {
            Label: SID_AND_ATTRIBUTES {
                Sid: sid,
                Attributes: 0x00000020,
            },
        };
        let result = SetTokenInformation(
            token,
            TokenIntegrityLevel,
            &label as *const _ as *const _,
            std::mem::size_of::<TOKEN_MANDATORY_LABEL>() as u32 + GetLengthSid(sid),
        );
        let _ = LocalFree(Some(HLOCAL(sid.0)));
        let _ = CloseHandle(token);
        result?;
        Ok(())
    }
}
#[cfg(not(windows))]
pub fn lower_integrity() -> Result<()> {
    Ok(())
}
#[cfg(windows)]
pub fn material(hwnd: isize, popup: bool, accent: [u8; 3], opacity: f32) {
    use windows::Win32::{
        Foundation::HWND,
        Graphics::Dwm::{
            DWM_WINDOW_CORNER_PREFERENCE, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
            DwmSetWindowAttribute,
        },
    };
    #[repr(C)]
    struct Accent {
        state: i32,
        flags: i32,
        color: u32,
        animation: i32,
    }
    #[repr(C)]
    struct Data {
        attribute: i32,
        data: *const std::ffi::c_void,
        size: usize,
    }
    unsafe {
        let rounded: DWM_WINDOW_CORNER_PREFERENCE = DWMWCP_ROUND;
        let _ = DwmSetWindowAttribute(
            HWND(hwnd as *mut _),
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &rounded as *const _ as *const _,
            std::mem::size_of_val(&rounded) as u32,
        );
        let tint = ((opacity * 24.0) as u32) << 24
            | ((accent[2] as u32 / 6) << 16)
            | ((accent[1] as u32 / 7) << 8)
            | (accent[0] as u32 / 6);
        let policy = Accent {
            state: if popup { 4 } else { 0 },
            flags: 2,
            color: tint,
            animation: 0,
        };
        let data = Data {
            attribute: 19,
            data: &policy as *const _ as *const _,
            size: std::mem::size_of::<Accent>(),
        };
        if let Ok(library) = libloading::Library::new("user32.dll") {
            if let Ok(function) = library
                .get::<unsafe extern "system" fn(isize, *const Data) -> i32>(
                    b"SetWindowCompositionAttribute\0",
                )
            {
                let _ = function(hwnd, &data);
            }
        }
    }
}
#[cfg(not(windows))]
pub fn material(_: isize, _: bool, _: [u8; 3], _: f32) {}

static PROGRAMS: std::sync::OnceLock<Vec<Hit>> = std::sync::OnceLock::new();
pub fn warm_programs() {
    std::thread::spawn(|| {
        PROGRAMS.get_or_init(scan_programs);
    });
}
fn scan_programs() -> Vec<Hit> {
    let mut out = vec![];
    for folder in [
        std::env::var("APPDATA").unwrap_or_default() + "\\Microsoft\\Windows\\Start Menu\\Programs",
        std::env::var("PROGRAMDATA").unwrap_or_default()
            + "\\Microsoft\\Windows\\Start Menu\\Programs",
    ] {
        for entry in walkdir::WalkDir::new(folder)
            .follow_links(false)
            .into_iter()
            .filter_map(|x| x.ok())
        {
            if entry.file_type().is_file() && entry.path().extension().is_some_and(|x| x == "lnk") {
                let name = entry
                    .path()
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned();
                out.push(Hit {
                    path: entry.path().to_string_lossy().into_owned(),
                    kind: "application".into(),
                    text: name,
                    score: 2.1,
                    reason: "Start menu program".into(),
                    page: 0,
                    seconds: -1.0,
                    line: 0,
                });
            }
        }
    }
    for (name, file) in [
        ("Calculator", "calc.exe"),
        ("Notepad", "notepad.exe"),
        ("Paint", "mspaint.exe"),
    ] {
        let path =
            std::path::PathBuf::from(std::env::var("SystemRoot").unwrap_or("C:\\Windows".into()))
                .join("System32")
                .join(file);
        if path.exists() {
            out.push(Hit {
                path: path.to_string_lossy().into_owned(),
                kind: "application".into(),
                text: name.into(),
                score: 2.1,
                reason: "Windows program".into(),
                page: 0,
                seconds: -1.0,
                line: 0,
            });
        }
    }
    if let Ok(mut apps) = store_apps() {
        out.append(&mut apps);
    }
    out
}
#[cfg(windows)]
pub fn icon_png(path: &str) -> Result<Vec<u8>> {
    use windows::{
        Win32::{
            Graphics::Gdi::*,
            UI::{
                Shell::*,
                WindowsAndMessaging::{DestroyIcon, GetIconInfo, ICONINFO},
            },
        },
        core::PCWSTR,
    };
    unsafe {
        static CAPTURE: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _capture = CAPTURE.lock().unwrap();
        use windows::Win32::System::Com::{
            COINIT_APARTMENTTHREADED, CoInitializeEx, CoTaskMemFree, CoUninitialize,
        };
        struct Com(bool);
        impl Drop for Com {
            fn drop(&mut self) {
                if self.0 {
                    unsafe {
                        CoUninitialize();
                    }
                }
            }
        }
        let _com = Com(CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok());
        let mut info = SHFILEINFOW::default();
        let mut pidl = std::ptr::null_mut();
        let virtual_item = path.starts_with("shell:")
            && SHParseDisplayName(PCWSTR(wide(path).as_ptr()), None, &mut pidl, 0, None).is_ok();
        let wide_path = wide(path);
        let result = SHGetFileInfoW(
            if virtual_item {
                PCWSTR(pidl.cast())
            } else {
                PCWSTR(wide_path.as_ptr())
            },
            windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES::default(),
            Some(&mut info),
            std::mem::size_of_val(&info) as u32,
            SHGFI_ICON
                | SHGFI_LARGEICON
                | if virtual_item {
                    SHGFI_PIDL
                } else {
                    SHGFI_FLAGS(0)
                },
        );
        if !pidl.is_null() {
            CoTaskMemFree(Some(pidl.cast()));
        }
        if result == 0 || info.hIcon.0.is_null() {
            let mut stock = SHSTOCKICONINFO {
                cbSize: std::mem::size_of::<SHSTOCKICONINFO>() as u32,
                ..Default::default()
            };
            SHGetStockIconInfo(SIID_APPLICATION, SHGSI_ICON | SHGSI_LARGEICON, &mut stock)?;
            info.hIcon = stock.hIcon;
        }
        let mut icon = ICONINFO::default();
        if GetIconInfo(info.hIcon, &mut icon).is_err() {
            let _ = DestroyIcon(info.hIcon);
            let mut stock = SHSTOCKICONINFO {
                cbSize: std::mem::size_of::<SHSTOCKICONINFO>() as u32,
                ..Default::default()
            };
            SHGetStockIconInfo(SIID_APPLICATION, SHGSI_ICON | SHGSI_LARGEICON, &mut stock)?;
            info.hIcon = stock.hIcon;
            GetIconInfo(info.hIcon, &mut icon)?;
        }
        let mut bitmap = BITMAP::default();
        GetObjectW(
            icon.hbmColor.into(),
            std::mem::size_of::<BITMAP>() as i32,
            Some(&mut bitmap as *mut _ as *mut _),
        );
        let width = bitmap.bmWidth.max(1);
        let height = bitmap.bmHeight.max(1);
        let mut pixels = vec![0u8; (width * height * 4) as usize];
        let mut bi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let dc = GetDC(None);
        GetDIBits(
            dc,
            icon.hbmColor,
            0,
            height as u32,
            Some(pixels.as_mut_ptr() as *mut _),
            &mut bi,
            DIB_RGB_COLORS,
        );
        let _ = ReleaseDC(None, dc);
        let _ = DeleteObject(icon.hbmColor.into());
        let _ = DeleteObject(icon.hbmMask.into());
        let _ = DestroyIcon(info.hIcon);
        for pixel in pixels.chunks_exact_mut(4) {
            pixel.swap(0, 2);
            if pixel[3] == 0 {
                pixel[3] = 255;
            }
        }
        let image = image::RgbaImage::from_raw(width as u32, height as u32, pixels)
            .context("Invalid icon pixels")?;
        let mut out = std::io::Cursor::new(vec![]);
        image::DynamicImage::ImageRgba8(image).write_to(&mut out, image::ImageFormat::Png)?;
        Ok(out.into_inner())
    }
}
#[cfg(not(windows))]
pub fn icon_png(_: &str) -> Result<Vec<u8>> {
    anyhow::bail!("Windows icons unavailable")
}
#[cfg(windows)]
pub fn local_network(ip: std::net::Ipv4Addr) -> bool {
    use windows::Win32::{NetworkManagement::IpHelper::*, Networking::WinSock::*};
    unsafe {
        let mut length = 0u32;
        let flags = GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST | GAA_FLAG_SKIP_DNS_SERVER;
        let _ = GetAdaptersAddresses(AF_INET.0 as u32, flags, None, None, &mut length);
        if length == 0 {
            return false;
        }
        let mut data = vec![0u64; (length as usize + 7) / 8];
        let head = data.as_mut_ptr() as *mut IP_ADAPTER_ADDRESSES_LH;
        if GetAdaptersAddresses(AF_INET.0 as u32, flags, None, Some(head), &mut length) != 0 {
            return false;
        }
        let mut adapter = head;
        while !adapter.is_null() {
            let mut address = (*adapter).FirstUnicastAddress;
            while !address.is_null() {
                let sock = (*address).Address.lpSockaddr;
                if !sock.is_null() && (*sock).sa_family == AF_INET {
                    let sin = &*(sock as *const SOCKADDR_IN);
                    let own = std::net::Ipv4Addr::from(sin.sin_addr.S_un.S_addr.to_ne_bytes());
                    let bits = (*address).OnLinkPrefixLength.min(32);
                    let mask = if bits == 0 {
                        0
                    } else {
                        u32::MAX << (32 - bits)
                    };
                    if own.is_private() && (u32::from(own) & mask) == (u32::from(ip) & mask) {
                        return true;
                    }
                }
                address = (*address).Next;
            }
            adapter = (*adapter).Next;
        }
        false
    }
}
#[cfg(not(windows))]
pub fn local_network(ip: std::net::Ipv4Addr) -> bool {
    ip.is_loopback()
}
#[cfg(windows)]
pub fn play_at(path: &str, seconds: f32) -> Result<()> {
    let player = std::env::current_exe()?.with_file_name("ffplay.exe");
    anyhow::ensure!(player.is_file(), "Bundled media player missing");
    let mut command = Command::new(player);
    command
        .args([
            "-loglevel",
            "error",
            "-autoexit",
            "-ss",
            &seconds.max(0.0).to_string(),
            "-window_title",
            "VectorDash preview",
        ])
        .arg(path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    crate::content::hidden(&mut command);
    let child = command.spawn()?;
    PLAYBACK
        .get_or_init(|| std::sync::Mutex::new(vec![]))
        .lock()
        .map_err(|_| anyhow::anyhow!("Playback state unavailable"))?
        .push(child);
    Ok(())
}
#[cfg(not(windows))]
pub fn play_at(_: &str, _: f32) -> Result<()> {
    Ok(())
}
static PLAYBACK: std::sync::OnceLock<std::sync::Mutex<Vec<Child>>> = std::sync::OnceLock::new();
pub fn stop_playback() {
    if let Some(players) = PLAYBACK.get() {
        if let Ok(mut players) = players.lock() {
            for mut child in players.drain(..) {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }
}

pub fn parse_devices(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            let (id, description) = line.split_once(':')?;
            let id = id.trim();
            if id.len() < 24
                && ["CUDA", "Vulkan", "SYCL", "OpenCL", "Metal"]
                    .iter()
                    .any(|p| id.starts_with(p))
            {
                Some((id.into(), description.trim().into()))
            } else {
                None
            }
        })
        .collect()
}
#[cfg(windows)]
fn store_apps() -> Result<Vec<Hit>> {
    use windows::{
        Win32::{
            System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoTaskMemFree},
            UI::Shell::*,
        },
        core::PCWSTR,
    };
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let root: IShellItem =
            SHCreateItemFromParsingName(PCWSTR(wide("shell:AppsFolder").as_ptr()), None)?;
        let enumerator: IEnumShellItems = root.BindToHandler(None, &BHID_EnumItems)?;
        let mut out = vec![];
        for _ in 0..10000 {
            let mut item = [None];
            let mut count = 0;
            let status = enumerator.Next(&mut item, Some(&mut count));
            if status.is_err() || count == 0 {
                break;
            }
            if let Some(item) = item[0].take() {
                let display = item.GetDisplayName(SIGDN_NORMALDISPLAY)?;
                let name = display.to_string()?;
                CoTaskMemFree(Some(display.0 as *const _));
                let parsing = item.GetDisplayName(SIGDN_DESKTOPABSOLUTEPARSING)?;
                let path = parsing.to_string()?;
                CoTaskMemFree(Some(parsing.0 as *const _));
                let id = path.rsplit('\\').next().unwrap_or(&path);
                out.push(Hit {
                    path: format!("shell:AppsFolder\\{id}"),
                    kind: "application".into(),
                    text: name,
                    score: 2.1,
                    reason: "Installed Windows app".into(),
                    page: 0,
                    seconds: -1.0,
                    line: 0,
                });
            }
        }
        Ok(out)
    }
}
#[cfg(not(windows))]
fn store_apps() -> Result<Vec<Hit>> {
    Ok(vec![])
}

static MAIN_WINDOW: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);
pub fn register_main_window(hwnd: isize) {
    MAIN_WINDOW.store(hwnd, std::sync::atomic::Ordering::Relaxed);
}
#[cfg(windows)]
pub fn wake_main_window() {
    use windows::Win32::{Foundation::HWND, UI::WindowsAndMessaging::*};
    let value = MAIN_WINDOW.load(std::sync::atomic::Ordering::Relaxed);
    if value == 0 {
        return;
    }
    unsafe {
        let hwnd = HWND(value as *mut _);
        let _ = ShowWindowAsync(hwnd, SW_RESTORE);
        let _ = PostMessageW(
            Some(hwnd),
            WM_NULL,
            windows::Win32::Foundation::WPARAM(0),
            windows::Win32::Foundation::LPARAM(0),
        );
    }
}
#[cfg(not(windows))]
pub fn wake_main_window() {}
