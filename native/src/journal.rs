//! Bounded optional NTFS USN replay; persist cursors only after successful indexing.
mod directory;
use anyhow::{Context, Result};
use directory::DirectoryCache;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
struct Mark {
    id: u64,
    usn: i64,
}
#[derive(Default, Serialize, Deserialize)]
struct Saved {
    signature: String,
    marks: BTreeMap<String, Mark>,
    #[serde(default)]
    coverage: BTreeMap<String, String>,
}
struct Retry {
    attempts: u32,
    bad_batch: bool,
    after: Instant,
}
fn malformed(error: &anyhow::Error) -> bool {
    #[cfg(windows)]
    {
        !error.chain().any(|e| e.is::<windows::core::Error>())
    }
    #[cfg(not(windows))]
    {
        let _ = error;
        true
    }
}
fn retry_delay(attempts: u32) -> Duration {
    Duration::from_secs((5u64 << attempts.saturating_sub(1).min(6)).min(300))
}
#[derive(Default)]
pub struct Tracker {
    signature: String,
    coverage: BTreeMap<String, String>,
    directory_cache: DirectoryCache,
    flight_cache: Option<DirectoryCache>,
    encrypt_cache: bool,
    retries: BTreeMap<String, Retry>,
    logger: Option<crate::engine::NotifySender>,
    roots: Vec<PathBuf>,
    resolution_warning: BTreeMap<String, Instant>,
    missing_names: BTreeMap<(String, String), i64>,
    read: BTreeMap<String, Mark>,
    committed: BTreeMap<String, Mark>,
    flight: Option<BTreeMap<String, Mark>>,
}
impl Tracker {
    pub fn configure(
        &mut self,
        s: &crate::config::Settings,
        home: &Path,
        tx: &crate::engine::NotifySender,
    ) -> Vec<PathBuf> {
        self.configure_with(s, home, tx, query, journal_supported)
    }
    fn configure_with(
        &mut self,
        s: &crate::config::Settings,
        home: &Path,
        tx: &crate::engine::NotifySender,
        mut query: impl FnMut(&str) -> Result<(u64, i64, i64)>,
        mut supported: impl FnMut(&str) -> bool,
    ) -> Vec<PathBuf> {
        *self = Self::default();
        if !s.ntfs_journal {
            return s.roots.iter().map(|r| PathBuf::from(&r.path)).collect();
        }
        self.logger = Some(tx.clone());
        self.roots = s.roots.iter().map(|r| PathBuf::from(&r.path)).collect();
        self.signature = format!(
            "{:?}|{:?}|{}|{}",
            s.roots, s.exclusions, s.excluded_types, s.skip_windows_churn
        );
        let saved = std::fs::read(home.join("ntfs-journal.json"))
            .ok()
            .and_then(|b| serde_json::from_slice::<Saved>(&b).ok())
            .unwrap_or_default();
        self.encrypt_cache = s.encrypt_index;
        self.directory_cache = DirectoryCache::load(home, s.encrypt_index, &saved.marks)
            .unwrap_or_else(|error| {
                self.log(
                    "Info",
                    format!("NTFS directory cache will be rebuilt: {error:#}"),
                );
                DirectoryCache::default()
            });
        let signature_matches = saved.signature == self.signature;
        let mut uncovered = vec![];
        for root in &s.roots {
            self.coverage.insert(
                crate::config::fold(&root.path),
                format!(
                    "{}|{:?}|{}|{}|{}",
                    root.mode,
                    s.exclusions,
                    s.excluded_types,
                    s.skip_windows_churn,
                    s.include_hidden
                ),
            );
        }
        let mut volumes = BTreeMap::new();
        for root in &s.roots {
            let path = root.path.as_bytes();
            if path.len() >= 3 && path[0].is_ascii_alphabetic() && path[1] == b':' {
                let volume = root.path[..2].to_uppercase();
                if supported(&volume) {
                    volumes.insert(volume, ());
                } else {
                    uncovered.push(PathBuf::from(&root.path));
                    self.log("Info",format!("NTFS journal {volume}: network or unsupported filesystem; folder notifications only, journal retries disabled"));
                }
            } else {
                uncovered.push(PathBuf::from(&root.path));
            }
        }
        for (volume, _) in volumes {
            match query(&volume) {
                Ok((id, first, next)) => {
                    let old = saved
                        .marks
                        .get(&volume)
                        .copied()
                        .filter(|m| m.id == id && m.usn >= first && m.usn <= next);
                    for root in &s.roots {
                        let key = crate::config::fold(&root.path);
                        if crate::config::under(&root.path, &format!("{volume}\\"))
                            && (old.is_none()
                                || !saved.coverage.get(&key).map_or(signature_matches, |value| {
                                    Some(value) == self.coverage.get(&key)
                                }))
                        {
                            uncovered.push(PathBuf::from(&root.path));
                        }
                    }
                    let mark = old.unwrap_or(Mark { id, usn: next });
                    self.log("Info", format!("NTFS journal {volume}: reader ready; journal={}, cursor={}, retained range={first}..{next}", mark.id, mark.usn));
                    self.read.insert(volume.clone(), mark);
                    self.committed.insert(volume, mark);
                }
                Err(e) => {
                    self.retries.insert(
                        volume.clone(),
                        Retry {
                            attempts: 1,
                            bad_batch: false,
                            after: Instant::now() + retry_delay(1),
                        },
                    );
                    // Startup access failures do not prove that saved history
                    // was lost. Wait for retry validation before requesting a
                    // volume catch-up; only genuinely new/changed sources need it.
                    if let Some(mark) = saved.marks.get(&volume) {
                        self.committed.insert(volume.clone(), *mark);
                    }
                    for root in &s.roots {
                        let key = crate::config::fold(&root.path);
                        if crate::config::under(&root.path, &format!("{volume}\\"))
                            && (saved.marks.get(&volume).is_none()
                                || !saved.coverage.get(&key).map_or(signature_matches, |value| {
                                    Some(value) == self.coverage.get(&key)
                                }))
                        {
                            uncovered.push(PathBuf::from(&root.path));
                        }
                    }
                    let _=tx.send(crate::engine::Event::Log("Warning".into(),format!("NTFS journal {volume} unavailable; retry in 5s; folder notifications remain active: {e:#}")));
                }
            }
        }
        let _ = tx.send(crate::engine::Event::Log(
            "Info".into(),
            format!(
                "NTFS journal: {} accessible volumes; {}",
                self.read.len(),
                if uncovered.is_empty() {
                    "replaying saved cursors"
                } else {
                    "catch-up queued only for sources without continuous coverage"
                }
            ),
        ));
        uncovered.sort();
        uncovered.dedup();
        uncovered
    }
    fn log(&self, level: &str, message: String) {
        if let Some(tx) = &self.logger {
            let _ = tx.send(crate::engine::Event::Log(level.into(), message));
        }
    }
    fn recover_volume(&self, volume: &str, paths: &mut Vec<PathBuf>) {
        // Never broaden a journal failure to unrelated drives or unwatched folders.
        for root in &self.roots {
            if crate::config::under(&root.to_string_lossy(), &format!("{volume}\\")) {
                paths.push(root.clone());
            }
        }
    }
    pub fn covers(&self, path: &Path) -> bool {
        let text = path.to_string_lossy();
        text.get(..2).is_some_and(|v| {
            let v = v.to_ascii_uppercase();
            self.read.contains_key(&v) && !self.retries.contains_key(&v)
        })
    }
    /// Indexed basename seeks recover deleted/unavailable parents without walking
    /// the volume. New live paths are resolved by file ID and folder notifications.
    pub fn resolve_missing(&mut self, db: &crate::db::Database) -> Result<Vec<PathBuf>> {
        if self.missing_names.is_empty() {
            return Ok(vec![]);
        }
        let c = db.connect(true)?;
        // Bound each scheduler tick to four indexed-name seeks / 1024 paths.
        // Continuations retain duplicate basenames without allocating huge lists.
        let mut statement = c.prepare_cached(
            "SELECT rowid,path FROM names WHERE base=?1 AND rowid>?2 ORDER BY rowid LIMIT 256",
        )?;
        let mut paths = std::collections::BTreeSet::new();
        let keys = self
            .missing_names
            .keys()
            .take(4)
            .cloned()
            .collect::<Vec<_>>();
        let mut completed = vec![];
        for key in &keys {
            let (volume, name) = key;
            let cursor = &self.missing_names[key];
            let rows = statement
                .query_map(rusqlite::params![name.to_lowercase(), cursor], |r| {
                    Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
                })?;
            let mut count = 0;
            let mut last = i64::MIN;
            for row in rows {
                let (id, path) = row?;
                last = id;
                count += 1;
                if crate::config::under(&path, &format!("{volume}\\")) {
                    paths.insert(PathBuf::from(path));
                }
            }
            completed.push((key.clone(), if count < 256 { None } else { Some(last) }));
        }
        // Do not acknowledge earlier pages if a later query fails.
        for (key, cursor) in completed {
            if let Some(cursor) = cursor {
                self.missing_names.insert(key, cursor);
            } else {
                self.missing_names.remove(&key);
            }
        }
        Ok(paths.into_iter().collect())
    }
    pub fn retrying(&self) -> usize {
        self.retries.len()
    }
    pub fn poll(&mut self) -> (Vec<PathBuf>, bool) {
        let mut cache = std::mem::take(&mut self.directory_cache);
        let result = self.poll_with(Instant::now(), query, |volume, mark| {
            read_cached(volume, mark, &mut cache)
        });
        self.directory_cache = cache;
        result
    }
    fn poll_with(
        &mut self,
        now: Instant,
        mut query_volume: impl FnMut(&str) -> Result<(u64, i64, i64)>,
        mut read_volume: impl FnMut(&str, Mark) -> Result<Batch>,
    ) -> (Vec<PathBuf>, bool) {
        let mut paths = vec![];
        let volumes: std::collections::BTreeSet<_> = self
            .read
            .keys()
            .chain(self.retries.keys())
            .cloned()
            .collect();
        for volume in volumes {
            let attempts = if let Some(retry) = self.retries.get(&volume) {
                if now < retry.after {
                    continue;
                }
                retry.attempts
            } else {
                0
            };
            let result = (|| -> Result<Batch> {
                if attempts > 0 {
                    let (id, first, next) =
                        query_volume(&volume).context("query journal before retry")?;
                    let previous = self
                        .read
                        .get(&volume)
                        .or_else(|| self.committed.get(&volume))
                        .copied();
                    let valid = previous.filter(|m| m.id == id && m.usn >= first && m.usn <= next);
                    // Rebase only a confirmed gap, or a repeatedly unreadable batch.
                    if valid.is_none() || (attempts >= 3 && self.retries[&volume].bad_batch) {
                        self.recover_volume(&volume, &mut paths);
                        self.log("Warning", format!("NTFS journal {volume}: scoped recovery queued; previous={previous:?}, journal={id}, first={first}, next={next}, attempts={attempts}"));
                        self.read.insert(volume.clone(), Mark { id, usn: next });
                    } else {
                        self.read.insert(volume.clone(), valid.unwrap());
                    }
                }
                read_volume(&volume, self.read[&volume]).context("read journal batch")
            })();
            match result {
                Ok(batch) => {
                    self.read.get_mut(&volume).unwrap().usn = batch.next;
                    paths.extend(batch.paths);
                    if let Some(issue) = batch.unresolved {
                        for name in batch.missing_names {
                            self.missing_names
                                .entry((volume.clone(), name))
                                .or_insert(i64::MIN);
                        }
                        if self.resolution_warning.get(&volume).is_none_or(|last| {
                            now.saturating_duration_since(*last) >= Duration::from_secs(60)
                        }) {
                            self.log("Warning", format!("NTFS journal {volume}: {issue}; reader retained, cursor advanced to {}; targeted indexed-name checks queued; live folder notifications remain active (similar warnings limited to once per minute)", batch.next));
                            self.resolution_warning.insert(volume.clone(), now);
                        }
                    }
                    if attempts > 0 {
                        self.log("Info", format!("NTFS journal {volume}: reader resumed after {attempts} failed attempt(s), cursor={}", batch.next));
                    }
                    self.retries.remove(&volume);
                }
                Err(error) => {
                    // Permission/sharing/I/O errors must preserve a valid cursor.
                    let bad_batch = malformed(&error);
                    let attempts = attempts.saturating_add(1);
                    let delay = retry_delay(attempts);
                    self.log("Warning", format!("NTFS journal {volume}: attempt {attempts} failed; cursor={:?}; retry in {}s; folder notifications remain active: {error:#}", self.read.get(&volume), delay.as_secs()));
                    self.retries.insert(
                        volume,
                        Retry {
                            attempts,
                            bad_batch,
                            after: now + delay,
                        },
                    );
                }
            }
        }
        paths.sort();
        paths.dedup();
        (paths, false) // scoped recovery paths use the existing durable targeted queue
    }
    pub fn begin(&mut self) {
        self.flight_cache = Some(self.directory_cache.clone());
        self.flight = if self.read.is_empty() {
            None
        } else {
            let mut marks = self.committed.clone();
            // A crash must replay unresolved names whose bounded DB checks have
            // not yet reached the durable change queue.
            marks.extend(
                self.read
                    .iter()
                    .filter(|(volume, _)| !self.missing_names.keys().any(|(v, _)| v == *volume))
                    .map(|(v, m)| (v.clone(), *m)),
            );
            Some(marks)
        };
    }
    pub fn finish(&mut self, success: bool, home: &Path) {
        if let Some(marks) = self.flight.take() {
            if success {
                let saved = Saved {
                    signature: self.signature.clone(),
                    coverage: self.coverage.clone(),
                    marks: marks.clone(),
                };
                let persisted = (|| -> Result<()> {
                    let bytes = serde_json::to_vec(&saved)?;
                    let temp = home.join("ntfs-journal.pending");
                    std::fs::write(&temp, bytes).context("write journal checkpoint")?;
                    std::fs::rename(temp, home.join("ntfs-journal.json"))
                        .context("replace journal checkpoint")?;
                    Ok(())
                })();
                match persisted {
                    Ok(()) => {
                        self.committed = marks;
                        if let Some(cache)=self.flight_cache.take() {
                            if let Err(error)=cache.save(home,self.encrypt_cache){self.log("Warning",format!("NTFS directory cache not saved (journal checkpoints remain valid): {error:#}"));}
                        }
                    },
                    Err(error) => self.log("Warning", format!("NTFS journal checkpoint not saved; previous durable cursors retained: {error:#}")),
                }
            } else {
                self.read = self.committed.clone();
                self.directory_cache = DirectoryCache::default();
            }
        }
        self.flight_cache = None;
    }
    pub fn active(&self) -> usize {
        self.read
            .keys()
            .filter(|v| !self.retries.contains_key(*v))
            .count()
    }
}
struct Batch {
    next: i64,
    paths: Vec<PathBuf>,
    unresolved: Option<String>,
    missing_names: Vec<String>,
}
#[derive(Debug)]
struct Record {
    id: u64,
    parent: u64,
    name: String,
    reason: u32,
    attributes: u32,
}
fn parse(buffer: &[u8]) -> Result<(i64, Vec<Record>)> {
    anyhow::ensure!(buffer.len() >= 8, "Truncated USN cursor");
    let next = i64::from_le_bytes(buffer[..8].try_into()?);
    let mut records = vec![];
    let mut offset = 8;
    while offset < buffer.len() {
        let r = &buffer[offset..];
        anyhow::ensure!(r.len() >= 60, "Truncated USN header");
        let length = u32::from_le_bytes(r[..4].try_into()?) as usize;
        let version = u16::from_le_bytes(r[4..6].try_into()?);
        anyhow::ensure!(
            version == 2 && length >= 60 && length <= r.len(),
            "Unsupported/malformed USN record"
        );
        let name_len = u16::from_le_bytes(r[56..58].try_into()?) as usize;
        let name_offset = u16::from_le_bytes(r[58..60].try_into()?) as usize;
        anyhow::ensure!(
            name_offset >= 60 && name_len % 2 == 0 && name_offset + name_len <= length,
            "Invalid USN filename"
        );
        let utf16 = r[name_offset..name_offset + name_len]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect::<Vec<_>>();
        let name = String::from_utf16(&utf16)?;
        anyhow::ensure!(
            !name.contains(['\\', '/', char::from(0)]) && name != "." && name != "..",
            "Invalid journal path component"
        );
        records.push(Record {
            id: u64::from_le_bytes(r[8..16].try_into()?),
            parent: u64::from_le_bytes(r[16..24].try_into()?),
            reason: u32::from_le_bytes(r[40..44].try_into()?),
            attributes: u32::from_le_bytes(r[52..56].try_into()?),
            name,
        });
        offset += length;
    }
    Ok((next, records))
}

