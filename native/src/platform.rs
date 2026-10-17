use crate::{config::Settings, db::Hit};
use anyhow::{Context, Result};
use std::{
    path::Path,
    process::{Child, Command},
    time::{Duration, Instant},
};
pub struct Job(#[cfg(windows)] windows::Win32::Foundation::HANDLE);
#[cfg(windows)]
#[derive(Default)]
struct OwnedJobs {
    handles: std::collections::HashSet<usize>,
    finished_cpu: u64,
}
#[cfg(windows)]
fn jobs() -> &'static std::sync::Mutex<OwnedJobs> {
    static JOBS: std::sync::OnceLock<std::sync::Mutex<OwnedJobs>> = std::sync::OnceLock::new();
    JOBS.get_or_init(Default::default)
}
#[cfg(windows)]
fn job_cpu(handle: windows::Win32::Foundation::HANDLE) -> u64 {
    use windows::Win32::System::JobObjects::*;
    let mut info = JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();
    unsafe {
        if QueryInformationJobObject(
            Some(handle),
            JobObjectBasicAccountingInformation,
            &mut info as *mut _ as *mut _,
            std::mem::size_of_val(&info) as u32,
            None,
        )
        .is_ok()
        {
            return info.TotalKernelTime.max(0) as u64 + info.TotalUserTime.max(0) as u64;
        }
    }
    0
}
#[cfg(windows)]
impl Job {
    fn new(handle: windows::Win32::Foundation::HANDLE) -> Self {
        jobs().lock().unwrap().handles.insert(handle.0 as usize);
        Self(handle)
    }
}
#[cfg(windows)]
impl Drop for Job {
    fn drop(&mut self) {
        let mut registry = jobs().lock().unwrap();
        registry.finished_cpu = registry.finished_cpu.saturating_add(job_cpu(self.0));
        registry.handles.remove(&(self.0.0 as usize));
        unsafe {
            let _ = windows::Win32::Foundation::CloseHandle(self.0);
        }
    }
}
// Job accounting includes helpers that finish between monitoring samples.
#[cfg(windows)]
pub fn owned_usage() -> (u64, std::collections::HashSet<u32>) {
    use windows::Win32::{
        Foundation::{FILETIME, HANDLE},
        System::{
            JobObjects::*,
            Threading::{GetCurrentProcess, GetProcessTimes},
        },
    };
    let registry = jobs().lock().unwrap();
    let mut cpu = registry.finished_cpu;
    let mut pids = std::collections::HashSet::from([std::process::id()]);
    unsafe {
        let (mut created, mut exited, mut kernel, mut user) = (
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
        );
        if GetProcessTimes(
            GetCurrentProcess(),
            &mut created,
            &mut exited,
            &mut kernel,
            &mut user,
        )
        .is_ok()
        {
            let n = |f: FILETIME| ((f.dwHighDateTime as u64) << 32) | f.dwLowDateTime as u64;
            cpu = cpu.saturating_add(n(kernel) + n(user));
        }
        for &value in &registry.handles {
            let handle = HANDLE(value as *mut _);
            cpu = cpu.saturating_add(job_cpu(handle));
            for capacity in [32usize, 256, 1024] {
                let mut buffer = vec![0u64; capacity + 1];
                if QueryInformationJobObject(
                    Some(handle),
                    JobObjectBasicProcessIdList,
                    buffer.as_mut_ptr().cast(),
                    (buffer.len() * 8) as u32,
                    None,
                )
                .is_ok()
                {
                    let count = *((buffer.as_ptr() as *const u32).add(1)) as usize;
                    for pid in std::slice::from_raw_parts(
                        buffer.as_ptr().add(1) as *const usize,
                        count.min(capacity),
                    ) {
                        pids.insert(*pid as u32);
                    }
                    break;
                }
            }
        }
    }
    (cpu, pids)
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
        let job = Job::new(handle);
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
pub fn limit_worker_memory(child: &Child, mib: usize) -> Result<Job> {
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
        info.ProcessMemoryLimit = mib * 1024 * 1024;
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
        Ok(Job::new(job))
    }
}
#[cfg(not(windows))]
pub fn limit_worker_memory(_: &Child, _: usize) -> Result<Job> {
    Ok(Job())
}
pub fn limit_worker(child: &Child) -> Result<Job> {
    limit_worker_memory(child, 1024)
}
#[cfg(windows)]
pub fn open(path: &str) -> Result<()> {
    use windows::{
        Win32::UI::{
            Shell::{SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SHELLEXECUTEINFOW, ShellExecuteExW},
            WindowsAndMessaging::SW_SHOWNORMAL,
        },
        core::PCWSTR,
    };
    // Shell shortcuts and packaged apps can require COM on this launch thread.
    use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};
    struct Apartment(bool);
    impl Drop for Apartment {
        fn drop(&mut self) {
            if self.0 {
                unsafe {
                    CoUninitialize();
                }
            }
        }
    }
    let _apartment = Apartment(unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok() });
    if crate::admin::elevated()
        && (path.starts_with(r"\\") || path.as_bytes().get(1) == Some(&b':'))
    {
        match open_from_desktop(path) {
            Ok(()) => return Ok(()),
            Err(error) if !std::path::Path::new(path).exists() => anyhow::bail!(
                "Explorer could not open {path}: {error:#}. Check the network share and desktop-session drive mapping"
            ),
            Err(_) => {}
        }
    }
    let file = wide(path);
    let verb = wide("open");
    // Complete the shell handoff before this worker exits, without making the
    // UI wait or waiting for the launched application's input-idle state.
    let mut launch = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI,
        lpVerb: PCWSTR(verb.as_ptr()),
        lpFile: PCWSTR(file.as_ptr()),
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };
    unsafe { ShellExecuteExW(&mut launch) }
        .map_err(|error| anyhow::anyhow!("Windows could not open {path}: {error}"))?;
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
        crate::tray::publish(crate::tray::Visual {
            status: crate::tray::current().status,
            accent: s.accent,
        });
        let mut tray_visual = crate::tray::current();
        let mut tray = match tray_window(tx.clone(), tray_visual) {
            Ok(v) => Some(v),
            Err(e) => {
                let _ = tx.send(crate::engine::Event::Log(
                    "Error".into(),
                    format!("Tray icon: {e}"),
                ));
                None
            }
        };
        let mut tray_attempt = Instant::now() - Duration::from_secs(2);
        let requested = s
            .global_bindings()
            .into_iter()
            .enumerate()
            .map(|(i, (action, binding))| (777 + i as i32, action, binding))
            .collect::<Vec<_>>();
        let mut bindings = Vec::new();
        for (id, _action, text) in &requested {
            match shortcut(text) {
                Ok((mods, key)) => {
                    let registered = RegisterHotKey(None, *id, mods, key).is_ok();
                    if !registered {
                        let _ = tx.try_send(crate::engine::Event::Log(
                            "Warning".into(),
                            format!(
                                "Shortcut {text} is currently unavailable; retrying automatically"
                            ),
                        ));
                    }
                    bindings.push((*id, mods, key, registered));
                }
                Err(e) => {
                    let _ = tx.try_send(crate::engine::Event::Log("Warning".into(), e.to_string()));
                }
            }
        }
        let mut retry = Instant::now();
        let mut message = MSG::default();
        let mut down = false;
        let mut last = Instant::now() - Duration::from_secs(1);
        while !shutdown.load(std::sync::atomic::Ordering::Relaxed) {
            if retry.elapsed() >= Duration::from_secs(2) {
                retry = Instant::now();
                for (id, mods, key, registered) in &mut bindings {
                    if !*registered && RegisterHotKey(None, *id, *mods, *key).is_ok() {
                        *registered = true;
                        let _ = tx.try_send(crate::engine::Event::Log(
                            "Info".into(),
                            "Global shortcut registered after retry".into(),
                        ));
                    }
                }
            }
            while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                if message.hwnd.0 as usize != 0 {
                    let _ = TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
                if message.message == WM_HOTKEY {
                    let hwnd = GetForegroundWindow();
                    if let Some((_, action, _)) = requested
                        .iter()
                        .find(|(id, _, _)| *id == message.wParam.0 as i32)
                    {
                        let _ = tx.send(crate::engine::Event::Shortcut(
                            hwnd.0 as isize,
                            action.clone(),
                        ));
                    }
                }
            }
            if let Some((_, data)) = tray.as_mut() {
                if (crate::tray::current() != tray_visual
                    || TRAY_READD.load(std::sync::atomic::Ordering::Relaxed))
                    && tray_attempt.elapsed() >= Duration::from_secs(1)
                {
                    tray_attempt = Instant::now();
                    let force = TRAY_READD.swap(false, std::sync::atomic::Ordering::Relaxed);
                    if let Err(error) = refresh_tray(data, &mut tray_visual, force) {
                        TRAY_READD.store(true, std::sync::atomic::Ordering::Relaxed);
                        let _ = tx.try_send(crate::engine::Event::Log(
                            "Warning".into(),
                            format!("Tray icon update: {error:#}"),
                        ));
                    }
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
                        if name == "CabinetWClass" && s.explorer_scope {
                            let notify = tx.clone();
                            let target = hwnd.0 as isize;
                            std::thread::spawn(move || match explorer_folder(target) {
                                Ok(path) => {
                                    let _ = notify
                                        .send(crate::engine::Event::ScopedSummon(target, path));
                                }
                                Err(error) => {
                                    let _=notify.send(crate::engine::Event::Log("Info".into(),format!("Explorer folder scope unavailable; using all sources: {error}")));
                                    let _ = notify
                                        .send(crate::engine::Event::Summon(target, String::new()));
                                }
                            });
                        } else {
                            let _ = tx
                                .send(crate::engine::Event::Summon(hwnd.0 as isize, String::new()));
                        }
                    }
                }
                last = Instant::now();
            }
            down = pressed;
            std::thread::sleep(Duration::from_millis(20));
        }
        for (id, _, _) in requested {
            let _ = UnregisterHotKey(None, id);
        }
        if let Some((hwnd, icon)) = tray {
            let _ = windows::Win32::UI::Shell::Shell_NotifyIconW(
                windows::Win32::UI::Shell::NIM_DELETE,
                &icon,
            );
            let _ = DestroyIcon(icon.hIcon);
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
fn file_drop_payload(path: &str) -> Vec<u8> {
    let words: Vec<u16> = path.encode_utf16().chain([0, 0]).collect();
    let mut bytes = vec![0u8; 20];
    bytes[..4].copy_from_slice(&20u32.to_le_bytes());
    bytes[16..20].copy_from_slice(&1u32.to_le_bytes());
    for word in words {
        bytes.extend_from_slice(&word.to_le_bytes());
    }
    bytes
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
        let payload = file_drop_payload(path);
        let size = payload.len();
        let memory = GlobalAlloc(GMEM_MOVEABLE, size)?;
        let ptr = GlobalLock(memory) as *mut u8;
        if ptr.is_null() {
            let _ = GlobalFree(Some(memory));
            anyhow::bail!("Clipboard allocation failed");
        }
        std::ptr::copy_nonoverlapping(payload.as_ptr(), ptr, size);
        let _ = GlobalUnlock(memory);
        let owner = MAIN_WINDOW.load(std::sync::atomic::Ordering::Relaxed);
        if owner == 0 {
            let _ = GlobalFree(Some(memory));
            anyhow::bail!("Clipboard owner window unavailable");
        }
        if let Err(e) = OpenClipboard(Some(windows::Win32::Foundation::HWND(owner as *mut _))) {
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
        let _ = RegDeleteValueW(key, PCWSTR(wide("VectorDashRust").as_ptr()));
        let name = wide("VectorDash");
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
            "win" | "windows" => modifiers |= MOD_WIN,
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
pub fn explorer_folder(target: isize) -> Result<String> {
    use windows::{
        Win32::{
            System::{Com::*, Variant::VARIANT},
            UI::Shell::*,
        },
        core::Interface,
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
    unsafe {
        let _com = Com(CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok());
        let windows: IShellWindows = CoCreateInstance(&ShellWindows, None, CLSCTX_LOCAL_SERVER)?;
        for i in 0..windows.Count()?.min(1000) {
            let Ok(dispatch) = windows.Item(&VARIANT::from(i)) else {
                continue;
            };
            let Ok(browser) = dispatch.cast::<IWebBrowser2>() else {
                continue;
            };
            if browser.HWND().ok().map(|h| h.0) != Some(target) {
                continue;
            }
            // Ask the Explorer shell browser for its active view, not a tab's URL.
            let services = browser.cast::<IServiceProvider>()?;
            let shell: IShellBrowser = services.QueryService(&SID_STopLevelBrowser)?;
            let shell_view = shell.QueryActiveShellView()?;
            if !windows::Win32::UI::WindowsAndMessaging::IsWindowVisible(shell_view.GetWindow()?)
                .as_bool()
            {
                continue;
            }
            let view = shell_view.cast::<IFolderView>()?;
            let folder: IPersistFolder2 = view.GetFolder()?;
            let pidl = folder.GetCurFolder()?;
            let mut buffer = vec![0u16; 32768];
            let ok = SHGetPathFromIDListEx(pidl, &mut buffer, GPFIDL_DEFAULT).as_bool();
            CoTaskMemFree(Some(pidl as *const _));
            if ok {
                let length = buffer.iter().position(|c| *c == 0).unwrap_or(buffer.len());
                let path = String::from_utf16_lossy(&buffer[..length]);
                if std::path::Path::new(&path).is_dir() {
                    return Ok(path);
                }
            }
        }
        anyhow::bail!("This Explorer view has no indexed filesystem folder")
    }
}
#[cfg(windows)]
static TRAY_TX: std::sync::Mutex<Option<crate::engine::NotifySender>> = std::sync::Mutex::new(None);
#[cfg(windows)]
static TASKBAR_CREATED: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
#[cfg(windows)]
static TRAY_READD: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
#[cfg(windows)]
fn tray_sender() -> Option<crate::engine::NotifySender> {
    TRAY_TX.lock().ok().and_then(|tx| tx.clone())
}
#[cfg(windows)]
fn tray_open(tx: &crate::engine::NotifySender) {
    let _ = tx.try_send(crate::engine::Event::Log(
        "Info".into(),
        "Tray requested main window".into(),
    ));
    let _ = tx.try_send(crate::engine::Event::Full);
}
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
        if TASKBAR_CREATED
            .get()
            .is_some_and(|id| *id != 0 && msg == *id)
        {
            TRAY_READD.store(true, std::sync::atomic::Ordering::Relaxed);
            return LRESULT(0);
        }
        if msg == WM_APP + 9 {
            // Version 4 packs the icon ID in HIWORD; legacy callbacks use the whole LPARAM.
            let event = (l.0 as u32) & 0xffff;
            if event == WM_LBUTTONDBLCLK
                || event == windows::Win32::UI::Shell::NIN_SELECT
                || event
                    == (windows::Win32::UI::Shell::NIN_SELECT | windows::Win32::UI::Shell::NINF_KEY)
            {
                if let Some(tx) = tray_sender() {
                    tray_open(&tx);
                }
            }
            if event == WM_RBUTTONUP || event == WM_CONTEXTMENU {
                if let Ok(menu) = CreatePopupMenu() {
                    let _ = AppendMenuW(
                        menu,
                        MF_STRING,
                        1,
                        windows::core::PCWSTR(
                            wide(crate::i18n::tray_text("Open VectorDash")).as_ptr(),
                        ),
                    );
                    let _ = AppendMenuW(
                        menu,
                        MF_STRING,
                        2,
                        windows::core::PCWSTR(
                            wide(crate::i18n::tray_text("Exit VectorDash")).as_ptr(),
                        ),
                    );
                    let mut point = POINT::default();
                    let _ = GetCursorPos(&mut point);
                    if event == WM_CONTEXTMENU {
                        point.x = (w.0 as u16 as i16) as i32;
                        point.y = ((w.0 >> 16) as u16 as i16) as i32;
                    }
                    let _ = SetForegroundWindow(hwnd);
                    let choice = TrackPopupMenu(
                        menu,
                        TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON,
                        point.x,
                        point.y,
                        Some(0),
                        hwnd,
                        None,
                    );
                    if let Some(tx) = tray_sender() {
                        if choice.0 == 1 {
                            tray_open(&tx);
                        } else if choice.0 == 2 {
                            let _ = tx.send(crate::engine::Event::Quit);
                        }
                    }
                    let _ = DestroyMenu(menu);
                    let _ = PostMessageW(
                        Some(hwnd),
                        WM_NULL,
                        windows::Win32::Foundation::WPARAM(0),
                        windows::Win32::Foundation::LPARAM(0),
                    );
                }
            }
            return LRESULT(0);
        }
        DefWindowProcW(hwnd, msg, w, l)
    }
}
#[cfg(windows)]
fn tray_icon(color: [u8; 3]) -> Result<windows::Win32::UI::WindowsAndMessaging::HICON> {
    let rgba = crate::tray::pixels(color);
    let pixels: Vec<u8> = rgba
        .chunks_exact(4)
        .flat_map(|p| {
            [
                ((p[2] as u16 * p[3] as u16) / 255) as u8,
                ((p[1] as u16 * p[3] as u16) / 255) as u8,
                ((p[0] as u16 * p[3] as u16) / 255) as u8,
                p[3],
            ]
        })
        .collect();
    let mask = [0u8; 128];
    Ok(unsafe {
        windows::Win32::UI::WindowsAndMessaging::CreateIcon(
            None,
            32,
            32,
            1,
            32,
            mask.as_ptr(),
            pixels.as_ptr(),
        )?
    })
}
#[cfg(windows)]
fn tray_tip(data: &mut windows::Win32::UI::Shell::NOTIFYICONDATAW, visual: crate::tray::Visual) {
    data.szTip.fill(0);
    let text = format!("VectorDash — {}", crate::i18n::tray_text(visual.label()));
    for (dst, src) in data.szTip.iter_mut().take(127).zip(text.encode_utf16()) {
        *dst = src;
    }
}
#[cfg(windows)]
fn refresh_tray(
    data: &mut windows::Win32::UI::Shell::NOTIFYICONDATAW,
    previous: &mut crate::tray::Visual,
    force: bool,
) -> Result<()> {
    use windows::Win32::UI::{Shell::*, WindowsAndMessaging::DestroyIcon};
    let visual = crate::tray::current();
    if visual == *previous && !force {
        return Ok(());
    }
    let old = data.hIcon;
    let replacement = if visual != *previous {
        tray_icon(visual.color())?
    } else {
        old
    };
    let mut update = *data;
    update.hIcon = replacement;
    tray_tip(&mut update, visual);
    unsafe {
        let restored = if force {
            Shell_NotifyIconW(NIM_ADD, &update).as_bool()
        } else {
            false
        };
        let ok = restored
            || Shell_NotifyIconW(NIM_MODIFY, &update).as_bool()
            || Shell_NotifyIconW(NIM_ADD, &update).as_bool();
        if !ok {
            if replacement != old {
                let _ = DestroyIcon(replacement);
            }
            anyhow::bail!("Windows could not refresh tray icon");
        }
        update.Anonymous.uVersion = NOTIFYICON_VERSION_4;
        let _ = Shell_NotifyIconW(NIM_SETVERSION, &update);
        *data = update;
        *previous = visual;
        if replacement != old {
            let _ = DestroyIcon(old);
        }
    }
    Ok(())
}
#[cfg(windows)]
fn tray_window(
    tx: crate::engine::NotifySender,
    visual: crate::tray::Visual,
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
        *TRAY_TX
            .lock()
            .map_err(|_| anyhow::anyhow!("Tray sender lock unavailable"))? = Some(tx);
        let restart = *TASKBAR_CREATED
            .get_or_init(|| RegisterWindowMessageW(PCWSTR(wide("TaskbarCreated").as_ptr())));
        let class = wide("VectorDashRustTray");
        let registration = WNDCLASSW {
            style: CS_DBLCLKS,
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
            uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP | NIF_SHOWTIP,
            uCallbackMessage: WM_APP + 9,
            hIcon: tray_icon(visual.color())?,
            ..Default::default()
        };
        tray_tip(&mut icon, visual);
        let _ = ChangeWindowMessageFilterEx(hwnd, WM_APP + 9, MSGFLT_ALLOW, None);
        if restart != 0 {
            let _ = ChangeWindowMessageFilterEx(hwnd, restart, MSGFLT_ALLOW, None);
        }
        if !Shell_NotifyIconW(NIM_ADD, &icon).as_bool() {
            let _ = DestroyIcon(icon.hIcon);
            let _ = DestroyWindow(hwnd);
            anyhow::bail!("Windows could not add the tray icon");
        }
        icon.Anonymous.uVersion = NOTIFYICON_VERSION_4;
        let _ = Shell_NotifyIconW(NIM_SETVERSION, &icon);
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
static NATIVE_GLASS_MATERIAL: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

#[cfg(windows)]
pub fn clip_window(value: isize, radius: f32, refresh_material: bool) {
    use windows::Win32::{
        Foundation::{HWND, POINT, RECT},
        Graphics::{Dwm::*, Gdi::*},
        UI::WindowsAndMessaging::*,
    };
    unsafe {
        let hwnd = HWND(value as *mut _);
        if !IsWindowVisible(hwnd).as_bool() || IsIconic(hwnd).as_bool() {
            crate::backdrop::hide();
            return;
        }
        static GEOMETRY: std::sync::OnceLock<
            std::sync::Mutex<std::collections::HashMap<isize, (i32, i32, i32, i32, i32)>>,
        > = std::sync::OnceLock::new();
        let mut geometry = GEOMETRY.get_or_init(Default::default).lock().unwrap();
        if NATIVE_GLASS_MATERIAL.load(std::sync::atomic::Ordering::Relaxed) {
            // Winit may restore caption flags while applying a native resize. Keep only
            // the invisible sizing frame needed by DWM, preserving visibility/maximize bits.
            let mask = (WS_CAPTION.0 | WS_THICKFRAME.0) as isize;
            let current = GetWindowLongPtrW(hwnd, GWL_STYLE);
            let style = (current & !mask) | WS_THICKFRAME.0 as isize;
            if current != style {
                SetWindowLongPtrW(hwnd, GWL_STYLE, style);
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
                );
            }
            // DWM only offers a fixed small radius. Apply the same custom 28px
            // region as the renderer to the entire HWND, including its backdrop.
        }
        if IsZoomed(hwnd).as_bool() {
            let _ = crate::backdrop::sync(value, 0.0);
            if geometry.get(&value) != Some(&(0, 0, 0, 0, 0)) {
                SetWindowRgn(hwnd, None, true);
                geometry.insert(value, (0, 0, 0, 0, 0));
            }
            return;
        }
        let (mut client, mut window, mut origin) =
            (RECT::default(), RECT::default(), POINT::default());
        if GetClientRect(hwnd, &mut client).is_err()
            || GetWindowRect(hwnd, &mut window).is_err()
            || !ClientToScreen(hwnd, &mut origin).as_bool()
        {
            return;
        }
        let (x, y) = (origin.x - window.left, origin.y - window.top);
        let _ = crate::backdrop::sync(value, radius);
        // Keep the hard safety mask outside the antialiased 28px paint edge.
        // A smaller radius includes the fringe without adding any painted frame.
        let diameter = ((radius - 2.0).max(0.0) * 2.0).round() as i32;
        let key = (x, y, client.right, client.bottom, diameter);
        if !refresh_material && geometry.get(&value) == Some(&key) {
            return;
        }
        let region = CreateRoundRectRgn(
            x,
            y,
            x + client.right,
            y + client.bottom,
            diameter,
            diameter,
        );
        let current = CreateRectRgn(0, 0, 0, 0);
        let unchanged = GetWindowRgn(hwnd, current).0 != 0 && EqualRgn(region, current).as_bool();
        let _ = DeleteObject(current.into());
        let applied = if unchanged {
            let _ = DeleteObject(region.into());
            true
        } else if region.0.is_null() {
            false
        } else if SetWindowRgn(hwnd, Some(region), true) == 0 {
            let _ = DeleteObject(region.into());
            false
        } else {
            true
        };
        if applied {
            geometry.insert(value, key);
        }
        if unchanged && !refresh_material {
            return;
        }
        if NATIVE_GLASS_MATERIAL.load(std::sync::atomic::Ordering::Relaxed) {
            // System acrylic is clipped by the HWND region. Do not enable a second
            // legacy blur surface, which can leave a rectangular fill in the corners.
            return;
        }
        // The compositor's blur surface has its own region; alpha-only rounded painting
        // or a hit-test region does not restrict a full-window backdrop surface.
        let blur = CreateRoundRectRgn(
            1,
            1,
            client.right - 1,
            client.bottom - 1,
            diameter,
            diameter,
        );
        let bb = DWM_BLURBEHIND {
            dwFlags: DWM_BB_ENABLE | DWM_BB_BLURREGION,
            fEnable: true.into(),
            hRgnBlur: blur,
            fTransitionOnMaximized: false.into(),
        };
        let _ = DwmEnableBlurBehindWindow(hwnd, &bb);
        let _ = DeleteObject(blur.into());
    }
}
#[cfg(not(windows))]
pub fn clip_window(_: isize, _: f32, _: bool) {}
#[cfg(windows)]
pub fn material(value: isize, popup: bool, dark: bool, _: [u8; 3], _: f32) {
    use windows::Win32::{
        Foundation::HWND,
        Graphics::Dwm::*,
        UI::{Controls::MARGINS, WindowsAndMessaging::*},
    };
    unsafe {
        let hwnd = HWND(value as *mut _);
        NATIVE_GLASS_MATERIAL.store(true, std::sync::atomic::Ordering::Relaxed);
        let mask =
            (WS_CAPTION.0 | WS_THICKFRAME.0 | WS_SYSMENU.0 | WS_MINIMIZEBOX.0 | WS_MAXIMIZEBOX.0)
                as isize;
        let current = GetWindowLongPtrW(hwnd, GWL_STYLE);
        let controls = if popup {
            0
        } else {
            (WS_SYSMENU.0 | WS_MINIMIZEBOX.0 | WS_MAXIMIZEBOX.0) as isize
        };
        let style = (current & !mask) | WS_THICKFRAME.0 as isize | controls;
        if GetWindowLongPtrW(hwnd, GWL_STYLE) != style {
            SetWindowLongPtrW(hwnd, GWL_STYLE, style);
            let _ = SetWindowPos(
                hwnd,
                None,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
            );
        }
        if windows_build() >= 22000 {
            let no_border = 0xfffffffeu32;
            let _ = DwmSetWindowAttribute(hwnd, DWMWA_BORDER_COLOR, &no_border as *const _ as _, 4);
        }
        let mode = dark as i32;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            &mode as *const _ as _,
            4,
        );
        // Legacy accent blur creates a rectangular compositor layer outside the window region.
        // The system backdrop ignores custom radii; a clipped composition brush supplies
        // the blur instead. This HWND only carries the transparent UI.
        // The renderer and native region own the complete 28px silhouette. A DWM
        // frame/shadow uses a different outline and leaks behind those corners.
        let policy = DWMNCRP_DISABLED;
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_NCRENDERING_POLICY, &policy as *const _ as _, 4);
        if windows_build() >= 22000 {
            let corners = DWMWCP_DONOTROUND;
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_WINDOW_CORNER_PREFERENCE,
                &corners as *const _ as _,
                4,
            );
            let backdrop = DWMSBT_NONE;
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_SYSTEMBACKDROP_TYPE,
                &backdrop as *const _ as _,
                4,
            );
        }
        let empty = windows::Win32::Graphics::Gdi::CreateRectRgn(0, 0, 0, 0);
        let alpha = DWM_BLURBEHIND {
            dwFlags: DWM_BB_ENABLE | DWM_BB_BLURREGION,
            fEnable: true.into(),
            hRgnBlur: empty,
            fTransitionOnMaximized: false.into(),
        };
        let _ = DwmEnableBlurBehindWindow(hwnd, &alpha);
        let _ = windows::Win32::Graphics::Gdi::DeleteObject(empty.into());
        let margin = 0;
        let _ = DwmExtendFrameIntoClientArea(
            hwnd,
            &MARGINS {
                cxLeftWidth: margin,
                cxRightWidth: margin,
                cyTopHeight: margin,
                cyBottomHeight: margin,
            },
        );
    }
}
#[cfg(not(windows))]
pub fn material(_: isize, _: bool, _: bool, _: [u8; 3], _: f32) {}

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
    stop_playback();
    let log = std::env::var_os("VECTORDASH_MODEL_LOG")
        .map(std::path::PathBuf::from)
        .map(|p| p.with_file_name("playback.log"))
        .unwrap_or_else(|| std::env::temp_dir().join("vectordash-playback.log"));
    let stderr = std::fs::File::create(&log)?;
    let mut command = Command::new(player);
    if audio_only(path) {
        command.args(["-nodisp", "-vn"]);
    }

    command
        .args([
            "-loglevel",
            "error",
            "-autoexit",
            "-nostats",
            "-hide_banner",
            "-ss",
            &seconds.max(0.0).to_string(),
            "-window_title",
            "VectorDash preview",
        ])
        .arg(path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::from(stderr));
    crate::content::hidden(&mut command);
    let child = command.spawn()?;
    PLAYBACK
        .get_or_init(|| std::sync::Mutex::new(vec![]))
        .lock()
        .map_err(|_| anyhow::anyhow!("Playback state unavailable"))?
        .push(Playback {
            child,
            log,
            paused: false,
        });
    Ok(())
}
#[cfg(not(windows))]
pub fn play_at(_: &str, _: f32) -> Result<()> {
    Ok(())
}
pub fn play_audio_stream(pcm: Option<(u32, u16)>) -> Result<std::process::ChildStdin> {
    let player = std::env::current_exe()?.with_file_name("ffplay.exe");
    anyhow::ensure!(player.is_file(), "Bundled media player missing");
    stop_playback();
    let log = std::env::var_os("VECTORDASH_MODEL_LOG")
        .map(std::path::PathBuf::from)
        .map(|p| p.with_file_name("playback.log"))
        .unwrap_or_else(|| std::env::temp_dir().join("vectordash-playback.log"));
    let stderr = std::fs::File::create(&log)?;
    let mut command = Command::new(player);
    command.args([
        "-nodisp",
        "-vn",
        "-autoexit",
        "-loglevel",
        "error",
        "-nostats",
        "-hide_banner",
        "-probesize",
        "4096",
        "-analyzeduration",
        "0",
    ]);
    if let Some((rate, channels)) = pcm {
        anyhow::ensure!(
            (8000..=192000).contains(&rate) && (1..=8).contains(&channels),
            "Invalid PCM audio format"
        );
        command.args([
            "-f",
            "s16le",
            "-ar",
            &rate.to_string(),
            "-ch_layout",
            &format!("{channels}c"),
        ]);
    }
    command
        .args(["-i", "pipe:0"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::from(stderr));
    crate::content::hidden(&mut command);
    let mut child = command.spawn()?;
    let input = child.stdin.take().context("Audio input unavailable")?;
    PLAYBACK
        .get_or_init(|| std::sync::Mutex::new(vec![]))
        .lock()
        .map_err(|_| anyhow::anyhow!("Playback state unavailable"))?
        .push(Playback {
            child,
            log,
            paused: false,
        });
    Ok(input)
}
struct Playback {
    child: Child,
    log: std::path::PathBuf,
    paused: bool,
}
pub fn pause_playback(paused: bool) -> Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        #[link(name = "ntdll")]
        unsafe extern "system" {
            fn NtSuspendProcess(process: *mut std::ffi::c_void) -> i32;
            fn NtResumeProcess(process: *mut std::ffi::c_void) -> i32;
        }
        if let Some(players) = PLAYBACK.get() {
            let mut players = players
                .lock()
                .map_err(|_| anyhow::anyhow!("Playback state unavailable"))?;
            for player in players.iter_mut() {
                if player.paused != paused && player.child.try_wait()?.is_none() {
                    let status = unsafe {
                        if paused {
                            NtSuspendProcess(player.child.as_raw_handle())
                        } else {
                            NtResumeProcess(player.child.as_raw_handle())
                        }
                    };
                    anyhow::ensure!(status >= 0, "Could not pause/resume playback ({status:#x})");
                    player.paused = paused;
                }
            }
        }
    }
    Ok(())
}
static PLAYBACK: std::sync::OnceLock<std::sync::Mutex<Vec<Playback>>> = std::sync::OnceLock::new();
pub fn stop_playback() {
    if let Some(players) = PLAYBACK.get() {
        if let Ok(mut players) = players.lock() {
            for mut player in players.drain(..) {
                let _ = player.child.kill();
                let _ = player.child.wait();
            }
        }
    }
}

