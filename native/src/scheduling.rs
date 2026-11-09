use crate::config::Settings;
use chrono::{DateTime, Datelike, Local, TimeZone, Timelike};
use std::sync::{Mutex, OnceLock};
#[derive(Clone, Copy, Debug, Default)]
pub struct Metrics {
    pub cpu: Option<f32>,
    pub gpu: Option<f32>,
    pub owned_cpu: Option<f32>,
    pub owned_gpu: Option<f32>,
    pub idle: u64,
    pub on_ac: Option<bool>,
    pub game: bool,
    pub fullscreen: bool,
}
static METRICS: OnceLock<Mutex<Metrics>> = OnceLock::new();
pub fn metrics() -> Metrics {
    *METRICS
        .get_or_init(|| Mutex::new(Metrics::default()))
        .lock()
        .unwrap()
}
pub fn resource_pause(s: &Settings, m: Metrics) -> Option<&'static str> {
    if s.pause_for_game && m.game {
        Some("A game is running")
    } else if s.pause_for_fullscreen && m.fullscreen {
        Some("A full-screen application is running")
    } else {
        None
    }
}
pub fn model_hold(s: &Settings, m: Metrics) -> Option<&'static str> {
    if s.remote_embeddings() {
        return None;
    }
    if let Some(reason) = resource_pause(s, m) {
        return Some(reason);
    }
    if s.defer_search_model {
        let mut limits = s.clone();
        // Typing a search is activity; resource limits should not require typing to stop.
        limits.check_user_idle = false;
        idle_allowed(&limits, m).err()
    } else {
        None
    }
}
pub fn idle_allowed(s: &Settings, m: Metrics) -> Result<(), &'static str> {
    if let Some(reason) = resource_pause(s, m) {
        return Err(reason);
    }
    if s.check_user_idle && m.idle < s.idle_seconds as u64 {
        return Err("Waiting for keyboard/mouse inactivity");
    }
    if s.check_cpu {
        match m.cpu {
            None => return Err("CPU monitoring unavailable"),
            Some(v) if v > s.cpu_limit => return Err("Waiting for CPU usage to fall"),
            _ => {}
        }
    }
    if s.check_gpu {
        match m.gpu {
            None => return Err("GPU monitoring unavailable; disable its condition to proceed"),
            Some(v) if v > s.gpu_limit => return Err("Waiting for GPU usage to fall"),
            _ => {}
        }
    }
    if s.ac_only && m.on_ac != Some(true) {
        return Err("Waiting for AC power");
    }
    Ok(())
}
pub fn due(s: &Settings, now: DateTime<Local>, m: Metrics) -> Result<i64, String> {
    due_impl(s, now, m, true)
}
pub fn work_due(s: &Settings, now: DateTime<Local>) -> bool {
    due_impl(s, now, Metrics::default(), false).is_ok()
}
fn due_impl(
    s: &Settings,
    now: DateTime<Local>,
    m: Metrics,
    conditions: bool,
) -> Result<i64, String> {
    if let Some(reason) = resource_pause(s, m).filter(|_| conditions) {
        return Err(reason.into());
    }
    if s.schedule == "Manual" {
        return Err("Periodic scan scheduling is off".into());
    }
    if s.schedule == "When idle" {
        if now.timestamp() < s.last_success + s.idle_interval_minutes as i64 * 60 {
            return Err("Waiting for the next idle interval".into());
        }
        if conditions {
            idle_allowed(s, m).map_err(str::to_string)?;
        }
        return Ok(now.timestamp());
    }
    let mut slot = None;
    for offset in 0..8 {
        let date = now.date_naive() - chrono::Duration::days(offset);
        if !s.schedule_days[date.weekday().num_days_from_monday() as usize] {
            continue;
        }
        if let Some(time) = date.and_hms_opt(s.schedule_hour.min(23), s.schedule_minute.min(59), 0)
        {
            if let Some(candidate) = Local.from_local_datetime(&time).earliest() {
                if candidate <= now {
                    slot = Some(candidate.timestamp());
                    break;
                }
            }
        }
    }
    let slot = slot.ok_or("Waiting for the selected day/time")?;
    if slot < s.schedule_enabled_at || slot <= s.last_schedule_slot {
        return Err("Waiting for the next scheduled time".into());
    }
    if conditions && s.ac_only && m.on_ac != Some(true) {
        return Err("Waiting for AC power".into());
    }
    if s.schedule == "Time + idle" {
        if conditions {
            idle_allowed(s, m).map_err(str::to_string)?;
        }
    }
    Ok(slot)
}
pub fn start_monitor(settings: std::sync::Arc<Mutex<Settings>>) {
    static STARTED: OnceLock<()> = OnceLock::new();
    STARTED.get_or_init(|| {
        std::thread::spawn(move || {
            #[cfg(windows)]
            let mut sampler = Sampler::new();
            loop {
                #[cfg(windows)]
                let monitor_settings = settings.lock().unwrap().clone();
                #[cfg(windows)]
                let mut value = sampler.sample(&monitor_settings);
                #[cfg(not(windows))]
                let mut value = Metrics::default();
                let (game, fullscreen) = crate::platform::game_or_fullscreen();
                value.game = game;
                value.fullscreen = fullscreen;
                *METRICS
                    .get_or_init(|| Mutex::new(Metrics::default()))
                    .lock()
                    .unwrap() = value;
                std::thread::sleep(std::time::Duration::from_secs(2));
            }
        });
    });
}
fn external_cpu(total: u64, busy: u64, owned: u64) -> f32 {
    if total == 0 {
        return 0.0;
    }
    (100.0 * busy.saturating_sub(owned) as f64 / total as f64).clamp(0.0, 100.0) as f32
}
fn gpu_pid(name: &str) -> Option<u32> {
    name.strip_prefix("pid_")?.split('_').next()?.parse().ok()
}
#[cfg(windows)]
struct Sampler {
    previous: Option<(u64, u64, u64)>,
    query: windows::Win32::System::Performance::PDH_HQUERY,
    counter: windows::Win32::System::Performance::PDH_HCOUNTER,
    gpu_ready: bool,
    previous_pids: std::collections::HashSet<u32>,
    gpu_target: Option<(u32, u32)>,
    target_key: String,
    target_retry: std::time::Instant,
}
#[cfg(windows)]
impl Sampler {
    fn new() -> Self {
        use windows::{Win32::System::Performance::*, core::PCWSTR};
        let mut query = PDH_HQUERY::default();
        let mut counter = PDH_HCOUNTER::default();
        let path: Vec<u16> = "\\GPU Engine(*)\\Utilization Percentage"
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let ready = unsafe {
            PdhOpenQueryW(PCWSTR::null(), 0, &mut query) == 0
                && PdhAddEnglishCounterW(query, PCWSTR(path.as_ptr()), 0, &mut counter) == 0
        };
        if ready {
            unsafe {
                PdhCollectQueryData(query);
            }
        }
        Self {
            previous: None,
            query,
            counter,
            gpu_ready: ready,
            previous_pids: Default::default(),
            gpu_target: None,
            target_key: String::new(),
            target_retry: std::time::Instant::now(),
        }
    }
    fn sample(&mut self, s: &Settings) -> Metrics {
        let key = format!("{}|{}|{}", s.gpu_idle_device, s.hardware, s.device);
        if key != self.target_key || std::time::Instant::now() >= self.target_retry {
            let target = selected_gpu(s);
            self.gpu_target = target.as_ref().map(|a| a.luid);
            *GPU_MONITOR_NAME
                .get_or_init(Default::default)
                .lock()
                .unwrap() = target.map(|a| a.name);
            self.target_key = key;
            self.target_retry = std::time::Instant::now() + std::time::Duration::from_secs(30);
        }
        use windows::Win32::{
            Foundation::FILETIME,
            System::{
                Performance::*,
                Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS},
                Threading::GetSystemTimes,
            },
        };
        let mut m = Metrics {
            idle: crate::platform::idle_seconds(),
            ..Default::default()
        };
        let (owned_cpu, mut owned_pids) = crate::platform::owned_usage();
        let current_pids = owned_pids.clone();
        owned_pids.extend(self.previous_pids.drain());
        self.previous_pids = current_pids;
        unsafe {
            let (mut idle, mut kernel, mut user) = (
                FILETIME::default(),
                FILETIME::default(),
                FILETIME::default(),
            );
            if GetSystemTimes(Some(&mut idle), Some(&mut kernel), Some(&mut user)).is_ok() {
                let n = |f: FILETIME| ((f.dwHighDateTime as u64) << 32) | f.dwLowDateTime as u64;
                let pair = (n(idle), n(kernel) + n(user), owned_cpu);
                if let Some(old) = self.previous {
                    let total = pair.1.saturating_sub(old.1);
                    if total > 0 {
                        let busy = total.saturating_sub(pair.0.saturating_sub(old.0));
                        m.cpu = Some(external_cpu(total, busy, pair.2.saturating_sub(old.2)));
                        m.owned_cpu = Some(
                            (100.0 * pair.2.saturating_sub(old.2) as f64 / total as f64)
                                .clamp(0.0, 100.0) as f32,
                        );
                    }
                }
                self.previous = Some(pair);
            }
            let mut power = SYSTEM_POWER_STATUS::default();
            if GetSystemPowerStatus(&mut power).is_ok() {
                m.on_ac = match power.ACLineStatus {
                    0 => Some(false),
                    1 => Some(true),
                    _ => None,
                };
            }
            if self.gpu_ready && PdhCollectQueryData(self.query) == 0 {
                let (mut size, mut count) = (0, 0);
                PdhGetFormattedCounterArrayW(
                    self.counter,
                    PDH_FMT_DOUBLE,
                    &mut size,
                    &mut count,
                    None,
                );
                if size > 0 && size < 8 * 1024 * 1024 {
                    let mut data = vec![0u64; (size as usize + 7) / 8];
                    if PdhGetFormattedCounterArrayW(
                        self.counter,
                        PDH_FMT_DOUBLE,
                        &mut size,
                        &mut count,
                        Some(data.as_mut_ptr().cast()),
                    ) == 0
                    {
                        let items = std::slice::from_raw_parts(
                            data.as_ptr() as *const PDH_FMT_COUNTERVALUE_ITEM_W,
                            count as usize,
                        );
                        let counters = items
                            .iter()
                            .filter(|x| x.FmtValue.CStatus <= 1)
                            .map(|x| {
                                (
                                    x.szName.to_string().unwrap_or_default(),
                                    x.FmtValue.Anonymous.doubleValue,
                                )
                            })
                            .collect::<Vec<_>>();
                        let counters = counters
                            .into_iter()
                            .filter(|(name, _)| {
                                self.gpu_target
                                    .is_some_and(|target| gpu_luid(name) == Some(target))
                            })
                            .collect::<Vec<_>>();
                        let other = counters.iter().filter(|(name, _)| {
                            !gpu_pid(name).is_some_and(|pid| owned_pids.contains(&pid))
                        });
                        let owned = counters.iter().filter(|(name, _)| {
                            gpu_pid(name).is_some_and(|pid| owned_pids.contains(&pid))
                        });
                        m.gpu = self
                            .gpu_target
                            .map(|_| busiest_gpu_engine(other.map(|(n, v)| (n.as_str(), *v))));
                        m.owned_gpu = self
                            .gpu_target
                            .map(|_| busiest_gpu_engine(owned.map(|(n, v)| (n.as_str(), *v))));
                    }
                }
            }
        }
        m
    }
}
#[cfg(windows)]
impl Drop for Sampler {
    fn drop(&mut self) {
        unsafe {
            windows::Win32::System::Performance::PdhCloseQuery(self.query);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn owned_load_does_not_pause_itself() {
        assert_eq!(external_cpu(1000, 900, 750), 15.0);
        assert_eq!(external_cpu(1000, 100, 200), 0.0);
        assert_eq!(
            gpu_pid("pid_123_luid_0x00000000_engtype_Compute"),
            Some(123)
        );
        assert_eq!(gpu_pid("unexpected"), None);
        let s = Settings {
            check_user_idle: false,
            ac_only: false,
            cpu_limit: 20.0,
            ..Default::default()
        };
        assert!(
            idle_allowed(
                &s,
                Metrics {
                    cpu: Some(external_cpu(1000, 900, 750)),
                    ..Default::default()
                }
            )
            .is_ok()
        );
        assert!(
            idle_allowed(
                &s,
                Metrics {
                    cpu: Some(external_cpu(1000, 900, 200)),
                    ..Default::default()
                }
            )
            .is_err()
        );
    }
    #[test]
    fn selected_conditions_are_enforced() {
        let mut s = Settings::default();
        s.check_user_idle = false;
        s.ac_only = false;
        let m = Metrics {
            cpu: Some(10.0),
            gpu: Some(70.0),
            ..Default::default()
        };
        assert!(idle_allowed(&s, m).is_ok());
        s.check_gpu = true;
        assert!(idle_allowed(&s, m).is_err());
        s.gpu_limit = 80.0;
        assert!(idle_allowed(&s, m).is_ok());
        s.check_cpu = false;
        assert!(
            idle_allowed(
                &s,
                Metrics {
                    gpu: Some(0.0),
                    ..Default::default()
                }
            )
            .is_ok()
        );
    }
    #[test]
    fn idle_interval_and_manual() {
        let mut s = Settings::default();
        s.check_cpu = false;
        s.check_user_idle = false;
        s.ac_only = false;
        let now = Local::now();
        assert!(due(&s, now, Metrics::default()).is_err());
        s.schedule = "When idle".into();
        assert!(due(&s, now, Metrics::default()).is_ok());
        s.last_success = now.timestamp();
        assert!(due(&s, now, Metrics::default()).is_err());
    }
    #[test]
    fn timed_schedule_handles_missed_runs_once() {
        let mut s = Settings::default();
        s.schedule = "At selected time".into();
        s.ac_only = false;
        s.schedule_hour = 0;
        s.schedule_minute = 0;
        let now = Local::now();
        let slot = due(&s, now, Metrics::default()).unwrap();
        s.last_schedule_slot = slot;
        assert!(due(&s, now, Metrics::default()).is_err());
    }
}

#[cfg(test)]
mod model_resource_tests {
    use super::*;
    #[test]
    fn model_limits_ignore_typing_but_obey_gpu_and_game_flags() {
        let mut s = Settings {
            defer_search_model: true,
            check_cpu: false,
            check_gpu: true,
            check_user_idle: true,
            idle_seconds: 120,
            ac_only: false,
            gpu_limit: 30.0,
            ..Default::default()
        };
        assert!(
            model_hold(
                &s,
                Metrics {
                    gpu: Some(10.0),
                    idle: 0,
                    ..Default::default()
                }
            )
            .is_none()
        );
        assert_eq!(
            model_hold(
                &s,
                Metrics {
                    gpu: Some(80.0),
                    ..Default::default()
                }
            ),
            Some("Waiting for GPU usage to fall")
        );
        s.pause_for_game = true;
        assert_eq!(
            model_hold(
                &s,
                Metrics {
                    game: true,
                    ..Default::default()
                }
            ),
            Some("A game is running")
        );
        s.remote_url = "http://127.0.0.1:9999".into();
        assert!(
            model_hold(
                &s,
                Metrics {
                    game: true,
                    ..Default::default()
                }
            )
            .is_none()
        );
        assert_eq!(
            resource_pause(
                &s,
                Metrics {
                    game: true,
                    ..Default::default()
                }
            ),
            Some("A game is running")
        );
    }
}

fn busiest_gpu_engine<'a>(items: impl Iterator<Item = (&'a str, f64)>) -> f32 {
    let mut engines = std::collections::HashMap::<String, f64>::new();
    for (name, value) in items {
        if !value.is_finite() {
            continue;
        }
        let Some((_, engine)) = name.split_once("_luid_") else {
            continue;
        };
        let key = engine.split("_engtype_").next().unwrap_or(engine);
        *engines.entry(key.to_owned()).or_default() += value.max(0.0);
    }
    engines
        .values()
        .copied()
        .fold(0.0, f64::max)
        .clamp(0.0, 100.0) as f32
}