#[cfg(windows)]
fn journal_supported(volume: &str) -> bool {
    use windows::{
        Win32::Storage::FileSystem::{GetDriveTypeW, GetVolumeInformationW},
        core::PCWSTR,
    };
    let root = format!("{volume}\\")
        .encode_utf16()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let kind = unsafe { GetDriveTypeW(PCWSTR(root.as_ptr())) };
    if kind == 0 || kind == 1 || kind == 4 || kind == 5 {
        return false;
    }
    let mut fs = [0u16; 32];
    let info = unsafe {
        GetVolumeInformationW(PCWSTR(root.as_ptr()), None, None, None, None, Some(&mut fs))
    };
    if info.is_ok() {
        let end = fs.iter().position(|c| *c == 0).unwrap_or(fs.len());
        return String::from_utf16_lossy(&fs[..end]).eq_ignore_ascii_case("NTFS");
    }
    true
}
#[cfg(not(windows))]
fn journal_supported(_: &str) -> bool {
    false
}
#[cfg(windows)]
mod win {
    use super::*;
    use windows::{
        Win32::{
            Foundation::{CloseHandle, GENERIC_READ, HANDLE},
            Storage::FileSystem::*,
            System::{IO::DeviceIoControl, Ioctl::*},
        },
        core::PCWSTR,
    };
    struct Volume(HANDLE);
    impl Drop for Volume {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }
    fn open(volume: &str) -> Result<Volume> {
        let name = format!("\\\\.\\{volume}")
            .encode_utf16()
            .chain(Some(0))
            .collect::<Vec<_>>();
        Ok(Volume(unsafe {
            CreateFileW(
                PCWSTR(name.as_ptr()),
                GENERIC_READ.0,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                None,
                OPEN_EXISTING,
                FILE_FLAGS_AND_ATTRIBUTES(0),
                None,
            )
        }?))
    }

