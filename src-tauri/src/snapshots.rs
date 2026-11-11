//! Local disk-usage snapshots and comparisons.
//!
//! A snapshot is written after a *completed* (not cancelled) analysis. It
//! records the volume identity, timing, the free space reported by the OS,
//! aggregated folder sizes (large or shallow folders only, to bound size) and
//! what could not be read. Snapshots never leave the machine.
//!
//! Two kinds of change are kept apart:
//! * **free space** — measured by the OS for the whole volume (includes
//!   other users, system files, Trash, shadow copies, ...);
//! * **folder sizes** — logical sizes of what this user could read.
//!
//! A folder that was unreadable in either snapshot is reported as such and
//! never counted as created or deleted.

use crate::disk_usage::{Analysis, VolumeInfo};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub const SNAPSHOT_VERSION: u32 = 1;
/// Maximum snapshots kept on disk (oldest deleted first).
pub const MAX_SNAPSHOTS: usize = 30;
/// Folders at or above this size are stored...
pub const MIN_ENTRY_BYTES: u64 = 32 * 1024 * 1024;
/// ...as are all folders this shallow (depth 1 = direct child of the root).
pub const ALWAYS_KEEP_DEPTH: usize = 2;
pub const MAX_ENTRIES: usize = 20_000;
const MAX_UNREADABLE: usize = 2_000;
const SEP: char = '\\';

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VolumeRef {
    pub root: String,
    pub serial: u32,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Scope {
    pub dirs_read: u64,
    pub dirs_unreadable: u64,
    pub files: u64,
    pub min_entry_bytes: u64,
    pub always_keep_depth: usize,
    /// More folders qualified than `MAX_ENTRIES`; smallest were dropped.
    pub entries_truncated: bool,
    /// The analysis hit its in-memory folder cap.
    pub tree_truncated: bool,
    pub unreadable_truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    /// Path relative to the volume root, `\`-separated; "" is the root.
    pub path: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub version: u32,
    pub id: String,
    pub volume: VolumeRef,
    pub started_at_ms: u64,
    pub taken_at_ms: u64,
    pub total_bytes: u64,
    /// Free space reported by the OS when the analysis finished.
    pub free_bytes: u64,
    /// Logical bytes of everything that could be read.
    pub logical_bytes: u64,
    pub scope: Scope,
    pub entries: Vec<Entry>,
    /// Relative paths of folders that could not be listed.
    pub unreadable: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotMeta {
    pub id: String,
    pub volume: VolumeRef,
    pub taken_at_ms: u64,
    pub total_bytes: u64,
    pub free_bytes: u64,
    pub logical_bytes: u64,
    pub dirs_unreadable: u64,
    pub entries: usize,
}

impl Snapshot {
    pub fn meta(&self) -> SnapshotMeta {
        SnapshotMeta {
            id: self.id.clone(),
            volume: self.volume.clone(),
            taken_at_ms: self.taken_at_ms,
            total_bytes: self.total_bytes,
            free_bytes: self.free_bytes,
            logical_bytes: self.logical_bytes,
            dirs_unreadable: self.scope.dirs_unreadable,
            entries: self.entries.len(),
        }
    }

    /// Build from a finished analysis. Returns `None` for cancelled runs.
    pub fn from_analysis(a: &Analysis, volume: &VolumeInfo, free_bytes: u64, total_bytes: u64) -> Option<Self> {
        if a.cancelled {
            return None;
        }
        let rel = |id: u32| a.rel_components(id).join("\\");
        let mut qualifying: Vec<(u32, u64)> = Vec::new();
        let mut unreadable = Vec::new();
        let mut unreadable_truncated = false;
        for (i, n) in a.nodes.iter().enumerate() {
            let id = i as u32;
            if n.unreadable {
                if unreadable.len() < MAX_UNREADABLE {
                    unreadable.push(rel(id));
                } else {
                    unreadable_truncated = true;
                }
            }
            let depth = a.rel_components(id).len();
            if id == 0 || depth <= ALWAYS_KEEP_DEPTH || n.bytes >= MIN_ENTRY_BYTES {
                qualifying.push((id, n.bytes));
            }
        }
        let entries_truncated = qualifying.len() > MAX_ENTRIES;
        if entries_truncated {
            // Keep the root and the largest folders.
            qualifying.sort_by(|x, y| (y.0 == 0).cmp(&(x.0 == 0)).then(y.1.cmp(&x.1)));
            qualifying.truncate(MAX_ENTRIES);
        }
        let entries = qualifying
            .into_iter()
            .map(|(id, bytes)| Entry { path: rel(id), bytes })
            .collect();
        Some(Snapshot {
            version: SNAPSHOT_VERSION,
            id: format!("{}-{:08x}", a.finished_at_ms, volume.serial),
            volume: VolumeRef {
                root: volume.root.clone(),
                serial: volume.serial,
                label: volume.label.clone(),
            },
            started_at_ms: a.started_at_ms,
            taken_at_ms: a.finished_at_ms,
            total_bytes,
            free_bytes,
            logical_bytes: a.nodes[0].bytes,
            scope: Scope {
                dirs_read: a.dirs_read,
                dirs_unreadable: a.dirs_unreadable,
                files: a.files_seen,
                min_entry_bytes: MIN_ENTRY_BYTES,
                always_keep_depth: ALWAYS_KEEP_DEPTH,
                entries_truncated,
                tree_truncated: a.truncated,
                unreadable_truncated,
            },
            entries,
            unreadable,
        })
    }
}

// ---------------------------------------------------------------------------
// Storage
// ---------------------------------------------------------------------------

pub struct SnapshotStore {
    dir: PathBuf,
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

impl SnapshotStore {
    pub fn new(dir: PathBuf) -> Self {
        SnapshotStore { dir }
    }

    fn file(&self, id: &str) -> Result<PathBuf, String> {
        if !valid_id(id) {
            return Err("invalid snapshot id".into());
        }
        Ok(self.dir.join(format!("{id}.json")))
    }

    pub fn save(&self, snap: &Snapshot) -> Result<(), String> {
        std::fs::create_dir_all(&self.dir).map_err(|e| e.to_string())?;
        let path = self.file(&snap.id)?;
        let tmp = path.with_extension("json.tmp");
        let json = serde_json::to_vec(snap).map_err(|e| e.to_string())?;
        std::fs::write(&tmp, json).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
        self.prune(MAX_SNAPSHOTS)
    }

    pub fn load(&self, id: &str) -> Result<Snapshot, String> {
        let bytes = std::fs::read(self.file(id)?).map_err(|_| "snapshot not found".to_string())?;
        let snap: Snapshot = serde_json::from_slice(&bytes).map_err(|e| format!("corrupt snapshot: {e}"))?;
        if snap.version != SNAPSHOT_VERSION {
            return Err("unsupported snapshot version".into());
        }
        Ok(snap)
    }

    fn ids(&self) -> Vec<String> {
        let Ok(rd) = std::fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        rd.flatten()
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                name.strip_suffix(".json").filter(|id| valid_id(id)).map(str::to_string)
            })
            .collect()
    }

    /// Newest first. Unreadable or corrupt files are skipped.
    pub fn list(&self) -> Vec<SnapshotMeta> {
        let mut metas: Vec<SnapshotMeta> = self.ids().iter().filter_map(|id| self.load(id).ok()).map(|s| s.meta()).collect();
        metas.sort_by(|a, b| b.taken_at_ms.cmp(&a.taken_at_ms));
        metas
    }

    pub fn delete(&self, id: &str) -> Result<(), String> {
        std::fs::remove_file(self.file(id)?).map_err(|e| e.to_string())
    }

    pub fn clear(&self) -> Result<usize, String> {
        let ids = self.ids();
        for id in &ids {
            self.delete(id)?;
        }
        Ok(ids.len())
    }

    fn prune(&self, keep: usize) -> Result<(), String> {
        let metas = self.list();
        for m in metas.iter().skip(keep) {
            self.delete(&m.id)?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Comparison
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeltaStatus {
    /// Present in both snapshots.
    Changed,
    /// Only in the newer snapshot; exact when shallow enough to always be
    /// recorded, otherwise it may have existed below the size threshold.
    Added,
    /// Only in the older snapshot (same caveat).
    Removed,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirDelta {
    pub path: String,
    pub older_bytes: Option<u64>,
    pub newer_bytes: Option<u64>,
    pub delta: i64,
    pub status: DeltaStatus,
    /// The missing side may have been below the storage threshold, so the
    /// real change can be smaller by up to `min_entry_bytes`.
    pub approximate: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Comparison {
    pub older: SnapshotMeta,
    pub newer: SnapshotMeta,
    pub interval_ms: u64,
    /// newer.free - older.free, measured by the OS for the whole volume.
    pub free_delta: i64,
    /// Change in logical size of everything readable.
    pub logical_delta: i64,
    pub partial: bool,
    pub partial_reasons: Vec<String>,
    pub growers: Vec<DirDelta>,
    pub shrinkers: Vec<DirDelta>,
    /// Folders left out of the comparison because either side could not
    /// read them.
    pub unreadable_skipped: usize,
}

fn is_under(path: &str, ancestor: &str) -> bool {
    if ancestor.is_empty() {
        return true;
    }
    let (p, a) = (path.to_lowercase(), ancestor.to_lowercase());
    p == a || (p.starts_with(&a) && p[a.len()..].starts_with(SEP))
}

fn depth(path: &str) -> usize {
    if path.is_empty() {
        0
    } else {
        path.split(SEP).count()
    }
}

pub fn compare(a: &Snapshot, b: &Snapshot, limit: usize) -> Result<Comparison, String> {
    if a.volume.serial != b.volume.serial || !a.volume.root.eq_ignore_ascii_case(&b.volume.root) {
        return Err("snapshots are from different volumes and cannot be compared".into());
    }
    let (older, newer) = if a.taken_at_ms <= b.taken_at_ms { (a, b) } else { (b, a) };

    let mut reasons = Vec::new();
    if older.scope.dirs_unreadable > 0 || newer.scope.dirs_unreadable > 0 {
        reasons.push(format!(
            "some folders could not be read ({} older, {} newer)",
            older.scope.dirs_unreadable, newer.scope.dirs_unreadable
        ));
    }
    let mut ou = older.unreadable.clone();
    let mut nu = newer.unreadable.clone();
    ou.sort();
    nu.sort();
    if ou != nu {
        reasons.push("the set of unreadable folders differs between the scans".into());
    }
    if older.scope.min_entry_bytes != newer.scope.min_entry_bytes
        || older.scope.always_keep_depth != newer.scope.always_keep_depth
    {
        reasons.push("snapshots were stored with different detail settings".into());
    }
    if older.scope.entries_truncated || newer.scope.entries_truncated {
        reasons.push("a snapshot hit its folder limit; small folders are missing".into());
    }
    if older.scope.tree_truncated || newer.scope.tree_truncated {
        reasons.push("an analysis hit its in-memory folder cap".into());
    }
    if older.scope.unreadable_truncated || newer.scope.unreadable_truncated {
        reasons.push("too many unreadable folders to list them all".into());
    }

    // Folders inside an unreadable folder (either side) are left out. A
    // folder *containing* something that was unreadable on only one side is
    // still shown, but its delta is flagged approximate.
    let unreadable_any: Vec<&String> = older.unreadable.iter().chain(newer.unreadable.iter()).collect();
    let one_sided: Vec<&String> = older
        .unreadable
        .iter()
        .filter(|u| !newer.unreadable.contains(u))
        .chain(newer.unreadable.iter().filter(|u| !older.unreadable.contains(u)))
        .collect();
    let blocked = |p: &str| unreadable_any.iter().any(|u| is_under(p, u));
    let affected = |p: &str| one_sided.iter().any(|u| is_under(u, p));

    let om: HashMap<&str, u64> = older.entries.iter().map(|e| (e.path.as_str(), e.bytes)).collect();
    let nm: HashMap<&str, u64> = newer.entries.iter().map(|e| (e.path.as_str(), e.bytes)).collect();
    let keep_depth = older.scope.always_keep_depth.min(newer.scope.always_keep_depth);
    let threshold = older.scope.min_entry_bytes.max(newer.scope.min_entry_bytes);

    let mut deltas = Vec::new();
    let mut unreadable_skipped = 0usize;
    let mut paths: Vec<&str> = om.keys().chain(nm.keys()).copied().collect();
    paths.sort_unstable();
    paths.dedup();
    for p in paths {
        if p.is_empty() {
            continue; // root is reported as logical_delta
        }
        // A folder (or an ancestor/descendant) that could not be read on
        // either side is neither "added" nor "removed".
        if blocked(p) {
            unreadable_skipped += 1;
            continue;
        }
        let exact_presence = depth(p) <= keep_depth;
        let mut d = match (om.get(p), nm.get(p)) {
            (Some(&o), Some(&n)) => DirDelta {
                path: p.to_string(),
                older_bytes: Some(o),
                newer_bytes: Some(n),
                delta: n as i64 - o as i64,
                status: DeltaStatus::Changed,
                approximate: false,
            },
            (None, Some(&n)) => DirDelta {
                path: p.to_string(),
                older_bytes: None,
                newer_bytes: Some(n),
                delta: n as i64,
                status: DeltaStatus::Added,
                approximate: !exact_presence,
            },
            (Some(&o), None) => DirDelta {
                path: p.to_string(),
                older_bytes: Some(o),
                newer_bytes: None,
                delta: -(o as i64),
                status: DeltaStatus::Removed,
                approximate: !exact_presence,
            },
            (None, None) => unreachable!(),
        };
        if affected(p) {
            d.approximate = true;
        }
        // Entries that may merely have crossed the storage threshold are only
        // meaningful if the change exceeds that threshold.
        if d.status != DeltaStatus::Changed && d.approximate && d.delta.unsigned_abs() < threshold {
            continue;
        }
        if d.delta != 0 {
            deltas.push(d);
        }
    }

    let limit = limit.clamp(1, 500);
    let mut growers: Vec<DirDelta> = deltas.iter().filter(|d| d.delta > 0).cloned().collect();
    growers.sort_by(|x, y| y.delta.cmp(&x.delta));
    growers.truncate(limit);
    let mut shrinkers: Vec<DirDelta> = deltas.into_iter().filter(|d| d.delta < 0).collect();
    shrinkers.sort_by(|x, y| x.delta.cmp(&y.delta));
    shrinkers.truncate(limit);

    Ok(Comparison {
        older: older.meta(),
        newer: newer.meta(),
        interval_ms: newer.taken_at_ms.saturating_sub(older.taken_at_ms),
        free_delta: newer.free_bytes as i64 - older.free_bytes as i64,
        logical_delta: newer.logical_bytes as i64 - older.logical_bytes as i64,
        partial: !reasons.is_empty(),
        partial_reasons: reasons,
        growers,
        shrinkers,
        unreadable_skipped,
    })
}

/// Helper for tests and the command layer.
pub fn store_dir(base: &Path) -> PathBuf {
    base.join("snapshots")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::disk_usage::analyze;
    use std::sync::atomic::AtomicBool;
    use tempfile::tempdir;

    const MB: u64 = 1024 * 1024;

    fn vol(serial: u32) -> VolumeInfo {
        VolumeInfo {
            root: "C:\\".into(),
            label: "OS".into(),
            file_system: "NTFS".into(),
            serial,
            total_bytes: 1000 * MB,
            free_bytes: 500 * MB,
            removable: false,
        }
    }

    fn snap(id: &str, t: u64, serial: u32, free: u64, entries: &[(&str, u64)], unreadable: &[&str]) -> Snapshot {
        Snapshot {
            version: SNAPSHOT_VERSION,
            id: id.into(),
            volume: VolumeRef {
                root: "C:\\".into(),
                serial,
                label: "OS".into(),
            },
            started_at_ms: t,
            taken_at_ms: t,
            total_bytes: 1000 * MB,
            free_bytes: free,
            logical_bytes: entries.iter().find(|e| e.0.is_empty()).map_or(0, |e| e.1),
            scope: Scope {
                dirs_read: 10,
                dirs_unreadable: unreadable.len() as u64,
                files: 10,
                min_entry_bytes: MIN_ENTRY_BYTES,
                always_keep_depth: ALWAYS_KEEP_DEPTH,
                entries_truncated: false,
                tree_truncated: false,
                unreadable_truncated: false,
            },
            entries: entries
                .iter()
                .map(|(p, b)| Entry {
                    path: p.to_string(),
                    bytes: *b,
                })
                .collect(),
            unreadable: unreadable.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn different_volumes_cannot_be_compared() {
        let a = snap("a", 1, 1, 0, &[("", 1)], &[]);
        let b = snap("b", 2, 2, 0, &[("", 1)], &[]);
        assert!(compare(&a, &b, 10).is_err());
    }

    #[test]
    fn reports_growth_free_space_and_interval() {
        let day = 86_400_000;
        let a = snap("a", 0, 7, 500 * MB, &[("", 300 * MB), ("Users", 200 * MB), ("Temp", 100 * MB)], &[]);
        let b = snap("b", 2 * day, 7, 380 * MB, &[("", 420 * MB), ("Users", 320 * MB), ("Temp", 100 * MB)], &[]);
        // Argument order does not matter.
        let c = compare(&b, &a, 10).unwrap();
        assert_eq!(c.interval_ms, 2 * day);
        assert_eq!(c.free_delta, -(120 * MB as i64));
        assert_eq!(c.logical_delta, 120 * MB as i64);
        assert!(!c.partial);
        assert_eq!(c.growers.len(), 1);
        assert_eq!(c.growers[0].path, "Users");
        assert_eq!(c.growers[0].delta, 120 * MB as i64);
    }

    #[test]
    fn unreadable_folder_is_not_counted_as_deleted() {
        let a = snap("a", 1, 7, 0, &[("", 300 * MB), ("Secret", 100 * MB), ("Users", 200 * MB)], &[]);
        let b = snap("b", 2, 7, 0, &[("", 200 * MB), ("Users", 200 * MB)], &["Secret"]);
        let c = compare(&a, &b, 10).unwrap();
        assert!(c.partial);
        assert!(c.shrinkers.iter().all(|d| d.path != "Secret"));
        assert_eq!(c.unreadable_skipped, 1);
        assert!(c.partial_reasons.iter().any(|r| r.contains("differs")));
    }

    #[test]
    fn different_scope_settings_mark_partial() {
        let a = snap("a", 1, 7, 0, &[("", 1)], &[]);
        let mut b = snap("b", 2, 7, 0, &[("", 1)], &[]);
        b.scope.min_entry_bytes = 1;
        let c = compare(&a, &b, 10).unwrap();
        assert!(c.partial);
        b.scope.min_entry_bytes = MIN_ENTRY_BYTES;
        b.scope.entries_truncated = true;
        assert!(compare(&a, &b, 10).unwrap().partial);
    }

    #[test]
    fn deep_new_folder_is_approximate() {
        let a = snap("a", 1, 7, 0, &[("", 0)], &[]);
        let b = snap("b", 2, 7, 0, &[("", 0), ("a\\b\\c", 100 * MB)], &[]);
        let c = compare(&a, &b, 10).unwrap();
        assert_eq!(c.growers[0].status, DeltaStatus::Added);
        assert!(c.growers[0].approximate);
    }

    #[test]
    fn store_roundtrip_prune_and_clear() {
        let t = tempdir().unwrap();
        let store = SnapshotStore::new(store_dir(t.path()));
        for i in 0..(MAX_SNAPSHOTS + 3) {
            store.save(&snap(&format!("s{i}"), i as u64, 7, 0, &[("", 1)], &[])).unwrap();
        }
        let list = store.list();
        assert_eq!(list.len(), MAX_SNAPSHOTS);
        assert_eq!(list[0].id, format!("s{}", MAX_SNAPSHOTS + 2), "newest first, oldest pruned");
        assert!(store.load("../etc").is_err());
        assert_eq!(store.clear().unwrap(), MAX_SNAPSHOTS);
        assert!(store.list().is_empty());
    }

    #[test]
    fn snapshot_from_analysis() {
        let t = tempdir().unwrap();
        std::fs::create_dir_all(t.path().join("a/b/c/d")).unwrap();
        std::fs::write(t.path().join("a/b/c/d/f"), b"12345").unwrap();
        let a = analyze("x", t.path(), &AtomicBool::new(false), |_| {});
        let s = Snapshot::from_analysis(&a, &vol(9), 1, 2).unwrap();
        // Root + depth<=2 are kept; the tiny deep folders are not.
        let paths: Vec<_> = s.entries.iter().map(|e| e.path.as_str()).collect();
        assert!(paths.contains(&""));
        assert!(paths.contains(&"a"));
        assert!(paths.contains(&"a\\b"));
        assert!(!paths.contains(&"a\\b\\c"));
        assert_eq!(s.logical_bytes, 5);

        let cancelled = analyze("y", t.path(), &AtomicBool::new(true), |_| {});
        assert!(Snapshot::from_analysis(&cancelled, &vol(9), 1, 2).is_none());
    }
}