#[cfg(test)]
mod gpu_aggregation_tests {
    #[test]
    fn sums_processes_per_engine_but_not_independent_engines_or_adapters() {
        let rows = [
            ("pid_1_luid_0x1_0x0_phys_0_eng_0_engtype_3D", 5.0),
            ("pid_2_luid_0x1_0x0_phys_0_eng_0_engtype_3D", 4.0),
            ("pid_1_luid_0x1_0x0_phys_0_eng_1_engtype_Copy", 8.0),
            ("pid_3_luid_0x2_0x0_phys_0_eng_0_engtype_3D", 7.0),
        ];
        assert_eq!(super::busiest_gpu_engine(rows.into_iter()), 9.0);
    }
}

#[derive(Clone, Debug)]
pub struct GpuAdapter {
    pub id: String,
    pub name: String,
    pub luid: (u32, u32),
}
static GPU_MONITOR_NAME: OnceLock<Mutex<Option<String>>> = OnceLock::new();
pub fn gpu_monitor_name() -> Option<String> {
    GPU_MONITOR_NAME
        .get_or_init(Default::default)
        .lock()
        .unwrap()
        .clone()
}
pub fn gpu_adapters() -> Vec<GpuAdapter> {
    static CACHE: OnceLock<Mutex<Option<(std::time::Instant, Vec<GpuAdapter>)>>> = OnceLock::new();
    let mut cache = CACHE.get_or_init(Default::default).lock().unwrap();
    if let Some((at, items)) = &*cache {
        if at.elapsed() < std::time::Duration::from_secs(5) {
            return items.clone();
        }
    }
    let items = detect_gpu_adapters();
    *cache = Some((std::time::Instant::now(), items.clone()));
    items
}
fn detect_gpu_adapters() -> Vec<GpuAdapter> {
    #[cfg(windows)]
    unsafe {
        use windows::Win32::Graphics::Dxgi::{
            CreateDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE, IDXGIFactory1,
        };
        let Ok(factory) = CreateDXGIFactory1::<IDXGIFactory1>() else {
            return vec![];
        };
        return (0..32)
            .map_while(|i| factory.EnumAdapters1(i).ok().map(|a| (i, a)))
            .filter_map(|(i, a)| {
                let d = a.GetDesc1().ok()?;
                if d.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0 {
                    return None;
                }
                let end = d
                    .Description
                    .iter()
                    .position(|c| *c == 0)
                    .unwrap_or(d.Description.len());
                let name = String::from_utf16_lossy(&d.Description[..end]);
                Some(GpuAdapter {
                    id: format!(
                        "adapter:{i}:{:x}:{:x}:{:x}:{name}",
                        d.VendorId, d.DeviceId, d.SubSysId
                    ),
                    name,
                    luid: (d.AdapterLuid.HighPart as u32, d.AdapterLuid.LowPart),
                })
            })
            .collect();
    }
    #[cfg(not(windows))]
    {
        vec![]
    }
}
fn gpu_luid(name: &str) -> Option<(u32, u32)> {
    let tail = name.split_once("_luid_")?.1;
    let mut p = tail.split('_');
    Some((
        u32::from_str_radix(p.next()?.strip_prefix("0x")?, 16).ok()?,
        u32::from_str_radix(p.next()?.strip_prefix("0x")?, 16).ok()?,
    ))
}
fn selected_gpu(s: &Settings) -> Option<GpuAdapter> {
    let adapters = gpu_adapters();
    if s.gpu_idle_device != "model" {
        return adapters.into_iter().find(|a| a.id == s.gpu_idle_device);
    }
    if s.hardware == "CPU" || s.remote_embeddings() {
        return None;
    }
    #[cfg(windows)]
    if let Some(index) = s
        .device
        .strip_prefix("CUDA")
        .and_then(|n| n.parse::<i32>().ok())
    {
        let luid = cuda_luid(index)?;
        return adapters.into_iter().find(|a| a.luid == luid);
    }
    // Never guess that a compute API index is a Windows adapter index.
    if adapters.len() == 1 {
        return adapters.into_iter().next();
    }
    None
}
#[cfg(windows)]
fn cuda_luid(index: i32) -> Option<(u32, u32)> {
    unsafe {
        use windows::{
            Win32::{
                Foundation::FreeLibrary,
                System::LibraryLoader::{
                    GetProcAddress, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW,
                },
            },
            core::{PCSTR, w},
        };
        let module = LoadLibraryExW(w!("nvcuda.dll"), None, LOAD_LIBRARY_SEARCH_SYSTEM32).ok()?;
        struct Library(windows::Win32::Foundation::HMODULE);
        impl Drop for Library {
            fn drop(&mut self) {
                unsafe {
                    let _ = FreeLibrary(self.0);
                }
            }
        }
        let _library = Library(module);
        let init: unsafe extern "system" fn(u32) -> i32 =
            std::mem::transmute(GetProcAddress(module, PCSTR(c"cuInit".as_ptr().cast()))?);
        let get: unsafe extern "system" fn(*mut i32, i32) -> i32 = std::mem::transmute(
            GetProcAddress(module, PCSTR(c"cuDeviceGet".as_ptr().cast()))?,
        );
        let luid: unsafe extern "system" fn(*mut u8, *mut u32, i32) -> i32 = std::mem::transmute(
            GetProcAddress(module, PCSTR(c"cuDeviceGetLuid".as_ptr().cast()))?,
        );
        let mut device = 0;
        let mut bytes = [0u8; 8];
        let mut mask = 0;
        if init(0) != 0
            || get(&mut device, index) != 0
            || luid(bytes.as_mut_ptr(), &mut mask, device) != 0
        {
            return None;
        }
        Some((
            u32::from_le_bytes(bytes[4..].try_into().ok()?),
            u32::from_le_bytes(bytes[..4].try_into().ok()?),
        ))
    }
}