    fn seed_metadata(volume: &str, cache: &mut DirectoryCache) {
        for suffix in ["", "$Extend", "$Extend\\$Deleted", "$Extend\\$Delete"] {
            let path = PathBuf::from(format!("{volume}\\{suffix}"));
            let name = path
                .as_os_str()
                .to_string_lossy()
                .encode_utf16()
                .chain(Some(0))
                .collect::<Vec<_>>();
            if let Ok(handle) = unsafe {
                CreateFileW(
                    PCWSTR(name.as_ptr()),
                    0,
                    FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                    None,
                    OPEN_EXISTING,
                    FILE_FLAG_BACKUP_SEMANTICS,
                    None,
                )
            } {
                let handle = Volume(handle);
                let mut info = BY_HANDLE_FILE_INFORMATION::default();
                if unsafe { GetFileInformationByHandle(handle.0, &mut info) }.is_ok() {
                    let id = ((info.nFileIndexHigh as u64) << 32) | info.nFileIndexLow as u64;
                    cache.insert(volume, id, path);
                }
            }
        }
    }
    pub(super) fn query(volume: &str) -> Result<(u64, i64, i64)> {
        let handle = open(volume)?;
        let mut data = USN_JOURNAL_DATA_V0::default();
        let mut count = 0;
        unsafe {
            DeviceIoControl(
                handle.0,
                FSCTL_QUERY_USN_JOURNAL,
                None,
                0,
                Some(&mut data as *mut _ as _),
                std::mem::size_of_val(&data) as u32,
                Some(&mut count),
                None,
            )
        }?;
        anyhow::ensure!(
            count as usize >= std::mem::size_of_val(&data),
            "Incomplete journal information"
        );
        Ok((
            data.UsnJournalID,
            data.FirstUsn.max(data.LowestValidUsn),
            data.NextUsn,
        ))
    }
    pub(super) fn read(volume: &str, mark: Mark) -> Result<Batch> {
        read_cached(volume, mark, &mut DirectoryCache::default())
    }
    pub(super) fn read_cached(
        volume: &str,
        mark: Mark,
        cache: &mut DirectoryCache,
    ) -> Result<Batch> {
        let fresh = cache.align(volume, mark);
        if fresh {
            seed_metadata(volume, cache);
        }
        let handle = open(volume)?;
        let input = READ_USN_JOURNAL_DATA_V1 {
            MinMajorVersion: 2,
            MaxMajorVersion: 2,
            StartUsn: mark.usn,
            // Content/name/attribute changes affect the index; security-only,
            // close-only and object-ID bookkeeping do not.
            ReasonMask: 0x0011_b377,
            ReturnOnlyOnClose: 0,
            Timeout: 0,
            BytesToWaitFor: 0,
            UsnJournalID: mark.id,
        };
        let mut buffer = vec![0u8; 65536];
        let mut count = 0;
        unsafe {
            DeviceIoControl(
                handle.0,
                FSCTL_READ_USN_JOURNAL,
                Some(&input as *const _ as _),
                std::mem::size_of_val(&input) as u32,
                Some(buffer.as_mut_ptr() as _),
                buffer.len() as u32,
                Some(&mut count),
                None,
            )
        }?;
        anyhow::ensure!(
            count as usize <= buffer.len(),
            "Invalid journal buffer size"
        );
        let (next, records) =
            parse(&buffer[..count as usize]).context("decode V2 journal buffer")?;
        anyhow::ensure!(next >= mark.usn, "Journal cursor moved backwards");
        directory::resolve_records(volume, mark, next, records, cache, |parent_id| {
            let id = FILE_ID_DESCRIPTOR {
                dwSize: std::mem::size_of::<FILE_ID_DESCRIPTOR>() as u32,
                Type: FileIdType,
                Anonymous: FILE_ID_DESCRIPTOR_0 {
                    FileId: parent_id as i64,
                },
            };

            let parent = Volume(
                unsafe {
                    OpenFileById(
                        handle.0,
                        &id,
                        0,
                        FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                        None,
                        FILE_FLAG_BACKUP_SEMANTICS,
                    )
                }
                .context("Journal parent unavailable; recovery needed")?,
            );
            let mut text = vec![0u16; 32768];
            let n = unsafe {
                GetFinalPathNameByHandleW(parent.0, &mut text, GETFINALPATHNAMEBYHANDLE_FLAGS(0))
            } as usize;
            if n == 0 {
                return Err(windows::core::Error::from_win32())
                    .context("resolve parent final path");
            }
            anyhow::ensure!(n < text.len(), "Journal parent path exceeds buffer");
            let text = String::from_utf16(&text[..n])?;
            Ok(PathBuf::from(text.strip_prefix("\\\\?\\").unwrap_or(&text)))
        })
    }
}
#[cfg(windows)]
use win::{query, read, read_cached};
#[cfg(not(windows))]
fn query(_: &str) -> Result<(u64, i64, i64)> {
    anyhow::bail!("NTFS journals require Windows")
}
#[cfg(not(windows))]
fn read(_: &str, _: Mark) -> Result<Batch> {
    anyhow::bail!("NTFS journals require Windows")
}
#[cfg(not(windows))]
fn read_cached(_: &str, _: Mark, _: &mut DirectoryCache) -> Result<Batch> {
    anyhow::bail!("NTFS journals require Windows")
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn untrusted_records_are_bounded() {
        assert!(parse(&[0; 7]).is_err());
        assert!(parse(&[0; 8]).unwrap().1.is_empty());
        let mut b = vec![0; 68];
        b[8..12].copy_from_slice(&60u32.to_le_bytes());
        b[12..14].copy_from_slice(&3u16.to_le_bytes());
        assert!(parse(&b).is_err());
        b[12..14].copy_from_slice(&2u16.to_le_bytes());
        b[66..68].copy_from_slice(&60u16.to_le_bytes());
        assert!(parse(&b).is_ok());
        b[64..66].copy_from_slice(&100u16.to_le_bytes());
        assert!(parse(&b).is_err());
    }
}