fn audio_only(path: &str) -> bool {
    matches!(
        std::path::Path::new(path)
            .extension()
            .and_then(|v| v.to_str())
            .unwrap_or("")
            .to_lowercase()
            .as_str(),
        "mp3" | "wav" | "flac" | "aac" | "m4a" | "ogg" | "opus" | "wma" | "aiff" | "aif"
    )
}
pub fn playback_active() -> bool {
    PLAYBACK
        .get()
        .is_some_and(|players| players.lock().is_ok_and(|p| !p.is_empty()))
}
pub fn playback_errors() -> Vec<String> {
    let mut errors = vec![];
    if let Some(players) = PLAYBACK.get() {
        if let Ok(mut players) = players.lock() {
            let mut i = 0;
            while i < players.len() {
                if let Ok(Some(status)) = players[i].child.try_wait() {
                    let player = players.remove(i);
                    use std::io::Read;
                    let mut text = String::new();
                    if let Ok(file) = std::fs::File::open(player.log) {
                        let _ = file.take(65536).read_to_string(&mut text);
                    }
                    text = playback_message(&text);
                    if !status.success() || !text.trim().is_empty() {
                        errors.push(format!(
                            "Media playback failed: {}",
                            if text.trim().is_empty() {
                                status.to_string()
                            } else {
                                text.trim().to_owned()
                            }
                        ));
                    }
                } else {
                    i += 1;
                }
            }
        }
    }
    errors
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
static DRAG_REGIONS: std::sync::OnceLock<std::sync::Mutex<Vec<[i32; 4]>>> =
    std::sync::OnceLock::new();
pub fn clear_drag_regions() {
    DRAG_REGIONS
        .get_or_init(Default::default)
        .lock()
        .unwrap()
        .clear();
}
pub fn add_drag_region(rect: [f32; 4], scale: f32) {
    DRAG_REGIONS
        .get_or_init(Default::default)
        .lock()
        .unwrap()
        .push(rect.map(|v| (v * scale).round() as i32));
}
#[cfg(windows)]
unsafe extern "system" fn window_hit_test(
    hwnd: windows::Win32::Foundation::HWND,
    message: u32,
    wparam: windows::Win32::Foundation::WPARAM,
    lparam: windows::Win32::Foundation::LPARAM,
    _: usize,
    _: usize,
) -> windows::Win32::Foundation::LRESULT {
    use windows::Win32::{
        Foundation::{LRESULT, POINT},
        Graphics::Gdi::ScreenToClient,
        UI::{
            Shell::DefSubclassProc,
            WindowsAndMessaging::{HTCAPTION, WM_NCHITTEST},
        },
    };
    if message == windows::Win32::UI::WindowsAndMessaging::WM_SIZE
        && wparam.0 == windows::Win32::UI::WindowsAndMessaging::SIZE_MINIMIZED as usize
    {
        crate::backdrop::hide();
    }
    if message == windows::Win32::UI::WindowsAndMessaging::WM_WINDOWPOSCHANGED
        || message == windows::Win32::UI::WindowsAndMessaging::WM_ACTIVATE
    {
        crate::backdrop::follow(hwnd.0 as isize);
    }
    if message == windows::Win32::UI::WindowsAndMessaging::WM_SHOWWINDOW && wparam.0 == 0
        || message == windows::Win32::UI::WindowsAndMessaging::WM_NCDESTROY
    {
        crate::backdrop::hide();
    }
    if message == windows::Win32::UI::WindowsAndMessaging::WM_NCCALCSIZE
        && NATIVE_GLASS_MATERIAL.load(std::sync::atomic::Ordering::Relaxed)
    {
        return LRESULT(0);
    }
    if message == WM_NCHITTEST {
        let mut p = POINT {
            x: lparam.0 as i16 as i32,
            y: (lparam.0 >> 16) as i16 as i32,
        };
        if unsafe { ScreenToClient(hwnd, &mut p) }.as_bool()
            && DRAG_REGIONS.get().is_some_and(|regions| {
                regions.try_lock().is_ok_and(|regions| {
                    regions
                        .iter()
                        .any(|r| p.x >= r[0] && p.y >= r[1] && p.x < r[2] && p.y < r[3])
                })
            })
        {
            return LRESULT(HTCAPTION as isize);
        }
    }
    unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
}

/// Start the Windows move loop directly while the pointer button is down.
/// A queued viewport drag command can arrive after input/capture state changes.
#[cfg(windows)]
pub fn drag_main_window() {
    use windows::Win32::{
        Foundation::{HWND, LPARAM, POINT, WPARAM},
        UI::{Input::KeyboardAndMouse::ReleaseCapture, WindowsAndMessaging::*},
    };
    let value = MAIN_WINDOW.load(std::sync::atomic::Ordering::Relaxed);
    if value == 0 {
        return;
    }
    unsafe {
        let mut cursor = POINT::default();
        if GetCursorPos(&mut cursor).is_ok() {
            let _ = ReleaseCapture();
            let packed = (cursor.x as u16 as u32) | ((cursor.y as u16 as u32) << 16);
            let _ = PostMessageW(
                Some(HWND(value as *mut _)),
                WM_NCLBUTTONDOWN,
                WPARAM(HTCAPTION as usize),
                LPARAM(packed as i32 as isize),
            );
        }
    }
}
#[cfg(not(windows))]
pub fn drag_main_window() {}

pub fn register_main_window(hwnd: isize) {
    MAIN_WINDOW.store(hwnd, std::sync::atomic::Ordering::Relaxed);
    #[cfg(windows)]
    unsafe {
        let _ = windows::Win32::UI::Shell::SetWindowSubclass(
            windows::Win32::Foundation::HWND(hwnd as *mut _),
            Some(window_hit_test),
            0x5644,
            0,
        );
    }
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

#[cfg(all(test, windows))]
mod resource_tests {
    #[test]
    #[ignore]
    fn owned_cpu_helper() {
        let start = std::time::Instant::now();
        while start.elapsed() < std::time::Duration::from_millis(250) {
            std::hint::black_box((1..1000u64).sum::<u64>());
        }
    }
    #[test]
    fn accounting_keeps_finished_helpers_and_tracks_their_pids() {
        let before = super::owned_usage().0;
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "platform::resource_tests::owned_cpu_helper",
                "--exact",
                "--ignored",
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        crate::content::hidden(&mut command);
        let mut child = command.spawn().unwrap();
        let job = super::owned_process(&child).unwrap();
        assert!(super::owned_usage().1.contains(&child.id()));
        assert!(child.wait().unwrap().success());
        let during = super::owned_usage().0;
        assert!(during > before + 500_000);
        drop(job);
        assert!(super::owned_usage().0 >= during);
    }
}

#[cfg(all(test, windows))]
mod clipboard_tests {
    #[test]
    fn windows_reads_unicode_file_drop_payload() {
        use windows::Win32::{Foundation::GlobalFree, System::Memory::*, UI::Shell::*};
        let path = "C:\\test\\file λ.txt";
        let payload = super::file_drop_payload(path);
        unsafe {
            let memory = GlobalAlloc(GMEM_MOVEABLE, payload.len()).unwrap();
            let data = GlobalLock(memory) as *mut u8;
            assert!(!data.is_null());
            std::ptr::copy_nonoverlapping(payload.as_ptr(), data, payload.len());
            let _ = GlobalUnlock(memory);
            let drop = HDROP(memory.0);
            let mut buffer = vec![0u16; 32768];
            assert_eq!(DragQueryFileW(drop, u32::MAX, None), 1);
            let length = DragQueryFileW(drop, 0, Some(&mut buffer));
            assert_eq!(String::from_utf16_lossy(&buffer[..length as usize]), path);
            let _ = GlobalFree(Some(memory));
        }
    }
}

#[cfg(test)]
mod audio_tests {
    #[test]
    fn audio_preview_skips_album_art_and_display() {
        assert!(super::audio_only("podcast.MP3"));
        assert!(super::audio_only("note.opus"));
        assert!(!super::audio_only("clip.mp4"));
    }
}

fn playback_message(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == char::from(27) {
            if chars.peek() == Some(&'[') {
                chars.next();
                for c in chars.by_ref() {
                    if ('@'..='~').contains(&c) {
                        break;
                    }
                }
            }
        } else if !c.is_control() || c == '\n' || c == '\t' {
            out.push(c);
        }
    }
    out.trim().to_owned()
}
#[cfg(test)]
mod playback_diagnostics_tests {
    #[test]
    fn terminal_controls_are_not_playback_errors() {
        let esc = char::from(27);
        assert!(super::playback_message(&format!("{esc}[2K")).is_empty());
        assert_eq!(
            super::playback_message(&format!("{esc}[31mAudio device failed{esc}[0m")),
            "Audio device failed"
        );
    }
}

static GAME_NAMES: std::sync::OnceLock<std::sync::Mutex<Vec<String>>> = std::sync::OnceLock::new();
pub fn set_game_names(names: &str) {
    *GAME_NAMES.get_or_init(Default::default).lock().unwrap() = names
        .split([',', ';', '\n'])
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .collect();
}
fn fullscreen_geometry(
    maximized: bool,
    captioned: bool,
    bounds: [i32; 4],
    monitor: [i32; 4],
) -> bool {
    !maximized
        && !captioned
        && bounds[0] <= monitor[0]
        && bounds[1] <= monitor[1]
        && bounds[2] >= monitor[2]
        && bounds[3] >= monitor[3]
}
#[cfg(windows)]
pub fn game_or_fullscreen() -> (bool, bool) {
    use windows::Win32::{
        Foundation::{CloseHandle, HWND, RECT},
        Graphics::Gdi::{
            GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
        },
        System::{
            Diagnostics::ToolHelp::*,
            Threading::{
                OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
                QueryFullProcessImageNameW,
            },
        },
        UI::{
            Shell::{QUNS_RUNNING_D3D_FULL_SCREEN, SHQueryUserNotificationState},
            WindowsAndMessaging::*,
        },
    };
    static LAST: std::sync::OnceLock<std::sync::Mutex<(isize, bool)>> = std::sync::OnceLock::new();
    unsafe {
        let mut game = false;
        let names = GAME_NAMES
            .get_or_init(Default::default)
            .lock()
            .unwrap()
            .clone();
        if !names.is_empty() {
            if let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) {
                let mut entry = PROCESSENTRY32W {
                    dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
                    ..Default::default()
                };
                if Process32FirstW(snapshot, &mut entry).is_ok() {
                    loop {
                        let name = String::from_utf16_lossy(
                            &entry.szExeFile[..entry
                                .szExeFile
                                .iter()
                                .position(|v| *v == 0)
                                .unwrap_or(entry.szExeFile.len())],
                        )
                        .to_lowercase();
                        if names.contains(&name) {
                            game = true;
                            break;
                        }
                        if Process32NextW(snapshot, &mut entry).is_err() {
                            break;
                        }
                    }
                }
                let _ = CloseHandle(snapshot);
            }
        }
        let mut hwnd = GetForegroundWindow();
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let mut last = LAST.get_or_init(Default::default).lock().unwrap();
        let own = pid == std::process::id();
        if own {
            hwnd = HWND(last.0 as *mut _);
        }
        if hwnd.0.is_null()
            || !IsWindow(Some(hwnd)).as_bool()
            || !IsWindowVisible(hwnd).as_bool()
            || IsIconic(hwnd).as_bool()
        {
            return (game, false);
        }
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let mut class = [0u16; 128];
        let len = GetClassNameW(hwnd, &mut class);
        let class = String::from_utf16_lossy(&class[..len.max(0) as usize]);
        if ["Progman", "WorkerW", "Shell_TrayWnd"].contains(&class.as_str()) {
            return (game, false);
        }
        let notification = if own {
            None
        } else {
            SHQueryUserNotificationState().ok()
        };
        let d3d = notification == Some(QUNS_RUNNING_D3D_FULL_SCREEN);
        let mut rect = RECT::default();
        let mut monitor = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        let fullscreen = GetWindowRect(hwnd, &mut rect).is_ok()
            && GetMonitorInfoW(
                MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST),
                &mut monitor,
            )
            .as_bool()
            && fullscreen_geometry(
                IsZoomed(hwnd).as_bool(),
                GetWindowLongW(hwnd, GWL_STYLE) as u32 & WS_CAPTION.0 != 0,
                [rect.left, rect.top, rect.right, rect.bottom],
                [
                    monitor.rcMonitor.left,
                    monitor.rcMonitor.top,
                    monitor.rcMonitor.right,
                    monitor.rcMonitor.bottom,
                ],
            );
        if let Ok(process) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
            let mut path = [0u16; 32768];
            let mut size = path.len() as u32;
            if QueryFullProcessImageNameW(
                process,
                PROCESS_NAME_WIN32,
                windows::core::PWSTR(path.as_mut_ptr()),
                &mut size,
            )
            .is_ok()
            {
                let path = String::from_utf16_lossy(&path[..size as usize]).to_lowercase();
                game |= path.contains("\\steamapps\\")
                    || path.contains("\\epic games\\")
                    || path.contains("\\gog games\\");
            }
            let _ = CloseHandle(process);
        }
        game |= d3d;
        if !own {
            *last = (hwnd.0 as isize, game);
        }
        (game, fullscreen || d3d)
    }
}
#[cfg(not(windows))]
pub fn game_or_fullscreen() -> (bool, bool) {
    (false, false)
}

