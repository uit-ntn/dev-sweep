//! Filesystem scanner.
//!
//! The scanner is a plain synchronous function so it can be unit-tested
//! without Tauri; the command layer runs it on a blocking thread and forwards
//! progress callbacks as events.
//!
//! Invariants:
//! * only user-selected roots are visited, and protected locations are refused;
//! * symlinks, junctions and other reparse points are never followed;
//! * on Unix, mount boundaries are not crossed;
//! * a matched directory is not descended into, so matches never nest and
//!   bytes are never double counted.

use crate::rules::{self, Category, Risk, Rule};
use crate::safety::{self, DirIdentity, ProtectedPaths};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanRequest {
    pub roots: Vec<String>,
    #[serde(default)]
    pub exclusions: Vec<String>,
}

/// A match as sent to the UI.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub id: String,
    pub path: String,
    pub name: String,
    pub project_dir: String,
    pub root: String,
    pub rule_id: &'static str,
    pub display_name: &'static str,
    pub ecosystem: &'static str,
    pub category: Category,
    pub risk: Risk,
    pub size_bytes: u64,
    pub file_count: u64,
    /// Entries that could not be read while sizing (size is then a lower bound).
    pub unreadable: u64,
    /// Latest modification time found in the tree, seconds since the epoch.
    pub last_modified: Option<u64>,
}

/// Backend-only record kept for revalidation before cleanup.
#[derive(Debug, Clone)]
pub struct CandidateRecord {
    pub path: PathBuf,
    pub root: PathBuf,
    pub rule: &'static Rule,
    pub identity: DirIdentity,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ScanProgress {
    pub dirs_visited: u64,
    pub candidates_found: u64,
    pub bytes_found: u64,
    pub errors: u64,
    pub current_path: String,
    /// Candidates discovered since the previous progress message.
    pub new_candidates: Vec<Candidate>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanSummary {
    pub roots: Vec<String>,
    pub candidates: Vec<Candidate>,
    pub dirs_visited: u64,
    pub errors: u64,
    pub total_bytes: u64,
    pub cancelled: bool,
    pub elapsed_ms: u64,
}

pub struct ScanOutput {
    pub summary: ScanSummary,
    pub records: Vec<(String, CandidateRecord)>,
}

const PROGRESS_INTERVAL: Duration = Duration::from_millis(150);

/// Canonicalize and validate a user-supplied scan root.
pub fn validate_root(raw: &str, protected: &ProtectedPaths) -> Result<PathBuf, String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err("empty path".into());
    }
    let input = Path::new(raw);
    if !input.is_absolute() {
        return Err("path must be absolute".into());
    }
    let meta = std::fs::symlink_metadata(input).map_err(|e| format!("cannot access path: {e}"))?;
    if safety::is_link_like(&meta) {
        return Err("scan roots cannot be symbolic links or junctions".into());
    }
    if !meta.is_dir() {
        return Err("not a directory".into());
    }
    let canonical = input
        .canonicalize()
        .map_err(|e| format!("cannot resolve path: {e}"))?;
    protected.check(&canonical)?;
    Ok(canonical)
}

/// Drop roots that are nested inside other selected roots.
fn dedupe_roots(mut roots: Vec<PathBuf>) -> Vec<PathBuf> {
    roots.sort_by_key(|p| p.components().count());
    let mut kept: Vec<PathBuf> = Vec::new();
    for r in roots {
        if !kept.iter().any(|k| r.starts_with(k)) {
            kept.push(r);
        }
    }
    kept
}

struct Scanner<'a, F: FnMut(ScanProgress)> {
    cancel: &'a AtomicBool,
    protected: &'a ProtectedPaths,
    exclusions: Vec<PathBuf>,
    on_progress: F,
    last_emit: Instant,
    progress: ScanProgress,
    candidates: Vec<Candidate>,
    records: Vec<(String, CandidateRecord)>,
    next_id: u64,
}