#[cfg(test)]
mod checkpoint_tests {
    use super::*;
    #[test]
    fn only_successful_batch_cursors_commit() {
        let home = tempfile::tempdir().unwrap();
        let mut tracker = Tracker::default();
        tracker.signature = "fixture".into();
        tracker
            .committed
            .insert("C:".into(), Mark { id: 1, usn: 10 });
        tracker.read.insert("C:".into(), Mark { id: 1, usn: 20 });
        tracker.begin();
        tracker.read.get_mut("C:").unwrap().usn = 30;
        tracker.finish(true, home.path());
        let saved: Saved =
            serde_json::from_slice(&std::fs::read(home.path().join("ntfs-journal.json")).unwrap())
                .unwrap();
        assert_eq!(saved.marks["C:"].usn, 20);
        tracker.begin();
        tracker.finish(false, home.path());
        assert_eq!(tracker.read["C:"].usn, 20);
    }
}

pub fn probe(volume: &str) -> Result<serde_json::Value> {
    let (id, first, next) = query(volume)?;
    let batch = read(volume, Mark { id, usn: next })?;
    Ok(
        serde_json::json!({"accessible":true,"id":id,"first":first,"next":next,"pending_paths":batch.paths.len(),"unresolved":batch.unresolved}),
    )
}