#[cfg(test)]
mod selected_gpu_tests {
    use super::*;
    #[test]
    fn ignores_busy_igpu_and_preserves_selected_adapter_process_accounting() {
        let rows = [
            (
                "pid_1_luid_0x00000000_0x00000001_phys_0_eng_0_engtype_3D",
                88.0,
            ),
            ("pid_2_luid_0x0_0x2_phys_0_eng_0_engtype_Compute", 3.0),
            ("pid_3_luid_0x0_0x2_phys_0_eng_0_engtype_Compute", 4.0),
        ];
        let selected = rows
            .into_iter()
            .filter(|(name, _)| gpu_luid(name) == Some((0, 2)));
        assert_eq!(busiest_gpu_engine(selected), 7.0);
        assert_eq!(
            gpu_luid("pid_1_luid_0xffffffff_0x1234_phys_0_eng_0_engtype_3D"),
            Some((u32::MAX, 0x1234))
        );
        assert_eq!(gpu_luid("invalid"), None);
    }
    #[test]
    fn gpu_selection_is_saved_and_old_settings_use_model_default() {
        let s: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(s.gpu_idle_device, "model");
        let mut s = s;
        s.gpu_idle_device = "adapter:1:10de:example".into();
        let restored: Settings = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(restored.gpu_idle_device, s.gpu_idle_device);
    }
    #[test]
    #[ignore = "reads real GPU counters and selected CUDA identity without loading a model"]
    #[cfg(windows)]
    fn live_cuda_gpu_idle_selection() {
        let s = Settings {
            hardware: "Dedicated GPU".into(),
            device: "CUDA0".into(),
            ..Default::default()
        };
        let selected = selected_gpu(&s).expect("CUDA model GPU must map to DXGI");
        assert!(selected.name.to_lowercase().contains("nvidia"));
        println!("Selected {} {:?}", selected.name, selected.luid);
        let mut sampler = Sampler::new();
        std::thread::sleep(std::time::Duration::from_secs(2));
        let m = sampler.sample(&s);
        assert!(m.gpu.is_some());
        assert_eq!(gpu_monitor_name(), Some(selected.name));
        println!("Selected GPU external utilization: {:?}", m.gpu);
        let mut manual = s.clone();
        manual.gpu_idle_device = selected.id;
        assert_eq!(selected_gpu(&manual).unwrap().luid, selected.luid);
    }
}
