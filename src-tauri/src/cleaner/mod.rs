//! Cleanup coordinator.
//!
//! The UI can only refer to candidates by the opaque id issued during the
//! most recent scan; it can never pass an arbitrary path to delete. Every item
//! is revalidated immediately before it is moved to the Trash.

use crate::rules;
use crate::safety::{self, ProtectedPaths};
use crate::scanner::CandidateRecord;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CleanStatus {
    /// Moved to the OS Trash / Recycle Bin.
    Trashed,
    /// Not touched because a safety check did not pass.
    Skipped,
    /// The OS refused the operation (locked file, permissions, ...).
    Failed,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanItemResult {
    pub id: String,
    pub path: String,
    pub status: CleanStatus,
    pub message: String,
    pub size_bytes: u64,
}

/// Abstraction over the destructive step so tests never touch the real Trash.
pub trait Remover {
    fn remove(&self, path: &Path) -> Result<(), String>;
}

pub struct TrashRemover;

impl Remover for TrashRemover {
    fn remove(&self, path: &Path) -> Result<(), String> {
        trash::delete(path).map_err(|e| e.to_string())
    }
}

/// Re-check everything we know about a candidate. Returns a reason on failure.
pub fn revalidate(record: &CandidateRecord, protected: &ProtectedPaths) -> Result<(), String> {
    let path = &record.path;

    let meta = std::fs::symlink_metadata(path).map_err(|_| "no longer exists".to_string())?;
    if safety::is_link_like(&meta) {
        return Err("path is now a link or junction".into());
    }
    if !meta.is_dir() {
        return Err("path is no longer a directory".into());
    }
    if safety::dir_identity(path, &meta) != record.identity {
        return Err("folder was replaced since the scan".into());
    }

    // The resolved path must be the one we scanned (no parent was swapped
    // for a link) and must sit strictly inside the authorised scan root.
    let canonical = path
        .canonicalize()
        .map_err(|e| format!("cannot resolve path: {e}"))?;
    if &canonical != path {
        return Err("path resolves somewhere else than at scan time".into());
    }
    if canonical == record.root || !canonical.starts_with(&record.root) {
        return Err("path is outside the authorised scan root".into());
    }
    protected.check(&canonical)?;

    if !rules::rule_still_matches(record.rule, &canonical) {
        return Err("project context no longer matches the rule".into());
    }
    Ok(())
}

/// Clean the given candidate ids. Duplicate ids and items nested inside
/// another selected item are skipped rather than processed twice.
pub fn clean(
    ids: &[String],
    records: &HashMap<String, CandidateRecord>,
    sizes: &HashMap<String, u64>,
    protected: &ProtectedPaths,
    remover: &dyn Remover,
) -> Vec<CleanItemResult> {
    let mut seen = HashSet::new();
    let selected: Vec<&String> = ids.iter().filter(|id| seen.insert(id.as_str())).collect();
    let selected_paths: Vec<&Path> = selected
        .iter()
        .filter_map(|id| records.get(*id).map(|r| r.path.as_path()))
        .collect();

    selected
        .into_iter()
        .map(|id| {
            let size_bytes = sizes.get(id).copied().unwrap_or(0);
            let Some(record) = records.get(id) else {
                return CleanItemResult {
                    id: id.clone(),
                    path: String::new(),
                    status: CleanStatus::Skipped,
                    message: "unknown item; run a new scan".into(),
                    size_bytes,
                };
            };
            let display = safety::display_path(&record.path);
            let skip = |message: String| CleanItemResult {
                id: id.clone(),
                path: display.clone(),
                status: CleanStatus::Skipped,
                message,
                size_bytes,
            };

            if selected_paths
                .iter()
                .any(|p| *p != record.path && record.path.starts_with(p))
            {
                return skip("contained in another selected item".into());
            }
            if let Err(reason) = revalidate(record, protected) {
                return skip(reason);
            }
            match remover.remove(&record.path) {
                Ok(()) => CleanItemResult {
                    id: id.clone(),
                    path: display,
                    status: CleanStatus::Trashed,
                    message: "moved to Trash".into(),
                    size_bytes,
                },
                Err(e) => CleanItemResult {
                    id: id.clone(),
                    path: display,
                    status: CleanStatus::Failed,
                    message: e,
                    size_bytes,
                },
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::{scan, ScanRequest};
    use std::cell::RefCell;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::AtomicBool;
    use tempfile::tempdir;

    /// Records what would have been removed, then removes it permanently
    /// inside the temp fixture (never the real Trash).
    struct FakeRemover(RefCell<Vec<PathBuf>>);
    impl Remover for FakeRemover {
        fn remove(&self, path: &Path) -> Result<(), String> {
            self.0.borrow_mut().push(path.to_path_buf());
            fs::remove_dir_all(path).map_err(|e| e.to_string())
        }
    }

    fn write(p: &Path) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, b"data").unwrap();
    }

    fn setup() -> (tempfile::TempDir, HashMap<String, CandidateRecord>, HashMap<String, u64>) {
        let t = tempdir().unwrap();
        write(&t.path().join("app/package.json"));
        write(&t.path().join("app/src/main.ts"));
        write(&t.path().join("app/node_modules/x/index.js"));
        let req = ScanRequest {
            roots: vec![t.path().to_string_lossy().into_owned()],
            exclusions: vec![],
        };
        let out = scan(&req, &ProtectedPaths::empty(), &AtomicBool::new(false), |_| {}).unwrap();
        let sizes = out
            .summary
            .candidates
            .iter()
            .map(|c| (c.id.clone(), c.size_bytes))
            .collect();
        (t, out.records.into_iter().collect(), sizes)
    }

    #[test]
    fn removes_validated_item_and_keeps_source() {
        let (t, records, sizes) = setup();
        let ids: Vec<String> = records.keys().cloned().collect();
        let fake = FakeRemover(RefCell::new(vec![]));
        let res = clean(&ids, &records, &sizes, &ProtectedPaths::empty(), &fake);
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].status, CleanStatus::Trashed);
        assert!(!t.path().join("app/node_modules").exists());
        assert!(t.path().join("app/src/main.ts").exists());
        assert!(t.path().join("app/package.json").exists());
    }

    #[test]
    fn unknown_ids_are_skipped() {
        let (_t, records, sizes) = setup();
        let fake = FakeRemover(RefCell::new(vec![]));
        let res = clean(&["nope".into()], &records, &sizes, &ProtectedPaths::empty(), &fake);
        assert_eq!(res[0].status, CleanStatus::Skipped);
        assert!(fake.0.borrow().is_empty());
    }

    #[test]
    fn duplicate_ids_processed_once() {
        let (_t, records, sizes) = setup();
        let id = records.keys().next().unwrap().clone();
        let fake = FakeRemover(RefCell::new(vec![]));
        let res = clean(&[id.clone(), id], &records, &sizes, &ProtectedPaths::empty(), &fake);
        assert_eq!(res.len(), 1);
        assert_eq!(fake.0.borrow().len(), 1);
    }

    #[test]
    fn replaced_folder_is_skipped() {
        let (t, records, sizes) = setup();
        let nm = t.path().join("app/node_modules");
        fs::remove_dir_all(&nm).unwrap();
        // Ensure a different creation timestamp on coarse-grained filesystems.
        std::thread::sleep(std::time::Duration::from_millis(20));
        write(&nm.join("other.js"));
        let ids: Vec<String> = records.keys().cloned().collect();
        let fake = FakeRemover(RefCell::new(vec![]));
        let res = clean(&ids, &records, &sizes, &ProtectedPaths::empty(), &fake);
        assert_eq!(res[0].status, CleanStatus::Skipped, "{}", res[0].message);
        assert!(nm.join("other.js").exists());
    }

    #[test]
    fn lost_project_context_is_skipped() {
        let (t, records, sizes) = setup();
        fs::remove_file(t.path().join("app/package.json")).unwrap();
        let ids: Vec<String> = records.keys().cloned().collect();
        let fake = FakeRemover(RefCell::new(vec![]));
        let res = clean(&ids, &records, &sizes, &ProtectedPaths::empty(), &fake);
        assert_eq!(res[0].status, CleanStatus::Skipped);
        assert!(t.path().join("app/node_modules/x/index.js").exists());
    }

    #[test]
    fn missing_item_is_skipped() {
        let (t, records, sizes) = setup();
        fs::remove_dir_all(t.path().join("app/node_modules")).unwrap();
        let ids: Vec<String> = records.keys().cloned().collect();
        let fake = FakeRemover(RefCell::new(vec![]));
        let res = clean(&ids, &records, &sizes, &ProtectedPaths::empty(), &fake);
        assert_eq!(res[0].status, CleanStatus::Skipped);
        assert!(fake.0.borrow().is_empty());
    }
}