#[cfg(test)]
mod retry_tests {
    use super::*;
    fn tracker() -> Tracker {
        let mut t = Tracker::default();
        t.read.insert("C:".into(), Mark { id: 1, usn: 10 });
        t.read.insert("D:".into(), Mark { id: 2, usn: 10 });
        t.roots = vec![PathBuf::from("C:\\Users"), PathBuf::from("D:\\Work")];
        t
    }
    fn batch(next: i64) -> Batch {
        Batch {
            next,
            paths: vec![],
            unresolved: None,
            missing_names: vec![],
        }
    }
    #[test]
    fn transient_failure_preserves_cursor_and_healthy_drive_then_retries() {
        let mut t = tracker();
        let now = Instant::now();
        let (paths, recovery) = t.poll_with(
            now,
            |_| unreachable!(),
            |v, _| {
                if v == "C:" {
                    anyhow::bail!("temporary failure")
                } else {
                    Ok(batch(20))
                }
            },
        );
        assert!(paths.is_empty() && !recovery);
        assert_eq!(t.read["C:"].usn, 10);
        assert_eq!(t.read["D:"].usn, 20);
        assert_eq!(t.active(), 1);
        t.poll_with(
            now + Duration::from_secs(4),
            |_| unreachable!(),
            |v, _| {
                assert_eq!(v, "D:");
                Ok(batch(21))
            },
        );
        let (paths, recovery) = t.poll_with(
            now + Duration::from_secs(5),
            |_| Ok((1, 0, 100)),
            |_, mark| Ok(batch(mark.usn + 1)),
        );
        assert!(paths.is_empty() && !recovery);
        assert_eq!(t.read["C:"].usn, 11);
        assert_eq!(t.active(), 2);
        assert_eq!(t.retrying(), 0);
    }
    #[test]
    fn confirmed_gap_queues_only_sources_on_affected_volume() {
        let mut t = tracker();
        let now = Instant::now();
        t.retries.insert(
            "C:".into(),
            Retry {
                attempts: 1,
                bad_batch: false,
                after: now,
            },
        );
        let (paths, global) = t.poll_with(now, |_| Ok((7, 50, 100)), |_, m| Ok(batch(m.usn)));
        assert!(!global);
        assert_eq!(paths, vec![PathBuf::from("C:\\Users")]);
        assert_eq!(t.read["C:"].id, 7);
        assert_eq!(t.read["C:"].usn, 100);
        assert_eq!(t.read["D:"].usn, 10);
    }
    #[test]
    fn expired_cursor_is_rebased_once() {
        let mut t = tracker();
        let now = Instant::now();
        t.retries.insert(
            "C:".into(),
            Retry {
                attempts: 1,
                bad_batch: false,
                after: now,
            },
        );
        let (paths, _) = t.poll_with(now, |_| Ok((1, 50, 100)), |_, m| Ok(batch(m.usn)));
        assert_eq!(paths.len(), 1);
        let (paths, _) = t.poll_with(now, |_| unreachable!(), |_, m| Ok(batch(m.usn)));
        assert!(paths.is_empty());
    }
    #[test]
    fn unresolved_parent_advances_batch_without_removing_reader() {
        let mut t = tracker();
        let (paths, global) = t.poll_with(
            Instant::now(),
            |_| unreachable!(),
            |v, _| {
                let mut b = batch(40);
                if v == "C:" {
                    b.paths.push(PathBuf::from("C:\\Users\\good.txt"));
                    b.unresolved = Some("deleted parent".into());
                }
                Ok(b)
            },
        );
        assert!(!global);
        assert!(paths.contains(&PathBuf::from("C:\\Users\\good.txt")));
        assert!(!paths.contains(&PathBuf::from("C:\\Users")));
        assert_eq!(t.read["C:"].usn, 40);
        assert_eq!(t.active(), 2);
    }
    #[test]
    fn repeated_missing_parents_never_queue_volume_roots_and_lookup_is_paged() {
        let home = tempfile::tempdir().unwrap();
        let mut s = crate::config::Settings::default();
        s.encrypt_index = false;
        let db = crate::db::Database::new(home.path(), &s).unwrap();
        let c = db.connect(false).unwrap();
        for n in 0..600 {
            c.execute(
                "INSERT INTO names(path,name,base) VALUES(?1,'gone.txt','gone.txt')",
                [format!(r"C:\Users\folder-{n:04}\gone.txt")],
            )
            .unwrap();
        }
        c.execute(
            r"INSERT INTO names(path,name,base) VALUES('D:\Work\gone.txt','gone.txt','gone.txt')",
            [],
        )
        .unwrap();
        let mut t = tracker();
        for _ in 0..100 {
            let (paths, global) = t.poll_with(
                Instant::now(),
                |_| unreachable!(),
                |v, m| {
                    let mut b = batch(m.usn + 1);
                    if v == "C:" {
                        b.unresolved = Some("parent unavailable".into());
                        b.missing_names = vec!["gone.txt".into()];
                    }
                    Ok(b)
                },
            );
            assert!(paths.is_empty() && !global);
        }
        let mut found = std::collections::BTreeSet::new();
        while !t.missing_names.is_empty() {
            let paths = t.resolve_missing(&db).unwrap();
            assert!(paths.len() <= 256);
            assert!(paths.iter().all(|p| p.starts_with(r"C:\Users")));
            found.extend(paths);
        }
        assert_eq!(found.len(), 600);
        assert_eq!(t.read["C:"].usn, 110);
    }
    #[test]
    fn unresolved_names_hold_durable_cursor_until_targeted_checks_are_queued() {
        let home = tempfile::tempdir().unwrap();
        let mut t = tracker();
        t.committed = t.read.clone();
        t.read.get_mut("C:").unwrap().usn = 30;
        t.missing_names
            .insert(("C:".into(), "gone.txt".into()), i64::MIN);
        t.begin();
        t.finish(true, home.path());
        assert_eq!(t.committed["C:"].usn, 10);
        assert_eq!(t.read["C:"].usn, 30);
        t.missing_names.clear();
        t.begin();
        t.finish(true, home.path());
        assert_eq!(t.committed["C:"].usn, 30);
    }
    #[test]
    fn journal_coverage_is_volume_specific_and_not_assumed_for_networks() {
        let mut t = tracker();
        assert!(t.covers(Path::new(r"C:\Users\file.txt")));
        assert!(!t.covers(Path::new(r"T:\Work")));
        t.retries.insert(
            "C:".into(),
            Retry {
                attempts: 1,
                bad_batch: false,
                after: Instant::now(),
            },
        );
        assert!(!t.covers(Path::new(r"C:\Users")));
    }
    #[test]
    fn initially_unavailable_volume_rejoins_automatically() {
        let mut t = tracker();
        let now = Instant::now();
        t.read.remove("C:");
        t.retries.insert(
            "C:".into(),
            Retry {
                attempts: 1,
                bad_batch: false,
                after: now,
            },
        );
        let (paths, _) = t.poll_with(now, |_| Ok((1, 0, 100)), |_, m| Ok(batch(m.usn)));
        assert_eq!(paths, vec![PathBuf::from("C:\\Users")]);
        assert_eq!(t.active(), 2);
    }
    #[test]
    fn backoff_is_bounded_and_no_poll_storm() {
        assert_eq!(retry_delay(1).as_secs(), 5);
        assert_eq!(retry_delay(2).as_secs(), 10);
        assert_eq!(retry_delay(u32::MAX).as_secs(), 300);
        let mut t = tracker();
        let now = Instant::now();
        t.retries.insert(
            "C:".into(),
            Retry {
                attempts: 2,
                bad_batch: false,
                after: now,
            },
        );
        t.poll_with(
            now,
            |_| anyhow::bail!("volume offline"),
            |_, m| Ok(batch(m.usn)),
        );
        assert_eq!(t.retries["C:"].attempts, 3);
        t.poll_with(
            now + Duration::from_secs(19),
            |_| unreachable!(),
            |v, m| {
                assert_eq!(v, "D:");
                Ok(batch(m.usn))
            },
        );
        assert_eq!(t.read["C:"].usn, 10);
    }