#[cfg(windows)]
pub fn clipboard_sequence() -> u32 {
    unsafe { windows::Win32::System::DataExchange::GetClipboardSequenceNumber() }
}
#[cfg(not(windows))]
pub fn clipboard_sequence() -> u32 {
    0
}
#[cfg(windows)]
pub fn clipboard_allowed(excluded: &str) -> Result<bool> {
    use windows::Win32::{
        Foundation::{CloseHandle, HGLOBAL},
        System::{DataExchange::*, Memory::*, Threading::*},
        UI::WindowsAndMessaging::*,
    };
    unsafe {
        if let Some(owner) = GetClipboardOwner().ok() {
            let mut pid = 0;
            GetWindowThreadProcessId(owner, Some(&mut pid));
            if let Ok(process) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
                let mut path = [0u16; 32768];
                let mut size = path.len() as u32;
                let ok = QueryFullProcessImageNameW(
                    process,
                    PROCESS_NAME_WIN32,
                    windows::core::PWSTR(path.as_mut_ptr()),
                    &mut size,
                )
                .is_ok();
                let _ = CloseHandle(process);
                if ok {
                    let name = String::from_utf16_lossy(&path[..size as usize])
                        .split('\\')
                        .next_back()
                        .unwrap_or("")
                        .to_lowercase();
                    if excluded
                        .split([',', ';', '\n'])
                        .any(|v| v.trim().eq_ignore_ascii_case(&name))
                    {
                        return Ok(false);
                    }
                }
            }
        }
        OpenClipboard(None)?;
        struct Guard;
        impl Drop for Guard {
            fn drop(&mut self) {
                unsafe {
                    let _ = CloseClipboard();
                }
            }
        }
        let _guard = Guard;
        for (name, zero_means_skip) in [
            ("ExcludeClipboardContentFromMonitorProcessing", false),
            ("CanIncludeInClipboardHistory", true),
        ] {
            let format = RegisterClipboardFormatW(windows::core::PCWSTR(wide(name).as_ptr()));
            if let Ok(data) = GetClipboardData(format) {
                let handle = HGLOBAL(data.0);
                if GlobalSize(handle) >= 4 {
                    let ptr = GlobalLock(handle) as *const u32;
                    if !ptr.is_null() {
                        let value = ptr.read_unaligned();
                        let _ = GlobalUnlock(handle);
                        if if zero_means_skip {
                            value == 0
                        } else {
                            value != 0
                        } {
                            return Ok(false);
                        }
                    }
                }
            }
        }
        Ok(true)
    }
}
#[cfg(windows)]
pub fn clipboard_text(excluded: &str) -> Result<Option<String>> {
    if !clipboard_allowed(excluded)? {
        return Ok(None);
    }
    use windows::Win32::{
        Foundation::HGLOBAL,
        System::{DataExchange::*, Memory::*},
    };
    unsafe {
        OpenClipboard(None)?;
        struct Guard;
        impl Drop for Guard {
            fn drop(&mut self) {
                unsafe {
                    let _ = CloseClipboard();
                }
            }
        }
        let _guard = Guard;
        let Ok(data) = GetClipboardData(13) else {
            return Ok(None);
        };
        let handle = HGLOBAL(data.0);
        let size = GlobalSize(handle);
        if size == 0 || size > crate::launcher::MAX_TEXT * 2 + 2 {
            return Ok(None);
        }
        let ptr = GlobalLock(handle) as *const u16;
        if ptr.is_null() {
            return Ok(None);
        }
        let chars = std::slice::from_raw_parts(ptr, size / 2);
        let length = chars.iter().position(|v| *v == 0).unwrap_or(chars.len());
        let value = String::from_utf16_lossy(&chars[..length]);
        let _ = GlobalUnlock(handle);
        if value.len() > crate::launcher::MAX_TEXT {
            return Ok(None);
        }
        Ok(Some(value))
    }
}
#[cfg(not(windows))]
pub fn clipboard_text(_: &str) -> Result<Option<String>> {
    Ok(None)
}

