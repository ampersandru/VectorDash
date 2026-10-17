use crate::{
    config::Settings,
    scheduling::{self, Metrics},
};
use chrono::{DateTime, Local};
use std::sync::atomic::{AtomicU32, Ordering};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Idle,
    Waiting,
    Indexing,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Visual {
    pub status: Status,
    pub accent: [u8; 3],
}
impl Visual {
    pub fn color(self) -> [u8; 3] {
        match self.status {
            Status::Idle => self.accent,
            Status::Waiting => [245, 190, 45],
            Status::Indexing => [239, 68, 68],
        }
    }
    pub fn label(self) -> &'static str {
        match self.status {
            Status::Idle => "Idle",
            Status::Waiting => "Waiting for indexing conditions",
            Status::Indexing => "Indexing",
        }
    }
    fn packed(self) -> u32 {
        u32::from_le_bytes([
            self.accent[0],
            self.accent[1],
            self.accent[2],
            match self.status {
                Status::Idle => 0,
                Status::Waiting => 1,
                Status::Indexing => 2,
            },
        ])
    }
}
static VISUAL: AtomicU32 = AtomicU32::new(u32::from_le_bytes([133, 115, 255, 0]));
pub fn publish(visual: Visual) {
    VISUAL.store(visual.packed(), Ordering::Relaxed);
}
pub fn current() -> Visual {
    let [r, g, b, state] = VISUAL.load(Ordering::Relaxed).to_le_bytes();
    Visual {
        accent: [r, g, b],
        status: match state {
            1 => Status::Waiting,
            2 => Status::Indexing,
            _ => Status::Idle,
        },
    }
}
#[derive(Default)]
pub struct Work {
    pub running: bool,
    pub paused: bool,
    pub automatic: bool,
    pub watching: bool,
    pub held: bool,
    pub pending: bool,
    pub restarting: bool,
}
pub fn status(s: &Settings, w: Work, m: Metrics, now: DateTime<Local>) -> Status {
    let idle_blocked = scheduling::idle_allowed(s, m).is_err();
    let resource_blocked = scheduling::resource_pause(s, m).is_some();
    if w.running {
        let idle_applies = w.automatic
            && (w.watching || ["When idle", "Time + idle"].contains(&s.schedule.as_str()));
        return if w.paused || resource_blocked || (idle_applies && idle_blocked) {
            Status::Waiting
        } else {
            Status::Indexing
        };
    }
    if w.held && !w.restarting {
        return Status::Idle;
    }
    let periodic = scheduling::work_due(s, now);
    let periodic_blocked = resource_blocked
        || (s.ac_only && m.on_ac != Some(true))
        || (["When idle", "Time + idle"].contains(&s.schedule.as_str()) && idle_blocked);
    if ((w.pending || w.restarting) && idle_blocked) || (periodic && periodic_blocked) {
        Status::Waiting
    } else {
        Status::Idle
    }
}
/// Antialiased magnifier, generated only when a tray visual changes.
pub fn pixels(color: [u8; 3]) -> Vec<u8> {
    let mut rgba = vec![0; 32 * 32 * 4];
    for y in 0..32 {
        for x in 0..32 {
            let mut covered = 0;
            for sy in 0..4 {
                for sx in 0..4 {
                    let px = x as f32 + (sx as f32 + 0.5) / 4.;
                    let py = y as f32 + (sy as f32 + 0.5) / 4.;
                    let ring =
                        (((px - 12.8).powi(2) + (py - 12.8).powi(2)).sqrt() - 8.).abs() <= 1.45;
                    let t =
                        (((px - 18.8) * 7.4 + (py - 18.8) * 7.4) / (2. * 7.4 * 7.4)).clamp(0., 1.);
                    let handle = (px - (18.8 + 7.4 * t)).powi(2) + (py - (18.8 + 7.4 * t)).powi(2)
                        <= 1.85f32.powi(2);
                    if ring || handle {
                        covered += 1;
                    }
                }
            }
            let i = (y * 32 + x) * 4;
            rgba[i..i + 3].copy_from_slice(&color);
            rgba[i + 3] = (covered * 255 / 16) as u8;
        }
    }
    rgba
}
#[cfg(test)]
mod tests {
    use super::*;
    fn settings() -> Settings {
        let mut s = Settings::default();
        s.check_cpu = true;
        s.cpu_limit = 20.;
        s.check_gpu = false;
        s.check_user_idle = false;
        s.ac_only = false;
        s.pause_for_fullscreen = false;
        s.pause_for_game = false;
        s.schedule = "Manual".into();
        s
    }
    fn metrics(cpu: f32) -> Metrics {
        Metrics {
            cpu: Some(cpu),
            on_ac: Some(true),
            ..Default::default()
        }
    }
    #[test]
    fn colors_follow_real_work_and_conditions() {
        let s = settings();
        let now = Local::now();
        assert_eq!(status(&s, Work::default(), metrics(90.), now), Status::Idle);
        assert_eq!(
            status(
                &s,
                Work {
                    pending: true,
                    ..Default::default()
                },
                metrics(90.),
                now
            ),
            Status::Waiting
        );
        assert_eq!(
            status(
                &s,
                Work {
                    pending: true,
                    ..Default::default()
                },
                metrics(0.),
                now
            ),
            Status::Idle
        );
        assert_eq!(
            status(
                &s,
                Work {
                    running: true,
                    ..Default::default()
                },
                metrics(90.),
                now
            ),
            Status::Indexing
        );
        assert_eq!(
            status(
                &s,
                Work {
                    running: true,
                    automatic: true,
                    watching: true,
                    ..Default::default()
                },
                metrics(90.),
                now
            ),
            Status::Waiting
        );
        assert_eq!(
            status(
                &s,
                Work {
                    running: true,
                    paused: true,
                    ..Default::default()
                },
                metrics(0.),
                now
            ),
            Status::Waiting
        );
        assert_eq!(
            status(
                &s,
                Work {
                    held: true,
                    pending: true,
                    ..Default::default()
                },
                metrics(90.),
                now
            ),
            Status::Idle
        );
    }
    #[test]
    fn periodic_work_only_waits_after_its_interval_and_respects_ac_power() {
        use chrono::Timelike;
        let mut s = settings();
        let now = Local::now();
        s.schedule = "When idle".into();
        s.last_success = now.timestamp();
        s.idle_interval_minutes = 60;
        assert_eq!(status(&s, Work::default(), metrics(90.), now), Status::Idle);
        s.last_success = 0;
        assert_eq!(
            status(&s, Work::default(), metrics(90.), now),
            Status::Waiting
        );
        s.schedule = "Time".into();
        s.schedule_hour = now.hour();
        s.schedule_minute = now.minute();
        s.schedule_days = [true; 7];
        s.schedule_enabled_at = 0;
        s.last_schedule_slot = 0;
        s.ac_only = true;
        assert_eq!(
            status(
                &s,
                Work::default(),
                Metrics {
                    on_ac: Some(false),
                    ..metrics(90.)
                },
                now
            ),
            Status::Waiting
        );
        assert_eq!(status(&s, Work::default(), metrics(90.), now), Status::Idle);
    }
    #[test]
    fn custom_theme_and_antialiased_transparent_icon() {
        let visual = Visual {
            status: Status::Idle,
            accent: [22, 160, 77],
        };
        assert_eq!(visual.color(), [22, 160, 77]);
        let p = pixels(visual.color());
        assert_eq!(p.len(), 4096);
        assert_eq!(p[3], 0);
        assert!(p.chunks_exact(4).any(|v| v[3] > 0 && v[3] < 255));
        assert!(p.chunks_exact(4).any(|v| v[3] == 255));
        assert_eq!(
            Visual {
                status: Status::Waiting,
                ..visual
            }
            .color(),
            [245, 190, 45]
        );
        assert_eq!(
            Visual {
                status: Status::Indexing,
                ..visual
            }
            .color(),
            [239, 68, 68]
        );
    }
}