    #[cfg(windows)]
    #[test]
    fn repeated_access_errors_do_not_skip_valid_journal_entries() {
        let mut t = tracker();
        let mut now = Instant::now();
        for attempt in 1..=5 {
            let (paths, global) = t.poll_with(
                now,
                |_| Ok((1, 0, 100)),
                |v, m| {
                    if v == "C:" {
                        return Err(windows::core::Error::from_hresult(windows::core::HRESULT(
                            0x80070005u32 as i32,
                        ))
                        .into());
                    }
                    Ok(batch(m.usn))
                },
            );
            assert!(paths.is_empty() && !global);
            assert_eq!(t.read["C:"].usn, 10);
            assert_eq!(t.retries["C:"].attempts, attempt);
            now += retry_delay(attempt);
        }
        let (paths, _) = t.poll_with(now, |_| Ok((1, 0, 100)), |_, m| Ok(batch(m.usn + 1)));
        assert!(paths.is_empty());
        assert_eq!(t.read["C:"].usn, 11);
    }
    #[test]
    fn repeated_malformed_batch_recovers_scope_and_resumes() {
        let mut t = tracker();
        let now = Instant::now();
        t.retries.insert(
            "C:".into(),
            Retry {
                attempts: 3,
                bad_batch: true,
                after: now,
            },
        );
        let (paths, global) = t.poll_with(now, |_| Ok((1, 0, 100)), |_, m| Ok(batch(m.usn)));
        assert!(!global);
        assert_eq!(paths, vec![PathBuf::from("C:\\Users")]);
        assert_eq!(t.read["C:"].usn, 100);
        assert_eq!(t.active(), 2);
    }
    #[test]
    fn repeated_unresolved_warnings_are_rate_limited_but_paths_not_lost() {
        let (tx, rx) = crossbeam_channel::unbounded();
        let mut t = tracker();
        t.logger = Some(tx.into());
        let now = Instant::now();
        for second in [0, 1, 60] {
            let (paths, _) = t.poll_with(
                now + Duration::from_secs(second),
                |_| unreachable!(),
                |v, m| {
                    let mut b = batch(m.usn + 1);
                    if v == "C:" {
                        b.unresolved = Some("unavailable parent".into());
                    }
                    Ok(b)
                },
            );
            assert!(paths.is_empty());
        }
        assert_eq!(rx.try_iter().count(), 2);
    }
    #[test]
    fn checkpoints_preserve_unavailable_volume_cursor() {
        let mut t = tracker();
        let home = tempfile::tempdir().unwrap();
        t.committed.insert("E:".into(), Mark { id: 3, usn: 5 });
        t.begin();
        t.finish(true, home.path());
        let saved: Saved =
            serde_json::from_slice(&std::fs::read(home.path().join("ntfs-journal.json")).unwrap())
                .unwrap();
        assert_eq!(saved.marks["E:"].usn, 5);
    }
    #[test]
    fn diagnostics_include_volume_cause_retry_and_resume() {
        let (tx, rx) = crossbeam_channel::unbounded();
        let mut t = tracker();
        t.logger = Some(tx.into());
        let now = Instant::now();
        t.poll_with(
            now,
            |_| unreachable!(),
            |v, m| {
                if v == "C:" {
                    anyhow::bail!("injected access failure")
                }
                Ok(batch(m.usn))
            },
        );
        t.poll_with(
            now + Duration::from_secs(5),
            |_| Ok((1, 0, 100)),
            |_, m| Ok(batch(m.usn)),
        );
        let logs: Vec<_> = rx
            .try_iter()
            .filter_map(|e| {
                if let crate::engine::Event::Log(_, s) = e {
                    Some(s)
                } else {
                    None
                }
            })
            .collect();
        assert!(logs.iter().any(|s| s.contains("C:")
            && s.contains("retry in 5s")
            && s.contains("injected access failure")));
        assert!(
            logs.iter()
                .any(|s| s.contains("C:") && s.contains("resumed"))
        );
    }
}

