//! User exclusions, owned and persisted by the backend.
//!
//! The backend is the single source of truth: scans read the policy from
//! application state (never from the scan request), and cleanup re-checks
//! every item against the *current* policy, so a direct IPC call cannot
//! bypass an exclusion.
//!
//! A candidate conflicts with an exclusion when either path lies inside the
//! other. Moving `app/dist` to the Trash when `app/dist/keep` is excluded
//! would remove the excluded folder too, so that is refused.

use crate::safety;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
struct Exclusion {
    display: String,
    key: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExclusionPolicy {
    entries: Vec<Exclusion>,
}

impl ExclusionPolicy {
    /// Build a policy from user input. Paths must be absolute. Existing
    /// folders are canonicalized; folders that do not exist (yet) are kept
    /// as typed and compared lexically.
    pub fn from_paths(paths: &[String]) -> Result<Self, String> {
        let mut entries: Vec<Exclusion> = Vec::new();
        for raw in paths {
            let raw = raw.trim();
            if raw.is_empty() {
                continue;
            }
            let p = Path::new(raw);
            if !p.is_absolute() {
                return Err(format!("{raw}: exclusions must be absolute paths"));
            }
            let resolved: PathBuf = p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
            let key = safety::path_key(&resolved);
            if entries.iter().any(|e| e.key == key) {
                continue;
            }
            entries.push(Exclusion {
                display: safety::display_path(&resolved),
                key,
            });
        }
        Ok(ExclusionPolicy { entries })
    }

    pub fn paths(&self) -> Vec<String> {
        self.entries.iter().map(|e| e.display.clone()).collect()
    }

    /// Order-independent identity of the policy, used to detect changes
    /// between a scan and a later cleanup.
    pub fn fingerprint(&self) -> Vec<Vec<String>> {
        let mut keys: Vec<_> = self.entries.iter().map(|e| e.key.clone()).collect();
        keys.sort();
        keys
    }

    /// `p` is an excluded folder or lies inside one.
    pub fn excludes(&self, p: &Path) -> bool {
        let key = safety::path_key(p);
        self.entries.iter().any(|e| safety::key_within(&key, &e.key))
    }

    /// Why `p` must not be removed as a whole, if it overlaps an exclusion in
    /// either direction.
    pub fn conflict(&self, p: &Path) -> Option<String> {
        let key = safety::path_key(p);
        for e in &self.entries {
            if safety::key_within(&key, &e.key) {
                return Some(format!("inside excluded folder {}", e.display));
            }
            if safety::key_within(&e.key, &key) {
                return Some(format!("contains excluded folder {}", e.display));
            }
        }
        None
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredSettings {
    #[serde(default)]
    pub exclusions: Vec<String>,
}

pub fn load_settings(file: &Path) -> StoredSettings {
    std::fs::read(file)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

pub fn save_settings(file: &Path, settings: &StoredSettings) -> Result<(), String> {
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_vec_pretty(settings).map_err(|e| e.to_string())?;
    // Write-then-rename so a crash never leaves a truncated settings file.
    let tmp = file.with_extension("json.tmp");
    std::fs::write(&tmp, json).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, file).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn conflict_in_both_directions() {
        let t = tempdir().unwrap();
        let keep = t.path().join("app/dist/keep");
        std::fs::create_dir_all(&keep).unwrap();
        let policy = ExclusionPolicy::from_paths(&[keep.to_string_lossy().into_owned()]).unwrap();
        let dist = t.path().join("app/dist").canonicalize().unwrap();
        assert!(policy.conflict(&dist).unwrap().starts_with("contains"));
        assert!(!policy.excludes(&dist));
        let inner = keep.join("x");
        assert!(policy.conflict(&inner).unwrap().starts_with("inside"));
        assert!(policy.excludes(&inner));
        let sibling = t.path().join("app/src");
        assert!(policy.conflict(&sibling).is_none());
    }

    #[test]
    fn missing_folders_are_compared_lexically() {
        let t = tempdir().unwrap();
        let ghost = t.path().join("later/dist");
        let policy = ExclusionPolicy::from_paths(&[ghost.to_string_lossy().into_owned()]).unwrap();
        assert!(policy.excludes(&ghost.join("a")));
    }

    #[test]
    fn rejects_relative_and_dedupes() {
        assert!(ExclusionPolicy::from_paths(&["rel/path".into()]).is_err());
        let t = tempdir().unwrap();
        let p = t.path().to_string_lossy().into_owned();
        let policy = ExclusionPolicy::from_paths(&[p.clone(), p]).unwrap();
        assert_eq!(policy.paths().len(), 1);
    }

    #[test]
    fn fingerprint_is_order_independent() {
        let t = tempdir().unwrap();
        let one = t.path().join("one").to_string_lossy().into_owned();
        let two = t.path().join("two").to_string_lossy().into_owned();
        let a = ExclusionPolicy::from_paths(&[one.clone(), two.clone()]).unwrap();
        let b = ExclusionPolicy::from_paths(&[two, one]).unwrap();
        assert_eq!(a.fingerprint(), b.fingerprint());
    }

    #[test]
    fn settings_roundtrip() {
        let t = tempdir().unwrap();
        let f = t.path().join("cfg/settings.json");
        assert!(load_settings(&f).exclusions.is_empty());
        save_settings(&f, &StoredSettings { exclusions: vec!["/a".into()] }).unwrap();
        assert_eq!(load_settings(&f).exclusions, vec!["/a".to_string()]);
    }
}