#[cfg(windows)]
pub fn left_button_down() -> bool {
    unsafe { windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(1) as u16 & 0x8000 != 0 }
}
#[cfg(not(windows))]
pub fn left_button_down() -> bool {
    false
}

#[cfg(not(windows))]
pub fn clipboard_allowed(_: &str) -> Result<bool> {
    Ok(false)
}

#[cfg(windows)]
pub fn hide_main_window() {
    let value = MAIN_WINDOW.load(std::sync::atomic::Ordering::Acquire);
    if value != 0 {
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::ShowWindow(
                windows::Win32::Foundation::HWND(value as *mut _),
                windows::Win32::UI::WindowsAndMessaging::SW_HIDE,
            );
        }
    }
}
#[cfg(not(windows))]
pub fn hide_main_window() {}

#[cfg(test)]
mod fullscreen_tests {
    #[test]
    fn maximized_desktop_windows_do_not_pause_indexing() {
        let screen = [0, 0, 1920, 1080];
        assert!(!super::fullscreen_geometry(true, false, screen, screen));
        assert!(!super::fullscreen_geometry(false, true, screen, screen));
        assert!(!super::fullscreen_geometry(
            false,
            false,
            [100, 100, 1700, 900],
            screen
        ));
        assert!(super::fullscreen_geometry(false, false, screen, screen));
    }
}

