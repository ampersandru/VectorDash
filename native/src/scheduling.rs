use crate::config::Settings;
use chrono::{DateTime, Datelike, Local, TimeZone, Timelike};
use std::sync::{Mutex, OnceLock};
#[derive(Clone, Copy, Debug, Default)]
pub struct Metrics {
    pub cpu: Option<f32>,
    pub gpu: Option<f32>,
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
    if !s.remote_url.is_empty() {
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
    if let Some(reason) = resource_pause(s, m) {
        return Err(reason.into());
    }
    if s.schedule == "Manual" {
        return Err("Periodic scan scheduling is off".into());
    }
    if s.schedule == "When idle" {
        if now.timestamp() < s.last_success + s.idle_interval_minutes as i64 * 60 {
            return Err("Waiting for the next idle interval".into());
        }
        idle_allowed(s, m).map_err(str::to_string)?;
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
    if s.ac_only && m.on_ac != Some(true) {
        return Err("Waiting for AC power".into());
    }
    if s.schedule == "Time + idle" {
        idle_allowed(s, m).map_err(str::to_string)?;
    }
    Ok(slot)
}
pub fn start_monitor() {
    static STARTED: OnceLock<()> = OnceLock::new();
    STARTED.get_or_init(|| {
        std::thread::spawn(|| {
            #[cfg(windows)]
            let mut sampler = Sampler::new();
            loop {
                #[cfg(windows)]
                let mut value = sampler.sample();
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
        }
    }
    fn sample(&mut self) -> Metrics {
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
                        let value: f64 = items
                            .iter()
                            .filter(|x| x.FmtValue.CStatus <= 1)
                            .filter(|x| {
                                !gpu_pid(&x.szName.to_string().unwrap_or_default())
                                    .is_some_and(|pid| owned_pids.contains(&pid))
                            })
                            .map(|x| x.FmtValue.Anonymous.doubleValue.max(0.0))
                            .sum();
                        m.gpu = Some(value.clamp(0.0, 100.0) as f32);
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