impl<'a, F: FnMut(ScanProgress)> Scanner<'a, F> {
    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    fn maybe_emit(&mut self, force: bool) {
        if force || self.last_emit.elapsed() >= PROGRESS_INTERVAL {
            self.last_emit = Instant::now();
            let msg = ScanProgress {
                new_candidates: std::mem::take(&mut self.progress.new_candidates),
                ..self.progress.clone()
            };
            (self.on_progress)(msg);
        }
    }

    fn is_excluded(&self, p: &Path) -> bool {
        self.exclusions.iter().any(|e| p.starts_with(e))
    }

    fn scan_root(&mut self, root: &Path) {
        let root_dev = std::fs::symlink_metadata(root)
            .ok()
            .and_then(|m| safety::device_id(&m));
        let mut stack: Vec<PathBuf> = vec![root.to_path_buf()];

        while let Some(dir) = stack.pop() {
            if self.cancelled() {
                return;
            }
            self.progress.dirs_visited += 1;
            self.progress.current_path = safety::display_path(&dir);
            self.maybe_emit(false);

            let entries = match std::fs::read_dir(&dir) {
                Ok(e) => e,
                Err(_) => {
                    self.progress.errors += 1;
                    continue;
                }
            };

            for entry in entries {
                let Ok(entry) = entry else {
                    self.progress.errors += 1;
                    continue;
                };
                let path = entry.path();
                // symlink_metadata: never follow links.
                let Ok(meta) = std::fs::symlink_metadata(&path) else {
                    self.progress.errors += 1;
                    continue;
                };
                if safety::is_link_like(&meta) || !meta.is_dir() {
                    continue;
                }
                if root_dev.is_some() && safety::device_id(&meta) != root_dev {
                    continue; // different mount
                }
                if self.is_excluded(&path) || self.protected.check(&path).is_err() {
                    continue;
                }

                if let Some(rule) = rules::match_dir(&path) {
                    self.add_candidate(root, &path, rule, &meta);
                    if self.cancelled() {
                        return;
                    }
                    continue; // never descend into a match
                }

                let name = entry.file_name();
                if rules::NEVER_DESCEND
                    .iter()
                    .any(|n| name.to_str() == Some(*n))
                {
                    continue;
                }
                stack.push(path);
            }
        }
    }

    fn add_candidate(&mut self, root: &Path, path: &Path, rule: &'static Rule, meta: &std::fs::Metadata) {
        let size = measure_dir(path, self.cancel);
        self.next_id += 1;
        let id = format!("c{}", self.next_id);
        let candidate = Candidate {
            id: id.clone(),
            path: safety::display_path(path),
            name: path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            project_dir: path.parent().map(safety::display_path).unwrap_or_default(),
            root: safety::display_path(root),
            rule_id: rule.id,
            display_name: rule.display_name,
            ecosystem: rule.ecosystem,
            category: rule.category,
            risk: rule.risk,
            size_bytes: size.bytes,
            file_count: size.files,
            unreadable: size.unreadable,
            last_modified: size.latest_mtime,
        };
        self.records.push((
            id,
            CandidateRecord {
                path: path.to_path_buf(),
                root: root.to_path_buf(),
                rule,
                identity: safety::dir_identity(path, meta),
            },
        ));
        self.progress.candidates_found += 1;
        self.progress.bytes_found += size.bytes;
        self.progress.new_candidates.push(candidate.clone());
        self.candidates.push(candidate);
        self.maybe_emit(false);
    }
}

#[derive(Debug, Default)]
pub struct DirSize {
    pub bytes: u64,
    pub files: u64,
    pub unreadable: u64,
    pub latest_mtime: Option<u64>,
}

fn mtime_secs(meta: &std::fs::Metadata) -> Option<u64> {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
}

