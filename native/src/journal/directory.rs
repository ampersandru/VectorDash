use super::{Batch, Mark, Record};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
const MAX_ENTRIES: usize = 4096;
const MAX_BYTES: usize = 2 * 1024 * 1024;
#[derive(Clone)]
struct Entry {
    path: PathBuf,
    tick: u64,
    cost: usize,
}
#[derive(Clone, Default)]
pub(super) struct DirectoryCache {
    entries: BTreeMap<(String, u64), Entry>,
    order: BTreeMap<u64, (String, u64)>,
    clock: u64,
    bytes: usize,
    cursors: BTreeMap<String, Mark>,
}
#[derive(Serialize, Deserialize)]
struct Stored {
    marks: BTreeMap<String, Mark>,
    entries: Vec<(String, u64, PathBuf)>,
}
impl DirectoryCache {
    pub(super) fn align(&mut self, volume: &str, mark: Mark) -> bool {
        if self
            .cursors
            .get(volume)
            .is_some_and(|old| old.id == mark.id && old.usn == mark.usn)
        {
            return false;
        }
        let keys = self
            .entries
            .keys()
            .filter(|(v, _)| v == volume)
            .cloned()
            .collect::<Vec<_>>();
        for key in keys {
            self.remove(&key);
        }
        self.cursors.insert(volume.into(), mark);
        true
    }
    fn remove(&mut self, key: &(String, u64)) {
        if let Some(old) = self.entries.remove(key) {
            self.order.remove(&old.tick);
            self.bytes -= old.cost;
        }
    }
    pub(super) fn insert(&mut self, volume: &str, id: u64, path: PathBuf) {
        let cost = path.to_string_lossy().len() * 2 + 192;
        if cost > MAX_BYTES / 4 {
            return;
        }
        let key = (volume.to_owned(), id);
        self.remove(&key);
        self.clock = self.clock.saturating_add(1);
        self.entries.insert(
            key.clone(),
            Entry {
                path,
                tick: self.clock,
                cost,
            },
        );
        self.order.insert(self.clock, key);
        self.bytes += cost;
        while self.entries.len() > MAX_ENTRIES || self.bytes > MAX_BYTES {
            let Some((_, key)) = self.order.first_key_value().map(|(t, k)| (*t, k.clone())) else {
                break;
            };
            self.remove(&key);
        }
    }
    pub(super) fn get(&mut self, volume: &str, id: u64) -> Option<PathBuf> {
        let path = self.entries.get(&(volume.to_owned(), id))?.path.clone();
        self.insert(volume, id, path.clone());
        Some(path)
    }
    fn rename(&mut self, volume: &str, id: u64, new_path: PathBuf) {
        if let Some(old) = self
            .entries
            .get(&(volume.to_owned(), id))
            .map(|e| e.path.clone())
        {
            let changed = self
                .entries
                .iter()
                .filter(|((v, _), e)| v == volume && e.path.starts_with(&old))
                .filter_map(|((_, id), e)| {
                    e.path
                        .strip_prefix(&old)
                        .ok()
                        .map(|tail| (*id, new_path.join(tail)))
                })
                .collect::<Vec<_>>();
            for (id, path) in changed {
                self.insert(volume, id, path);
            }
        }
        self.insert(volume, id, new_path);
    }
    pub(super) fn save(&self, home: &Path, encrypt: bool) -> Result<()> {
        let stored = Stored {
            marks: self.cursors.clone(),
            entries: self
                .order
                .values()
                .filter_map(|key| {
                    self.entries
                        .get(key)
                        .map(|e| (key.0.clone(), key.1, e.path.clone()))
                })
                .collect(),
        };
        let bytes = serde_json::to_vec(&stored)?;
        let mut envelope = vec![if encrypt { b'E' } else { b'P' }];
        envelope.extend(if encrypt {
            crate::config::crypt(&bytes, false)?
        } else {
            bytes
        });
        let temporary = home.join("ntfs-directories.pending");
        std::fs::write(&temporary, envelope)?;
        std::fs::rename(temporary, home.join("ntfs-directories.cache"))?;
        Ok(())
    }
    pub(super) fn load(home: &Path, encrypt: bool, marks: &BTreeMap<String, Mark>) -> Result<Self> {
        let path = home.join("ntfs-directories.cache");
        if !path.exists() {
            return Ok(Self::default());
        }
        anyhow::ensure!(
            std::fs::metadata(&path)?.len() <= 8 * 1024 * 1024,
            "Directory cache exceeds size limit"
        );
        let bytes = std::fs::read(path)?;
        let bytes = match bytes.first() {
            Some(b'E') => crate::config::crypt(&bytes[1..], true)?,
            Some(b'P') if !encrypt => bytes[1..].to_vec(),
            _ => anyhow::bail!("Directory cache protection differs from index settings"),
        };
        let stored: Stored = serde_json::from_slice(&bytes)?;
        let mut cache = Self::default();
        for (volume, mark) in stored.marks {
            if marks
                .get(&volume)
                .is_some_and(|m| m.id == mark.id && m.usn == mark.usn)
            {
                cache.cursors.insert(volume, mark);
            }
        }
        for (volume, id, path) in stored.entries.into_iter().take(MAX_ENTRIES) {
            if cache.cursors.contains_key(&volume)
                && crate::config::under(&path.to_string_lossy(), &format!("{volume}\\"))
            {
                cache.insert(&volume, id, path);
            }
        }
        Ok(cache)
    }
}
/// Interpret records in order, retaining directory tombstones until LRU eviction.
/// The complete file reference (including its reuse sequence) is the cache key.
pub(super) fn resolve_records(
    volume: &str,
    mark: Mark,
    next: i64,
    records: Vec<Record>,
    cache: &mut DirectoryCache,
    mut resolve: impl FnMut(u64) -> Result<PathBuf>,
) -> Result<Batch> {
    anyhow::ensure!(next >= mark.usn, "Journal cursor moved backwards");
    cache.align(volume, mark);
    let mut paths = vec![];
    let mut unresolved = None;
    let mut missing_names = std::collections::BTreeSet::new();
    let mut missing = std::collections::BTreeSet::new();
    for record in records {
        if record.reason & !0x80000000 == 0 {
            continue;
        }
        let directory = record.attributes & 0x10 != 0;
        if directory && record.reason & (0x100 | 0x200 | 0x1000 | 0x2000) == 0 {
            continue;
        }
        if directory && record.reason & 0x200 != 0 {
            if let Some(path) = cache.get(volume, record.id) {
                if !crate::config::ntfs_internal_path(&path.to_string_lossy()) {
                    paths.push(path);
                }
                continue;
            }
        }
        let parent = match cache.get(volume, record.parent) {
            Some(path) => path,
            None => match (if missing.contains(&record.parent) {
                Err(anyhow::anyhow!("parent already unavailable in this batch"))
            } else { resolve(record.parent) })
                .with_context(|| format!("resolve journal parent ID {}", record.parent))
            {
                Ok(path) => {
                    cache.insert(volume, record.parent, path.clone());
                    path
                }
                Err(error) => {
                    // A live child can still identify its full path when the
                    // parent's directory handle is inaccessible. Deleted/old-name
                    // records instead use indexed-name checks for stale entries.
                    if record.reason & (0x200 | 0x1000) == 0 {
                        if let Ok(path) = resolve(record.id) {
                            if directory { cache.insert(volume, record.id, path.clone()); }
                            if !crate::config::ntfs_internal_path(&path.to_string_lossy()) { paths.push(path); }
                            continue;
                        }
                    }
                    missing_names.insert(record.name.clone());
                    unresolved.get_or_insert_with(|| {
                        format!("parent ID {} unavailable: {error:#}", record.parent)
                    });
                    missing.insert(record.parent);
                    continue;
                }
            },
        };
        let path = parent.join(&record.name);
        if directory {
            if record.reason & 0x2000 != 0 {
                cache.rename(volume, record.id, path.clone());
            } else {
                cache.insert(volume, record.id, path.clone());
            }
        }
        if !crate::config::ntfs_internal_path(&path.to_string_lossy()) {
            paths.push(path);
        }
    }
    if let Some(sample) = unresolved.as_mut() {
        *sample = format!("{} unresolved parent ID(s); {sample}", missing.len());
    }
    cache.cursors.insert(
        volume.into(),
        Mark {
            id: mark.id,
            usn: next,
        },
    );
    Ok(Batch {
        next,
        paths,
        unresolved,
        missing_names: missing_names.into_iter().collect(),
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    fn rec(id: u64, parent: u64, name: &str, reason: u32, directory: bool) -> Record {
        Record {
            id,
            parent,
            name: name.into(),
            reason,
            attributes: if directory { 16 } else { 0 },
        }
    }
    #[test]
    fn deleted_parent_is_resolved_from_bounded_tombstone_cache() {
        let mark = Mark { id: 1, usn: 10 };
        let mut cache = DirectoryCache::default();
        cache.align("C:", mark);
        cache.insert("C:", 100, PathBuf::from("C:\\Users\\Photos"));
        let batch = resolve_records(
            "C:",
            mark,
            20,
            vec![rec(200, 100, "gone.jpg", 0x200, false)],
            &mut cache,
            |_| anyhow::bail!("parent deleted"),
        )
        .unwrap();
        assert_eq!(
            batch.paths,
            vec![PathBuf::from("C:\\Users\\Photos\\gone.jpg")]
        );
        assert!(batch.unresolved.is_none());
    }
    #[test]
    fn rename_updates_cached_descendants_and_retains_old_path_event() {
        let mark = Mark { id: 1, usn: 10 };
        let mut c = DirectoryCache::default();
        c.align("C:", mark);
        c.insert("C:", 1, PathBuf::from("C:\\Users"));
        c.insert("C:", 2, PathBuf::from("C:\\Users\\old"));
        c.insert("C:", 3, PathBuf::from("C:\\Users\\old\\child"));
        let batch = resolve_records(
            "C:",
            mark,
            20,
            vec![
                rec(2, 1, "old", 0x1000, true),
                rec(2, 1, "new", 0x2000, true),
                rec(4, 3, "file.txt", 1, false),
            ],
            &mut c,
            |_| unreachable!(),
        )
        .unwrap();
        assert!(batch.paths.contains(&PathBuf::from("C:\\Users\\old")));
        assert!(
            batch
                .paths
                .contains(&PathBuf::from("C:\\Users\\new\\child\\file.txt"))
        );
    }
    #[test]
    fn internal_metadata_never_queues_work_and_user_dollar_names_survive() {
        let mark = Mark { id: 1, usn: 10 };
        let mut c = DirectoryCache::default();
        let batch = resolve_records(
            "C:",
            mark,
            20,
            vec![
                rec(2, 1, "$Extend", 0x100, true),
                rec(4, 2, "$Deleted", 0x100, true),
                rec(5, 4, "abc", 1, false),
                rec(6, 1, "$Budget.txt", 1, false),
            ],
            &mut c,
            |id| {
                assert_eq!(id, 1);
                Ok(PathBuf::from("C:\\"))
            },
        )
        .unwrap();
        assert_eq!(batch.paths, vec![PathBuf::from("C:\\$Budget.txt")]);
        assert!(batch.unresolved.is_none());
    }

    #[cfg(windows)]
    #[test]
    fn protected_cache_roundtrips_without_plaintext_directory_names() {
        let home = tempfile::tempdir().unwrap();
        let mark = Mark { id: 1, usn: 10 };
        let mut c = DirectoryCache::default();
        c.align("C:", mark);
        c.insert(
            "C:",
            2,
            PathBuf::from("C:\\UniquePrivateDirectoryForCacheTest"),
        );
        c.save(home.path(), true).unwrap();
        let raw = std::fs::read(home.path().join("ntfs-directories.cache")).unwrap();
        assert_eq!(raw[0], b'E');
        assert!(
            !raw.windows(b"UniquePrivateDirectoryForCacheTest".len())
                .any(|w| w == b"UniquePrivateDirectoryForCacheTest")
        );
        let mut loaded =
            DirectoryCache::load(home.path(), true, &BTreeMap::from([("C:".into(), mark)]))
                .unwrap();
        assert!(loaded.get("C:", 2).is_some());
    }
    #[test]
    fn warm_directory_cache_avoids_repeat_handle_lookups() {
        let mut c = DirectoryCache::default();
        let mut mark = Mark { id: 1, usn: 0 };
        let mut resolutions = 0;
        for n in 1..=100 {
            let batch = resolve_records(
                "C:",
                mark,
                n,
                vec![rec(2, 1, "file.txt", 1, false)],
                &mut c,
                |_| {
                    resolutions += 1;
                    Ok(PathBuf::from("C:\\Users\\Photos"))
                },
            )
            .unwrap();
            assert!(batch.unresolved.is_none());
            mark.usn = n;
        }
        assert_eq!(resolutions, 1);
    }
    #[test]
    fn journal_gap_invalidates_cached_file_references() {
        let mark = Mark { id: 1, usn: 10 };
        let mut c = DirectoryCache::default();
        c.align("C:", mark);
        c.insert("C:", 2, PathBuf::from("C:\\old"));
        c.align("C:", Mark { id: 1, usn: 11 });
        assert!(c.get("C:", 2).is_none());
        c.insert("C:", 2, PathBuf::from("C:\\old"));
        c.align("C:", Mark { id: 2, usn: 11 });
        assert!(c.get("C:", 2).is_none());
    }
    #[test]
    fn cache_is_bounded_and_persistent_only_for_matching_durable_cursor() {
        let home = tempfile::tempdir().unwrap();
        let mark = Mark { id: 1, usn: 10 };
        let mut c = DirectoryCache::default();
        c.align("C:", mark);
        for i in 0..10000 {
            c.insert("C:", i, PathBuf::from(format!("C:\\folder-{i}")));
        }
        assert!(c.entries.len() <= MAX_ENTRIES && c.bytes <= MAX_BYTES);
        assert!(c.get("C:", 0).is_none());
        c.save(home.path(), false).unwrap();
        let mut loaded =
            DirectoryCache::load(home.path(), false, &BTreeMap::from([("C:".into(), mark)]))
                .unwrap();
        assert!(loaded.get("C:", 9999).is_some());
        let loaded = DirectoryCache::load(
            home.path(),
            false,
            &BTreeMap::from([("C:".into(), Mark { id: 1, usn: 11 })]),
        )
        .unwrap();
        assert!(loaded.entries.is_empty());
    }
}

#[cfg(test)]
mod targeted_missing_tests {
    use super::*;
    #[test]
    fn inaccessible_parent_resolves_live_child_and_retains_deleted_name_for_lookup() {
        let mut cache = DirectoryCache::default();
        let batch = resolve_records("C:", Mark { id: 1, usn: 10 }, 20, vec![
            Record { id: 2, parent: 1, name: "new.txt".into(), reason: 0x100, attributes: 0 },
            Record { id: 3, parent: 1, name: "gone.txt".into(), reason: 0x200, attributes: 0 },
            Record { id: 4, parent: 1, name: "other.txt".into(), reason: 0x200, attributes: 0 },
        ], &mut cache, |id| {
            if id == 2 { Ok(PathBuf::from(r"C:\Users\new.txt")) }
            else { anyhow::bail!("access denied") }
        }).unwrap();
        assert_eq!(batch.paths, vec![PathBuf::from(r"C:\Users\new.txt")]);
        assert_eq!(batch.missing_names, vec!["gone.txt", "other.txt"]);
        assert!(batch.unresolved.is_some());
    }
}