pub fn main_window_visible() -> bool {
    #[cfg(windows)]
    unsafe {
        let value = MAIN_WINDOW.load(std::sync::atomic::Ordering::Acquire);
        if value == 0 {
            return true;
        }
        let hwnd = windows::Win32::Foundation::HWND(value as *mut _);
        return windows::Win32::UI::WindowsAndMessaging::IsWindowVisible(hwnd).as_bool()
            && !windows::Win32::UI::WindowsAndMessaging::IsIconic(hwnd).as_bool();
    }
    #[cfg(not(windows))]
    {
        true
    }
}

/// Read the real build without manifest-sensitive GetVersionEx behavior.
#[cfg(windows)]
pub fn windows_build() -> u32 {
    static BUILD: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
    if development_overrides() {
        if let Ok(value) = std::env::var("VECTORDASH_TEST_WINDOWS_BUILD") {
            if let Ok(build) = value.parse() {
                return build;
            }
        }
    }
    *BUILD.get_or_init(|| unsafe {
        use windows::{
            Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress},
            core::{PCSTR, w},
        };
        #[repr(C)]
        struct Version {
            size: u32,
            major: u32,
            minor: u32,
            build: u32,
            platform: u32,
            service: [u16; 128],
        }
        let Ok(module) = GetModuleHandleW(w!("ntdll.dll")) else {
            return 0;
        };
        let Some(function) = GetProcAddress(module, PCSTR(c"RtlGetVersion".as_ptr().cast())) else {
            return 0;
        };
        let function: unsafe extern "system" fn(*mut Version) -> i32 =
            std::mem::transmute(function);
        let mut version = Version {
            size: std::mem::size_of::<Version>() as u32,
            major: 0,
            minor: 0,
            build: 0,
            platform: 0,
            service: [0; 128],
        };
        if function(&mut version) >= 0 {
            version.build
        } else {
            0
        }
    })
}
#[cfg(not(windows))]
pub fn windows_build() -> u32 {
    0
}
pub fn development_overrides() -> bool {
    cfg!(debug_assertions) || std::env::args().any(|a| a == "--allow-development-overrides")
}