/// Sum the apparent size of all regular files under `dir` without following
/// links. Links themselves are counted as zero bytes.
pub fn measure_dir(dir: &Path, cancel: &AtomicBool) -> DirSize {
    let mut out = DirSize::default();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let Ok(entries) = std::fs::read_dir(&d) else {
            out.unreadable += 1;
            continue;
        };
        for entry in entries {
            let Ok(entry) = entry else {
                out.unreadable += 1;
                continue;
            };
            let Ok(meta) = std::fs::symlink_metadata(entry.path()) else {
                out.unreadable += 1;
                continue;
            };
            if let Some(m) = mtime_secs(&meta) {
                out.latest_mtime = Some(out.latest_mtime.map_or(m, |cur| cur.max(m)));
            }
            if safety::is_link_like(&meta) {
                continue;
            }
            if meta.is_dir() {
                stack.push(entry.path());
            } else if meta.is_file() {
                out.files += 1;
                out.bytes = out.bytes.saturating_add(meta.len());
            }
        }
    }
    out
}

/// Run a scan. `on_progress` is called at most every [`PROGRESS_INTERVAL`]
/// plus once at the end.
pub fn scan<F: FnMut(ScanProgress)>(
    request: &ScanRequest,
    protected: &ProtectedPaths,
    cancel: &AtomicBool,
    on_progress: F,
) -> Result<ScanOutput, String> {
    let started = Instant::now();
    if request.roots.is_empty() {
        return Err("select at least one folder to scan".into());
    }
    let roots = request
        .roots
        .iter()
        .map(|r| validate_root(r, protected).map_err(|e| format!("{r}: {e}")))
        .collect::<Result<Vec<_>, _>>()?;
    let roots = dedupe_roots(roots);

    // Exclusions that cannot be resolved (e.g. deleted folders) are ignored.
    let exclusions = request
        .exclusions
        .iter()
        .filter_map(|e| Path::new(e.trim()).canonicalize().ok())
        .collect();

    let mut scanner = Scanner {
        cancel,
        protected,
        exclusions,
        on_progress,
        last_emit: Instant::now(),
        progress: ScanProgress::default(),
        candidates: Vec::new(),
        records: Vec::new(),
        next_id: 0,
    };

    for root in &roots {
        if scanner.cancelled() {
            break;
        }
        if scanner.is_excluded(root) {
            continue;
        }
        scanner.scan_root(root);
    }
    scanner.progress.current_path.clear();
    scanner.maybe_emit(true);

    let cancelled = scanner.cancelled();
    let mut candidates = scanner.candidates;
    candidates.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes));
    let total_bytes = candidates.iter().map(|c| c.size_bytes).sum();

    Ok(ScanOutput {
        summary: ScanSummary {
            roots: roots.iter().map(|r| safety::display_path(r)).collect(),
            candidates,
            dirs_visited: scanner.progress.dirs_visited,
            errors: scanner.progress.errors,
            total_bytes,
            cancelled,
            elapsed_ms: started.elapsed().as_millis() as u64,
        },
        records: scanner.records,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn write(p: &Path, bytes: usize) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, vec![b'a'; bytes]).unwrap();
    }

    fn run(root: &Path, exclusions: Vec<String>) -> ScanOutput {
        let req = ScanRequest {
            roots: vec![root.to_string_lossy().into_owned()],
            exclusions,
        };
        scan(&req, &ProtectedPaths::empty(), &AtomicBool::new(false), |_| {}).unwrap()
    }

    fn fixture() -> tempfile::TempDir {
        let t = tempdir().unwrap();
        let web = t.path().join("web");
        write(&web.join("package.json"), 10);
        write(&web.join("src/index.ts"), 10);
        write(&web.join("node_modules/a/index.js"), 100);
        write(&web.join("node_modules/b/sub/x.js"), 50);
        // Nested node_modules inside a match must not be reported separately.
        write(&web.join("node_modules/b/node_modules/c.js"), 5);

        let py = t.path().join("py");
        write(&py.join("main.py"), 10);
        write(&py.join(".venv/pyvenv.cfg"), 20);
        write(&py.join("pkg/__pycache__/m.cpython-312.pyc"), 30);
        write(&py.join("analysis.ipynb"), 40);
        t
    }

    #[test]
    fn finds_expected_candidates_with_sizes() {
        let t = fixture();
        let out = run(t.path(), vec![]);
        let mut ids: Vec<_> = out.summary.candidates.iter().map(|c| c.rule_id).collect();
        ids.sort();
        assert_eq!(ids, vec!["node.dependencies", "python.bytecode", "python.venv"]);

        let nm = out
            .summary
            .candidates
            .iter()
            .find(|c| c.rule_id == "node.dependencies")
            .unwrap();
        assert_eq!(nm.size_bytes, 155);
        assert_eq!(nm.file_count, 3);
        assert_eq!(out.summary.total_bytes, 155 + 20 + 30);
        assert_eq!(out.records.len(), 3);
        assert!(!out.summary.cancelled);
    }

    #[test]
    fn never_reports_notebooks_or_source() {
        let t = fixture();
        let out = run(t.path(), vec![]);
        for c in &out.summary.candidates {
            assert!(!c.path.ends_with(".ipynb"));
            assert!(!c.path.contains("src"));
        }
    }

    #[test]
    fn exclusions_are_respected() {
        let t = fixture();
        let excl = t.path().join("py").to_string_lossy().into_owned();
        let out = run(t.path(), vec![excl]);
        assert!(out.summary.candidates.iter().all(|c| c.ecosystem != "python"));
    }

    #[test]
    fn cancellation_stops_scan() {
        let t = fixture();
        let req = ScanRequest {
            roots: vec![t.path().to_string_lossy().into_owned()],
            exclusions: vec![],
        };
        let cancel = AtomicBool::new(true);
        let out = scan(&req, &ProtectedPaths::empty(), &cancel, |_| {}).unwrap();
        assert!(out.summary.cancelled);
        assert!(out.summary.candidates.is_empty());
    }

    #[test]
    fn nested_roots_are_deduplicated() {
        let t = fixture();
        let req = ScanRequest {
            roots: vec![
                t.path().to_string_lossy().into_owned(),
                t.path().join("web").to_string_lossy().into_owned(),
            ],
            exclusions: vec![],
        };
        let out = scan(&req, &ProtectedPaths::empty(), &AtomicBool::new(false), |_| {}).unwrap();
        assert_eq!(out.summary.roots.len(), 1);
        assert_eq!(out.summary.candidates.len(), 3);
    }

    #[test]
    fn rejects_relative_and_missing_roots() {
        let p = ProtectedPaths::empty();
        assert!(validate_root("relative/path", &p).is_err());
        assert!(validate_root("", &p).is_err());
        let t = tempdir().unwrap();
        assert!(validate_root(&t.path().join("missing").to_string_lossy(), &p).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn does_not_follow_symlinks() {
        let t = fixture();
        let outside = tempdir().unwrap();
        write(&outside.path().join("package.json"), 1);
        write(&outside.path().join("node_modules/x.js"), 1000);
        std::os::unix::fs::symlink(outside.path(), t.path().join("linked")).unwrap();
        let out = run(t.path(), vec![]);
        assert!(out.summary.candidates.iter().all(|c| !c.path.contains("linked")));
    }

    #[cfg(windows)]
    #[test]
    fn does_not_follow_dir_symlinks() {
        let t = fixture();
        let outside = tempdir().unwrap();
        write(&outside.path().join("package.json"), 1);
        write(&outside.path().join("node_modules/x.js"), 1000);
        // Creating symlinks needs Developer Mode or admin rights; skip if unavailable.
        if std::os::windows::fs::symlink_dir(outside.path(), t.path().join("linked")).is_err() {
            return;
        }
        let out = run(t.path(), vec![]);
        assert!(out.summary.candidates.iter().all(|c| !c.path.contains("linked")));
    }
}
