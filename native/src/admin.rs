use anyhow::{Context, Result};
use std::path::Path;

pub fn should_elevate(always: bool, elevated: bool, args: &[String]) -> bool {
    always
        && !elevated
        && !args
            .iter()
            .any(|a| a == "--admin-relaunch" || a == "--smoke-report")
}
fn quote(value: &str) -> String {
    let mut out = String::from("\"");
    let mut slashes = 0;
    for ch in value.chars() {
        if ch == '\\' {
            slashes += 1;
            continue;
        }
        out.extend(std::iter::repeat_n(
            '\\',
            if ch == '"' { slashes * 2 + 1 } else { slashes },
        ));
        slashes = 0;
        out.push(ch);
    }
    out.extend(std::iter::repeat_n('\\', slashes * 2));
    out.push('"');
    out
}
fn arguments(home: &Path, args: &[String], pid: u32) -> String {
    let mut result = vec![
        "--admin-relaunch".to_owned(),
        "--admin-data-home".into(),
        home.to_string_lossy().into_owned(),
        "--wait-for-process".into(),
        pid.to_string(),
    ];
    // Forward only supported GUI launch state, never a worker/test invocation.
    result.extend(
        args.iter()
            .filter(|a| matches!(a.as_str(), "--startup" | "--portable"))
            .cloned(),
    );
    result
        .iter()
        .map(|a| quote(a))
        .collect::<Vec<_>>()
        .join(" ")
}
#[cfg(windows)]
pub fn elevated() -> bool {
    use windows::Win32::{
        Foundation::{CloseHandle, HANDLE},
        Security::{GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation},
        System::Threading::{GetCurrentProcess, OpenProcessToken},
    };
    unsafe {
        let mut token = HANDLE::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_err() {
            return false;
        }
        let mut value = TOKEN_ELEVATION::default();
        let mut size = 0;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            Some(&mut value as *mut _ as _),
            std::mem::size_of_val(&value) as u32,
            &mut size,
        )
        .is_ok();
        let _ = CloseHandle(token);
        ok && value.TokenIsElevated != 0
    }
}
#[cfg(not(windows))]
pub fn elevated() -> bool {
    false
}

#[cfg(windows)]
pub fn relaunch(home: &Path, args: &[String]) -> Result<()> {
    use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};
    use windows::{
        Win32::UI::{
            Shell::{SEE_MASK_NOASYNC, SHELLEXECUTEINFOW, ShellExecuteExW},
            WindowsAndMessaging::SW_SHOWNORMAL,
        },
        core::PCWSTR,
    };
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
    let wide = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    let exe = wide(&std::env::current_exe()?.to_string_lossy());
    let verb = wide("runas");
    let params = wide(&arguments(home, args, std::process::id()));
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOASYNC,
        lpVerb: PCWSTR(verb.as_ptr()),
        lpFile: PCWSTR(exe.as_ptr()),
        lpParameters: PCWSTR(params.as_ptr()),
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };
    unsafe { ShellExecuteExW(&mut info) }
        .context("Administrator relaunch was canceled or failed")?;
    Ok(())
}
#[cfg(not(windows))]
pub fn relaunch(_: &Path, _: &[String]) -> Result<()> {
    anyhow::bail!("Administrator relaunch requires Windows")
}

pub fn prepare_handoff(args: &[String]) -> Result<()> {
    if !args.iter().any(|a| a == "--admin-relaunch") {
        return Ok(());
    }
    let value = |flag: &str| {
        args.iter()
            .position(|a| a == flag)
            .and_then(|i| args.get(i + 1))
    };
    if let Some(home) = value("--admin-data-home") {
        anyhow::ensure!(
            Path::new(home).is_absolute(),
            "Administrator profile path must be absolute"
        );
        unsafe {
            std::env::set_var("VECTORDASH_DATA_HOME", home);
        }
    }
    if let Some(pid) = value("--wait-for-process") {
        wait_parent(pid.parse()?)?;
    }
    Ok(())
}
#[cfg(windows)]
fn wait_parent(pid: u32) -> Result<()> {
    use windows::Win32::{
        Foundation::{CloseHandle, ERROR_INVALID_PARAMETER, WAIT_OBJECT_0},
        System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject},
    };
    anyhow::ensure!(pid != std::process::id(), "Cannot wait for current process");
    let handle = match unsafe { OpenProcess(PROCESS_SYNCHRONIZE, false, pid) } {
        Ok(h) => h,
        Err(e) if e.code() == ERROR_INVALID_PARAMETER.to_hresult() => return Ok(()),
        Err(e) => return Err(e).context("Could not wait for the previous VectorDash process"),
    };
    let status = unsafe { WaitForSingleObject(handle, 30_000) };
    unsafe {
        let _ = CloseHandle(handle);
    }
    anyhow::ensure!(
        status == WAIT_OBJECT_0,
        "Previous VectorDash process did not exit; administrator relaunch stopped"
    );
    Ok(())
}
#[cfg(not(windows))]
fn wait_parent(_: u32) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn elevation_policy_avoids_prompt_loops_and_smoke_tests() {
        assert!(should_elevate(true, false, &[]));
        assert!(!should_elevate(false, false, &[]));
        assert!(!should_elevate(true, true, &[]));
        assert!(!should_elevate(true, false, &["--admin-relaunch".into()]));
        assert!(!should_elevate(
            true,
            false,
            &["--smoke-report".into(), "report.json".into()]
        ));
    }
    #[test]
    fn administrator_preference_defaults_off_and_round_trips() {
        let old: crate::config::Settings = serde_json::from_str("{}").unwrap();
        assert!(!old.always_admin);
        let mut enabled = old;
        enabled.always_admin = true;
        let restored: crate::config::Settings =
            serde_json::from_str(&serde_json::to_string(&enabled).unwrap()).unwrap();
        assert!(restored.always_admin);
    }
    #[cfg(windows)]
    #[test]
    fn handoff_waits_for_the_previous_process_to_exit() {
        let mut command = std::process::Command::new("cmd.exe");
        command.args(["/d", "/c", "exit", "0"]);
        crate::content::hidden(&mut command);
        let mut child = command.spawn().unwrap();
        wait_parent(child.id()).unwrap();
        assert!(child.wait().unwrap().success());
    }
    #[cfg(windows)]
    #[test]
    fn launch_arguments_round_trip_windows_paths_and_unicode() {
        use windows::{
            Win32::{
                Foundation::{HLOCAL, LocalFree},
                UI::Shell::CommandLineToArgvW,
            },
            core::PCWSTR,
        };
        let values = [
            "app",
            "",
            r"C:\",
            r"C:\My profile\",
            "C:\\Tệp dữ liệu\\模型",
            "quote\"test",
            "space and & symbols",
        ];
        let command = values
            .iter()
            .map(|a| quote(a))
            .collect::<Vec<_>>()
            .join(" ");
        let wide: Vec<_> = command.encode_utf16().chain(Some(0)).collect();
        unsafe {
            let mut count = 0;
            let parsed = CommandLineToArgvW(PCWSTR(wide.as_ptr()), &mut count);
            assert!(!parsed.is_null());
            let actual: Vec<String> = std::slice::from_raw_parts(parsed, count as usize)
                .iter()
                .map(|p| p.to_string().unwrap())
                .collect();
            let _ = LocalFree(Some(HLOCAL(parsed as _)));
            assert_eq!(actual, values);
        }
        let params = arguments(
            Path::new(r"C:\My profile"),
            &["--startup".into(), "--extract-worker".into()],
            42,
        );
        assert!(params.contains("--startup") && !params.contains("--extract-worker"));
    }
}
