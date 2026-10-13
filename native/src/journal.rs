//! Bounded optional NTFS USN replay; persist cursors only after successful indexing.
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
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
}
#[derive(Default)]
pub struct Tracker {
    signature: String,
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
    ) -> bool {
        *self = Self::default();
        if !s.ntfs_journal {
            return false;
        }
        self.signature = format!(
            "{:?}|{:?}|{}|{}",
            s.roots, s.exclusions, s.excluded_types, s.skip_windows_churn
        );
        let saved = std::fs::read(home.join("ntfs-journal.json"))
            .ok()
            .and_then(|b| serde_json::from_slice::<Saved>(&b).ok())
            .unwrap_or_default();
        let signature_matches = saved.signature == self.signature;
        let mut covered = signature_matches;
        let mut volumes = BTreeMap::new();
        for root in &s.roots {
            let path = root.path.as_bytes();
            if path.len() >= 3 && path[0].is_ascii_alphabetic() && path[1] == b':' {
                volumes.insert(root.path[..2].to_uppercase(), ());
            } else {
                covered = false;
            }
        }
        for (volume, _) in volumes {
            match query(&volume) {
                Ok((id, first, next)) => {
                    let old = saved.marks.get(&volume).copied().filter(|m| {
                        signature_matches && m.id == id && m.usn >= first && m.usn <= next
                    });
                    if old.is_none() {
                        covered = false;
                    }
                    let mark = old.unwrap_or(Mark { id, usn: next });
                    self.read.insert(volume.clone(), mark);
                    self.committed.insert(volume, mark);
                }
                Err(e) => {
                    covered = false;
                    let _=tx.send(crate::engine::Event::Log("Warning".into(),format!("NTFS journal {volume} unavailable; folder notifications remain active: {e}")));
                }
            }
        }
        let _ = tx.send(crate::engine::Event::Log(
            "Info".into(),
            format!(
                "NTFS journal: {} accessible volumes; {}",
                self.read.len(),
                if covered {
                    "replaying saved cursors"
                } else {
                    "recovery scan establishes cursors"
                }
            ),
        ));
        covered && !self.read.is_empty()
    }
    pub fn poll(&mut self) -> (Vec<PathBuf>, bool) {
        let mut paths = vec![];
        let mut recovery = false;
        let mut failed = vec![];
        for (volume, mark) in &mut self.read {
            match read(volume, *mark) {
                Ok((next, mut batch)) => {
                    mark.usn = next;
                    paths.append(&mut batch)
                }
                Err(_) => {
                    failed.push(volume.clone());
                    recovery = true;
                }
            }
        }
        for v in failed {
            self.read.remove(&v);
        }
        (paths, recovery)
    }
    pub fn begin(&mut self) {
        self.flight = if self.read.is_empty() {
            None
        } else {
            Some(self.read.clone())
        };
    }
    pub fn finish(&mut self, success: bool, home: &Path) {
        if let Some(marks) = self.flight.take() {
            if success {
                let saved = Saved {
                    signature: self.signature.clone(),
                    marks: marks.clone(),
                };
                if let Ok(bytes) = serde_json::to_vec(&saved) {
                    let temp = home.join("ntfs-journal.pending");
                    if std::fs::write(&temp, bytes).is_ok()
                        && std::fs::rename(temp, home.join("ntfs-journal.json")).is_ok()
                    {
                        self.committed = marks;
                    }
                }
            } else {
                self.read = self.committed.clone();
            }
        }
    }
    pub fn active(&self) -> usize {
        self.read.len()
    }
}
#[derive(Debug)]
struct Record {
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
    pub(super) fn read(volume: &str, mark: Mark) -> Result<(i64, Vec<PathBuf>)> {
        let handle = open(volume)?;
        let input = READ_USN_JOURNAL_DATA_V0 {
            StartUsn: mark.usn,
            ReasonMask: u32::MAX,
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
        let (next, records) = parse(&buffer[..count as usize])?;
        let mut parents = BTreeMap::new();
        let mut paths = vec![];
        for r in records {
            if r.reason & !0x80000000 == 0 {
                continue;
            }
            if r.attributes & 0x10 != 0 && r.reason & (0x100 | 0x200 | 0x1000 | 0x2000) == 0 {
                continue;
            }
            let parent = if let Some(p) = parents.get(&r.parent) {
                PathBuf::clone(p)
            } else {
                let id = FILE_ID_DESCRIPTOR {
                    dwSize: std::mem::size_of::<FILE_ID_DESCRIPTOR>() as u32,
                    Type: FileIdType,
                    Anonymous: FILE_ID_DESCRIPTOR_0 {
                        FileId: r.parent as i64,
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
                    GetFinalPathNameByHandleW(
                        parent.0,
                        &mut text,
                        GETFINALPATHNAMEBYHANDLE_FLAGS(0),
                    )
                } as usize;
                anyhow::ensure!(n > 0 && n < text.len(), "Cannot resolve journal parent");
                let text = String::from_utf16(&text[..n])?;
                let path = PathBuf::from(text.strip_prefix("\\\\?\\").unwrap_or(&text));
                if parents.len() < 512 {
                    parents.insert(r.parent, path.clone());
                }
                path
            };
            paths.push(parent.join(r.name));
        }
        Ok((next, paths))
    }
}
#[cfg(windows)]
use win::{query, read};
#[cfg(not(windows))]
fn query(_: &str) -> Result<(u64, i64, i64)> {
    anyhow::bail!("NTFS journals require Windows")
}
#[cfg(not(windows))]
fn read(_: &str, _: Mark) -> Result<(i64, Vec<PathBuf>)> {
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
    let (_, records) = read(volume, Mark { id, usn: next })?;
    Ok(
        serde_json::json!({"accessible":true,"id":id,"first":first,"next":next,"pending_paths":records.len()}),
    )
}