/// Test the real Shell callback without touching another running instance.
#[cfg(windows)]
pub fn smoke_tray_open(
    tx: crate::engine::NotifySender,
    legacy: bool,
    accent: [u8; 3],
) -> Result<()> {
    use windows::Win32::{
        Foundation::{LPARAM, WPARAM},
        UI::{Shell::*, WindowsAndMessaging::*},
    };
    let mut visual = crate::tray::Visual {
        status: crate::tray::Status::Idle,
        accent,
    };
    crate::tray::publish(visual);
    let (hwnd, mut data) = tray_window(tx, visual)?;
    unsafe {
        // Simulate Explorer losing registration; do not restart the user's Explorer.
        let _ = Shell_NotifyIconW(NIM_DELETE, &data);
        let message = *TASKBAR_CREATED.get().unwrap();
        SendMessageW(hwnd, message, Some(WPARAM(0)), Some(LPARAM(0)));
        anyhow::ensure!(
            TRAY_READD.swap(false, std::sync::atomic::Ordering::Relaxed),
            "Explorer-restart callback not received"
        );
        refresh_tray(&mut data, &mut visual, true)?;
        for state in [
            crate::tray::Status::Waiting,
            crate::tray::Status::Indexing,
            crate::tray::Status::Idle,
        ] {
            let old = data.hIcon;
            crate::tray::publish(crate::tray::Visual {
                status: state,
                accent,
            });
            refresh_tray(&mut data, &mut visual, false)?;
            anyhow::ensure!(data.hIcon != old, "State transition did not update icon");
        }
        let payload = if legacy {
            WM_LBUTTONDBLCLK
        } else {
            (1 << 16) | WM_LBUTTONDBLCLK
        };
        SendMessageW(
            hwnd,
            WM_APP + 9,
            Some(WPARAM(1)),
            Some(LPARAM(payload as isize)),
        );
        let _ = Shell_NotifyIconW(NIM_DELETE, &data);
        let _ = DestroyIcon(data.hIcon);
        let _ = DestroyWindow(hwnd);
    }
    Ok(())
}
#[cfg(not(windows))]
pub fn smoke_tray_open(tx: crate::engine::NotifySender, _: bool, _: [u8; 3]) -> Result<()> {
    let _ = tx.try_send(crate::engine::Event::Full);
    Ok(())
}