#[cfg(all(test, windows))]
mod windows_replay_test {
    use super::*;
    #[test]
    #[ignore = "Requires an accessible NTFS journal; writes only to explicit test directory"]
    fn live_create_delete_and_parent_resolution() {
        let base = PathBuf::from(
            std::env::var("VECTORDASH_JOURNAL_TEST_DIR").expect("explicit test directory"),
        );
        let text = base.to_string_lossy();
        let volume = &text[..2];
        let (id, _, start) = query(volume).unwrap();
        let root = tempfile::tempdir_in(&base).unwrap();
        let live = root.path().join("journal-live.txt");
        std::fs::write(&live, b"journal replay fixture").unwrap();
        let gone = root.path().join("deleted-parent");
        std::fs::create_dir(&gone).unwrap();
        std::fs::write(gone.join("child.txt"), b"deleted fixture").unwrap();
        std::fs::remove_file(gone.join("child.txt")).unwrap();
        std::fs::remove_dir(&gone).unwrap();
        let (_, _, end) = query(volume).unwrap();
        let mut mark = Mark { id, usn: start };
        let mut found = false;
        let mut unresolved = false;
        for _ in 0..128 {
            let batch =
                read(volume, mark).expect("V2 replay must tolerate unresolved deleted parents");
            found |= batch.paths.iter().any(|p| p == &live);
            unresolved |= batch.unresolved.is_some();
            if batch.next == mark.usn || batch.next >= end {
                break;
            }
            mark.usn = batch.next;
        }
        assert!(found, "new file must be present in replay");
        eprintln!(
            "Live NTFS replay: created file found={found}, unresolved deleted parent tolerated={unresolved}"
        );
    }
}