pub fn windows_key_down() -> bool {
    #[cfg(windows)]
    {
        use windows::Win32::UI::Input::KeyboardAndMouse::*;
        unsafe { GetAsyncKeyState(VK_LWIN.0 as i32) < 0 || GetAsyncKeyState(VK_RWIN.0 as i32) < 0 }
    }
    #[cfg(not(windows))]
    {
        false
    }
}

pub fn paste_to_window(target: isize) -> Result<()> {
    #[cfg(windows)]
    unsafe {
        use windows::Win32::{
            Foundation::HWND,
            UI::{Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
        };
        let hwnd = HWND(target as *mut _);
        anyhow::ensure!(
            IsWindow(Some(hwnd)).as_bool(),
            "The previous application is no longer available"
        );
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        let _ = SetForegroundWindow(hwnd);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
        while std::time::Instant::now() < deadline {
            if GetForegroundWindow() == hwnd
                && [VK_CONTROL, VK_SHIFT, VK_MENU, VK_LWIN, VK_RWIN]
                    .iter()
                    .all(|key| GetAsyncKeyState(key.0 as i32) >= 0)
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        anyhow::ensure!(
            GetForegroundWindow() == hwnd,
            "Could not focus the previous application; item remains copied"
        );
        anyhow::ensure!(
            [VK_CONTROL, VK_SHIFT, VK_MENU, VK_LWIN, VK_RWIN]
                .iter()
                .all(|key| GetAsyncKeyState(key.0 as i32) >= 0),
            "Release modifier keys to paste; item remains copied"
        );
        let keys = [
            (VK_CONTROL, false),
            (VIRTUAL_KEY(0x56), false),
            (VIRTUAL_KEY(0x56), true),
            (VK_CONTROL, true),
        ]
        .map(|(key, up)| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: key,
                    dwFlags: if up {
                        KEYEVENTF_KEYUP
                    } else {
                        KEYBD_EVENT_FLAGS(0)
                    },
                    ..Default::default()
                },
            },
        });
        anyhow::ensure!(
            SendInput(&keys, std::mem::size_of::<INPUT>() as i32) == keys.len() as u32,
            "Windows blocked paste input; item remains copied"
        );
        return Ok(());
    }
    #[cfg(not(windows))]
    {
        let _ = target;
        anyhow::bail!("Paste is supported on Windows")
    }
}

#[cfg(windows)]
fn desktop_shell() -> Result<windows::Win32::UI::Shell::IShellDispatch2> {
    use windows::{
        Win32::{
            System::{Com::*, Variant::VARIANT},
            UI::Shell::*,
        },
        core::Interface,
    };
    unsafe {
        let windows: IShellWindows = CoCreateInstance(&ShellWindows, None, CLSCTX_LOCAL_SERVER)?;
        let empty = VARIANT::default();
        let mut hwnd = 0;
        let desktop =
            windows.FindWindowSW(&empty, &empty, SWC_DESKTOP, &mut hwnd, SWFO_NEEDDISPATCH)?;
        let provider: IServiceProvider = desktop.cast().context("Desktop service provider")?;
        let browser: IShellBrowser = provider
            .QueryService(&SID_STopLevelBrowser)
            .context("Desktop shell browser")?;
        let view = browser
            .QueryActiveShellView()
            .context("Desktop shell view")?;
        let dispatch: IDispatch = view
            .GetItemObject(SVGIO_BACKGROUND)
            .context("Desktop background automation")?;
        let folder: IShellFolderViewDual = dispatch.cast().context("Desktop folder automation")?;
        Ok(folder
            .Application()?
            .cast()
            .context("Desktop Shell.Application")?)
    }
}
#[cfg(windows)]
fn open_from_desktop(path: &str) -> Result<()> {
    use windows::{Win32::System::Variant::VARIANT, core::BSTR};
    unsafe {
        desktop_shell()?.ShellExecute(
            &BSTR::from(path),
            &VARIANT::from(""),
            &VARIANT::from(""),
            &VARIANT::from("open"),
            &VARIANT::from(1i32),
        )?;
    }
    Ok(())
}

#[cfg(all(test, windows))]
mod desktop_launch_tests {
    #[test]
    #[ignore = "Requires an interactive Explorer desktop; opens the explicitly supplied test folder"]
    fn desktop_broker_opens_mapped_folder() {
        use windows::Win32::System::Com::*;
        let path = std::env::var("VECTORDASH_TEST_OPEN_PATH").expect("Set an explicit test folder");
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().unwrap();
        }
        super::open_from_desktop(&path).unwrap();
        unsafe {
            CoUninitialize();
        }
    }
}

#[cfg(all(test, windows))]
mod shortcut_modifier_tests {
    #[test]
    fn windows_modifier_is_registered_not_dropped() {
        use windows::Win32::UI::Input::KeyboardAndMouse::*;
        let (mods, key) = super::shortcut("Ctrl+Win+K").unwrap();
        assert_eq!(key, 0x4b);
        assert!(mods.contains(MOD_WIN));
        assert!(mods.contains(MOD_CONTROL));
        let modifiers = eframe::egui::Modifiers {
            ctrl: true,
            mac_cmd: true,
            ..Default::default()
        };
        assert_eq!(
            crate::ai::shortcut(eframe::egui::Key::K, modifiers),
            "Ctrl+Win+K"
        );
    }
}