#[cfg(test)]
mod source_coverage_tests {
    use super::*;
    fn settings() -> crate::config::Settings {
        let mut s = crate::config::Settings::default();
        s.encrypt_index = false;
        s.ntfs_journal = true;
        s.roots = ["C:\\", "D:\\", "T:\\"]
            .iter()
            .map(|p| crate::config::Source {
                path: (*p).into(),
                mode: "quick".into(),
            })
            .collect();
        s
    }
    fn logger() -> crate::engine::NotifySender {
        let (tx, _) = crossbeam_channel::unbounded();
        tx.into()
    }
    #[test]
    fn network_source_never_queries_journal_or_invalidates_healthy_sources() {
        let home = tempfile::tempdir().unwrap();
        let s = settings();
        let mut first = Tracker::default();
        first.configure_with(
            &s,
            home.path(),
            &logger(),
            |v| {
                assert_ne!(v, "T:");
                Ok((1, 0, 100))
            },
            |v| v != "T:",
        );
        first.begin();
        first.finish(true, home.path());
        let mut resumed = Tracker::default();
        let uncovered = resumed.configure_with(
            &s,
            home.path(),
            &logger(),
            |v| {
                assert_ne!(v, "T:");
                Ok((1, 0, 101))
            },
            |v| v != "T:",
        );
        assert_eq!(uncovered, vec![PathBuf::from("T:\\")]);
        assert_eq!(resumed.active(), 2);
        assert_eq!(resumed.retrying(), 0);
    }
    #[test]
    fn startup_access_retry_waits_for_saved_cursor_validation_without_scan() {
        let home = tempfile::tempdir().unwrap();
        let mut s = settings();
        s.roots.truncate(2);
        let mut first = Tracker::default();
        first.configure_with(&s, home.path(), &logger(), |_| Ok((1, 0, 100)), |_| true);
        first.begin();
        first.finish(true, home.path());
        let mut resumed = Tracker::default();
        let uncovered = resumed.configure_with(
            &s,
            home.path(),
            &logger(),
            |v| {
                if v == "C:" {
                    anyhow::bail!("not ready after boot")
                }
                Ok((1, 0, 101))
            },
            |_| true,
        );
        assert!(uncovered.is_empty());
        assert_eq!(resumed.committed["C:"].usn, 100);
        resumed.retries.get_mut("C:").unwrap().after = Instant::now();
        let (paths, global) = resumed.poll_with(
            Instant::now(),
            |_| Ok((1, 0, 120)),
            |_, m| {
                Ok(super::Batch {
                    next: m.usn + 1,
                    paths: vec![],
                    unresolved: None,
                    missing_names: vec![],
                })
            },
        );
        assert!(paths.is_empty() && !global);
        assert_eq!(resumed.read["C:"].usn, 101);
    }
    #[test]
    fn adding_source_on_healthy_volume_only_catches_up_new_source() {
        let home = tempfile::tempdir().unwrap();
        let mut s = settings();
        s.roots.truncate(2);
        let mut first = Tracker::default();
        first.configure_with(&s, home.path(), &logger(), |_| Ok((1, 0, 100)), |_| true);
        first.begin();
        first.finish(true, home.path());
        s.roots.push(crate::config::Source {
            path: "C:\\New photos".into(),
            mode: "full".into(),
        });
        let mut resumed = Tracker::default();
        let uncovered =
            resumed.configure_with(&s, home.path(), &logger(), |_| Ok((1, 0, 101)), |_| true);
        assert_eq!(uncovered, vec![PathBuf::from("C:\\New photos")]);
        assert_eq!(resumed.read["C:"].usn, 100);
    }
    #[test]
    fn one_expired_drive_does_not_invalidate_other_drive() {
        let home = tempfile::tempdir().unwrap();
        let mut s = settings();
        s.roots.truncate(2);
        let mut first = Tracker::default();
        first.configure_with(&s, home.path(), &logger(), |_| Ok((1, 0, 100)), |_| true);
        first.begin();
        first.finish(true, home.path());
        let mut resumed = Tracker::default();
        let uncovered = resumed.configure_with(
            &s,
            home.path(),
            &logger(),
            |v| Ok((1, if v == "C:" { 101 } else { 0 }, 200)),
            |_| true,
        );
        assert_eq!(uncovered, vec![PathBuf::from("C:\\")]);
        assert_eq!(resumed.read["D:"].usn, 100);
    }
    #[test]
    fn hidden_policy_change_requires_source_catchup_even_if_legacy_signature_matches() {
        let home = tempfile::tempdir().unwrap();
        let mut s = settings();
        s.roots.truncate(1);
        let mut first = Tracker::default();
        first.configure_with(&s, home.path(), &logger(), |_| Ok((1, 0, 100)), |_| true);
        first.begin();
        first.finish(true, home.path());
        s.include_hidden = !s.include_hidden;
        let mut resumed = Tracker::default();
        let uncovered =
            resumed.configure_with(&s, home.path(), &logger(), |_| Ok((1, 0, 101)), |_| true);
        assert_eq!(uncovered, vec![PathBuf::from("C:\\")]);
    }
}
