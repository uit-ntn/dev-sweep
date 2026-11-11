//! Read-only disk usage analysis ("Disk Analyzer").
//!
//! Unlike the workspace scanner this module *may* start at a drive root, but
//! it never produces anything the cleanup commands accept: results are
//! addressed by plain node / file indices into an in-memory tree, which have
//! no relation to session-scoped cleanup ids.
//!
//! Traversal rules:
//! * runs with the current user's permissions — unreadable folders are
//!   counted and skipped, never escalated;
//! * symlinks, junctions and other reparse points are never followed (this
//!   also stops at volumes mounted into folders on Windows);
//! * on Unix, mount boundaries are not crossed;
//! * sizes are logical file lengths, aggregated bottom-up after a single
//!   traversal; hard-linked files are counted once per link.

use crate::safety;
use serde::Serialize;
use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

pub const NO_PARENT: u32 = u32::MAX;
const PROGRESS_INTERVAL: Duration = Duration::from_millis(200);
/// Largest files kept for the "largest files" list.
const TOP_FILES: usize = 1000;
/// Hard cap on folders kept in memory (~100 bytes each).
const MAX_NODES: usize = 5_000_000;

// ---------------------------------------------------------------------------
// Volumes
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VolumeInfo {
    /// Root path, e.g. `C:\`. Also the id used by `analyze_volume`.
    pub root: String,
    pub label: String,
    pub file_system: String,
    /// Volume serial number (Windows); used to match snapshots to volumes.
    pub serial: u32,
    pub total_bytes: u64,
    pub free_bytes: u64,
    pub removable: bool,
}

/// Total and free bytes of the volume holding `path`, as reported by the OS.
pub fn volume_space(path: &Path) -> Option<(u64, u64)> {
    #[cfg(windows)]
    {
        win::disk_free_space(path)
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        None
    }
}

/// Local fixed (and removable) drives. Network, optical and RAM drives are
/// not listed. Returns an empty list on non-Windows platforms.
pub fn list_volumes() -> Vec<VolumeInfo> {
    #[cfg(windows)]
    {
        win::list_volumes()
    }
    #[cfg(not(windows))]
    {
        Vec::new()
    }
}

/// Volume root (e.g. `C:\`) of an absolute path, for free-space lookups.
pub fn volume_root_of(path: &Path) -> Option<PathBuf> {
    let display = PathBuf::from(safety::display_path(path));
    display.ancestors().last().map(Path::to_path_buf)
}

#[cfg(windows)]
mod win {
    use super::VolumeInfo;
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;
    use windows_sys::Win32::Storage::FileSystem::{
        GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW,
    };

    const DRIVE_REMOVABLE: u32 = 2;
    const DRIVE_FIXED: u32 = 3;

    fn wide(s: &std::ffi::OsStr) -> Vec<u16> {
        s.encode_wide().chain(std::iter::once(0)).collect()
    }

    pub fn disk_free_space(path: &Path) -> Option<(u64, u64)> {
        let w = wide(path.as_os_str());
        let mut avail = 0u64;
        let mut total = 0u64;
        let mut free = 0u64;
        // SAFETY: valid NUL-terminated wide string and out pointers.
        let ok = unsafe { GetDiskFreeSpaceExW(w.as_ptr(), &mut avail, &mut total, &mut free) };
        (ok != 0).then_some((total, free))
    }

    pub fn list_volumes() -> Vec<VolumeInfo> {
        // SAFETY: no arguments.
        let mask = unsafe { GetLogicalDrives() };
        let mut out = Vec::new();
        for i in 0..26u32 {
            if mask & (1 << i) == 0 {
                continue;
            }
            let root = format!("{}:\\", (b'A' + i as u8) as char);
            let w = wide(std::ffi::OsStr::new(&root));
            // SAFETY: valid NUL-terminated wide string.
            let kind = unsafe { GetDriveTypeW(w.as_ptr()) };
            if kind != DRIVE_FIXED && kind != DRIVE_REMOVABLE {
                continue;
            }
            let mut label = [0u16; 261];
            let mut fs = [0u16; 261];
            let mut serial = 0u32;
            let mut max_comp = 0u32;
            let mut flags = 0u32;
            // SAFETY: buffers sized as passed; out pointers valid.
            let ok = unsafe {
                GetVolumeInformationW(
                    w.as_ptr(),
                    label.as_mut_ptr(),
                    label.len() as u32,
                    &mut serial,
                    &mut max_comp,
                    &mut flags,
                    fs.as_mut_ptr(),
                    fs.len() as u32,
                )
            };
            if ok == 0 {
                continue; // e.g. empty card reader
            }
            let Some((total, free)) = disk_free_space(Path::new(&root)) else {
                continue;
            };
            let to_string = |b: &[u16]| {
                let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
                String::from_utf16_lossy(&b[..end])
            };
            out.push(VolumeInfo {
                root,
                label: to_string(&label),
                file_system: to_string(&fs),
                serial,
                total_bytes: total,
                free_bytes: free,
                removable: kind == DRIVE_REMOVABLE,
            });
        }
        out
    }
}

// ---------------------------------------------------------------------------
// Analysis tree
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct Node {
    pub parent: u32,
    pub name: Box<str>,
    /// Aggregated logical size of everything below (after `aggregate`).
    pub bytes: u64,
    /// Aggregated file count.
    pub files: u64,
    /// Bytes of files directly in this folder.
    pub own_bytes: u64,
    pub children: Vec<u32>,
    /// This folder could not be listed.
    pub unreadable: bool,
    /// Some folder at or below this one could not be listed.
    pub incomplete: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct TopFile {
    bytes: u64,
    parent: u32,
    name: Box<str>,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisProgress {
    pub analysis_id: String,
    pub dirs_read: u64,
    pub dirs_unreadable: u64,
    pub files_seen: u64,
    pub bytes_seen: u64,
    pub current_path: String,
}

#[derive(Debug)]
pub struct Analysis {
    pub id: String,
    pub root: PathBuf,
    pub nodes: Vec<Node>,
    files: Vec<TopFile>,
    pub dirs_read: u64,
    pub dirs_unreadable: u64,
    pub links_skipped: u64,
    pub files_seen: u64,
    pub cancelled: bool,
    /// The folder cap was hit; deeper folders were not recorded.
    pub truncated: bool,
    pub started_at_ms: u64,
    pub finished_at_ms: u64,
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Walk `root` once, recording folder sizes and the largest files.
pub fn analyze<F: FnMut(&AnalysisProgress)>(
    id: &str,
    root: &Path,
    cancel: &AtomicBool,
    mut on_progress: F,
) -> Analysis {
    let started_at_ms = now_ms();
    let root_dev = std::fs::symlink_metadata(root)
        .ok()
        .and_then(|m| safety::device_id(&m));
    let mut nodes = vec![Node {
        parent: NO_PARENT,
        name: "".into(),
        bytes: 0,
        files: 0,
        own_bytes: 0,
        children: Vec::new(),
        unreadable: false,
        incomplete: false,
    }];
    let mut heap: BinaryHeap<Reverse<TopFile>> = BinaryHeap::new();
    let mut progress = AnalysisProgress {
        analysis_id: id.to_string(),
        ..Default::default()
    };
    let mut links_skipped = 0u64;
    let mut truncated = false;
    let mut last_emit = Instant::now();
    let mut stack: Vec<(u32, PathBuf)> = vec![(0, root.to_path_buf())];
    let mut cancelled = false;

    while let Some((idx, dir)) = stack.pop() {
        if cancel.load(Ordering::Relaxed) {
            cancelled = true;
            break;
        }
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => {
                nodes[idx as usize].unreadable = true;
                progress.dirs_unreadable += 1;
                continue;
            }
        };
        progress.dirs_read += 1;
        for entry in entries {
            let Ok(entry) = entry else {
                nodes[idx as usize].unreadable = true;
                continue;
            };
            // DirEntry::metadata does not follow links (and on Windows comes
            // from the directory listing, so locked files still report).
            let Ok(meta) = entry.metadata() else {
                nodes[idx as usize].unreadable = true;
                continue;
            };
            if safety::is_link_like(&meta) {
                links_skipped += 1;
                continue;
            }
            if meta.is_dir() {
                if root_dev.is_some() && safety::device_id(&meta) != root_dev {
                    links_skipped += 1;
                    continue;
                }
                if nodes.len() >= MAX_NODES {
                    truncated = true;
                    nodes[idx as usize].unreadable = true;
                    continue;
                }
                let child = nodes.len() as u32;
                nodes.push(Node {
                    parent: idx,
                    name: entry.file_name().to_string_lossy().into(),
                    bytes: 0,
                    files: 0,
                    own_bytes: 0,
                    children: Vec::new(),
                    unreadable: false,
                    incomplete: false,
                });
                nodes[idx as usize].children.push(child);
                stack.push((child, entry.path()));
            } else if meta.is_file() {
                let len = meta.len();
                let n = &mut nodes[idx as usize];
                n.own_bytes = n.own_bytes.saturating_add(len);
                n.files += 1;
                progress.files_seen += 1;
                progress.bytes_seen = progress.bytes_seen.saturating_add(len);
                let candidate = TopFile {
                    bytes: len,
                    parent: idx,
                    name: entry.file_name().to_string_lossy().into(),
                };
                if heap.len() < TOP_FILES {
                    heap.push(Reverse(candidate));
                } else if heap.peek().is_some_and(|Reverse(min)| len > min.bytes) {
                    heap.pop();
                    heap.push(Reverse(candidate));
                }
            }
        }
        if last_emit.elapsed() >= PROGRESS_INTERVAL {
            last_emit = Instant::now();
            progress.current_path = safety::display_path(&dir);
            on_progress(&progress);
        }
    }

    // Children always have a larger index than their parent, so one reverse
    // pass rolls sizes and the "incomplete" flag up to the root.
    for n in nodes.iter_mut() {
        n.bytes = n.own_bytes;
        n.incomplete = n.unreadable;
    }
    for i in (1..nodes.len()).rev() {
        let (bytes, files, incomplete, parent) = {
            let n = &nodes[i];
            (n.bytes, n.files, n.incomplete, n.parent as usize)
        };
        let p = &mut nodes[parent];
        p.bytes = p.bytes.saturating_add(bytes);
        p.files += files;
        p.incomplete |= incomplete;
    }
    if cancelled {
        nodes[0].incomplete = true;
    }

    progress.current_path.clear();
    on_progress(&progress);

    let mut files: Vec<TopFile> = heap.into_iter().map(|Reverse(f)| f).collect();
    files.sort_by(|a, b| b.bytes.cmp(&a.bytes));

    Analysis {
        id: id.to_string(),
        root: root.to_path_buf(),
        nodes,
        files,
        dirs_read: progress.dirs_read,
        dirs_unreadable: progress.dirs_unreadable,
        links_skipped,
        files_seen: progress.files_seen,
        cancelled,
        truncated,
        started_at_ms,
        finished_at_ms: now_ms(),
    }
}

// ---------------------------------------------------------------------------
// Queries (bounded results for IPC)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeInfo {
    pub id: u32,
    pub name: String,
    pub path: String,
    pub bytes: u64,
    pub files: u64,
    pub has_children: bool,
    pub unreadable: bool,
    pub incomplete: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Crumb {
    pub id: u32,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirView {
    pub analysis_id: String,
    pub node: NodeInfo,
    pub breadcrumb: Vec<Crumb>,
    /// Largest sub-folders, at most `limit`.
    pub children: Vec<NodeInfo>,
    pub more_children: u64,
    /// Bytes in sub-folders not listed.
    pub more_children_bytes: u64,
    /// Bytes of files directly in this folder.
    pub own_files_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileInfo {
    pub id: u32,
    pub name: String,
    pub path: String,
    pub folder_id: u32,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisSummary {
    pub analysis_id: String,
    pub root: String,
    pub total_bytes: u64,
    pub files: u64,
    pub dirs_read: u64,
    pub dirs_unreadable: u64,
    pub links_skipped: u64,
    pub cancelled: bool,
    pub truncated: bool,
    pub incomplete: bool,
    pub started_at_ms: u64,
    pub finished_at_ms: u64,
}

impl Analysis {
    pub fn summary(&self) -> AnalysisSummary {
        AnalysisSummary {
            analysis_id: self.id.clone(),
            root: safety::display_path(&self.root),
            total_bytes: self.nodes[0].bytes,
            files: self.files_seen,
            dirs_read: self.dirs_read,
            dirs_unreadable: self.dirs_unreadable,
            links_skipped: self.links_skipped,
            cancelled: self.cancelled,
            truncated: self.truncated,
            incomplete: self.nodes[0].incomplete || self.truncated,
            started_at_ms: self.started_at_ms,
            finished_at_ms: self.finished_at_ms,
        }
    }

    /// Path components from the root (exclusive) down to `id`.
    pub fn rel_components(&self, id: u32) -> Vec<&str> {
        let mut parts = Vec::new();
        let mut cur = id;
        while cur != 0 && cur != NO_PARENT {
            let n = &self.nodes[cur as usize];
            parts.push(&*n.name);
            cur = n.parent;
        }
        parts.reverse();
        parts
    }

    pub fn node_path(&self, id: u32) -> PathBuf {
        let mut p = self.root.clone();
        for c in self.rel_components(id) {
            p.push(c);
        }
        p
    }

    fn info(&self, id: u32) -> NodeInfo {
        let n = &self.nodes[id as usize];
        NodeInfo {
            id,
            name: if id == 0 {
                safety::display_path(&self.root)
            } else {
                n.name.to_string()
            },
            path: safety::display_path(&self.node_path(id)),
            bytes: n.bytes,
            files: n.files,
            has_children: !n.children.is_empty(),
            unreadable: n.unreadable,
            incomplete: n.incomplete,
        }
    }

    pub fn dir_view(&self, id: u32, limit: usize) -> Result<DirView, String> {
        let n = self.nodes.get(id as usize).ok_or("unknown folder")?;
        let mut kids: Vec<u32> = n.children.clone();
        kids.sort_by(|a, b| self.nodes[*b as usize].bytes.cmp(&self.nodes[*a as usize].bytes));
        let limit = limit.clamp(1, 500);
        let rest = kids.split_off(kids.len().min(limit));
        let mut breadcrumb = vec![Crumb {
            id: 0,
            name: safety::display_path(&self.root),
        }];
        let mut chain = Vec::new();
        let mut cur = id;
        while cur != 0 && cur != NO_PARENT {
            chain.push(cur);
            cur = self.nodes[cur as usize].parent;
        }
        for c in chain.into_iter().rev() {
            breadcrumb.push(Crumb {
                id: c,
                name: self.nodes[c as usize].name.to_string(),
            });
        }
        Ok(DirView {
            analysis_id: self.id.clone(),
            node: self.info(id),
            breadcrumb,
            children: kids.iter().map(|k| self.info(*k)).collect(),
            more_children: rest.len() as u64,
            more_children_bytes: rest.iter().map(|k| self.nodes[*k as usize].bytes).sum(),
            own_files_bytes: n.own_bytes,
        })
    }

    fn is_under(&self, mut node: u32, ancestor: u32) -> bool {
        loop {
            if node == ancestor {
                return true;
            }
            if node == 0 || node == NO_PARENT {
                return false;
            }
            node = self.nodes[node as usize].parent;
        }
    }

    /// Largest files below `folder` (from the volume-wide top list).
    pub fn top_files(&self, folder: u32, limit: usize) -> Vec<FileInfo> {
        self.files
            .iter()
            .enumerate()
            .filter(|(_, f)| self.is_under(f.parent, folder))
            .take(limit.clamp(1, 500))
            .map(|(i, f)| FileInfo {
                id: i as u32,
                name: f.name.to_string(),
                path: safety::display_path(&self.node_path(f.parent).join(&*f.name)),
                folder_id: f.parent,
                bytes: f.bytes,
            })
            .collect()
    }

    /// Resolve a folder or file index to a path that still exists and is not
    /// a link. Used for "Open in Explorer"; the UI never supplies a path.
    pub fn resolve(&self, target: &RevealTarget) -> Result<(PathBuf, bool), String> {
        let (path, is_file) = match target {
            RevealTarget::Folder { id } => {
                if *id as usize >= self.nodes.len() {
                    return Err("unknown folder".into());
                }
                (self.node_path(*id), false)
            }
            RevealTarget::File { id } => {
                let f = self.files.get(*id as usize).ok_or("unknown file")?;
                (self.node_path(f.parent).join(&*f.name), true)
            }
        };
        let meta = std::fs::symlink_metadata(&path).map_err(|_| "no longer exists".to_string())?;
        if safety::is_link_like(&meta) {
            return Err("path is now a link or junction".into());
        }
        if is_file != meta.is_file() {
            return Err("path changed type since the analysis".into());
        }
        Ok((path, is_file))
    }
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RevealTarget {
    Folder { id: u32 },
    File { id: u32 },
}

/// Show a validated path in the system file manager. No shell is involved.
pub fn reveal(path: &Path, is_file: bool) -> Result<(), String> {
    let display = safety::display_path(path);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        if display.contains('"') {
            return Err("unsupported path".into());
        }
        let mut cmd = std::process::Command::new("explorer.exe");
        if is_file {
            cmd.raw_arg(format!("/select,\"{display}\""));
        } else {
            cmd.raw_arg(format!("\"{display}\""));
        }
        cmd.spawn().map(|_| ()).map_err(|e| e.to_string())
    }
    #[cfg(target_os = "macos")]
    {
        let mut cmd = std::process::Command::new("open");
        if is_file {
            cmd.arg("-R");
        }
        cmd.arg(&display).spawn().map(|_| ()).map_err(|e| e.to_string())
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let target = if is_file {
            path.parent().map(safety::display_path).unwrap_or(display)
        } else {
            display
        };
        std::process::Command::new("xdg-open")
            .arg(target)
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn write(p: &Path, bytes: usize) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, vec![b'x'; bytes]).unwrap();
    }

    fn fixture() -> tempfile::TempDir {
        let t = tempdir().unwrap();
        write(&t.path().join("big/a.bin"), 1000);
        write(&t.path().join("big/deep/b.bin"), 500);
        write(&t.path().join("small/c.txt"), 10);
        write(&t.path().join("root.txt"), 5);
        t
    }

    fn run(root: &Path) -> Analysis {
        analyze("a1", root, &AtomicBool::new(false), |_| {})
    }

    #[test]
    fn aggregates_sizes_bottom_up() {
        let t = fixture();
        let a = run(t.path());
        assert_eq!(a.nodes[0].bytes, 1515);
        assert_eq!(a.nodes[0].files, 4);
        let view = a.dir_view(0, 10).unwrap();
        assert_eq!(view.children[0].name, "big");
        assert_eq!(view.children[0].bytes, 1500);
        assert_eq!(view.children[1].bytes, 10);
        assert_eq!(view.own_files_bytes, 5);
        assert!(!a.summary().incomplete);

        let big = view.children[0].id;
        let v2 = a.dir_view(big, 10).unwrap();
        assert_eq!(v2.breadcrumb.len(), 2);
        assert_eq!(v2.breadcrumb[1].name, "big");
    }

    #[test]
    fn largest_files_and_subtree_filter() {
        let t = fixture();
        let a = run(t.path());
        let top = a.top_files(0, 2);
        assert_eq!(top[0].bytes, 1000);
        assert_eq!(top[1].bytes, 500);
        let small = a.dir_view(0, 10).unwrap().children[1].id;
        let in_small = a.top_files(small, 10);
        assert_eq!(in_small.len(), 1);
        assert_eq!(in_small[0].name, "c.txt");
    }

    #[test]
    fn view_limits_children_for_ipc() {
        let t = tempdir().unwrap();
        for i in 0..30 {
            write(&t.path().join(format!("d{i}/f")), i + 1);
        }
        let a = run(t.path());
        let v = a.dir_view(0, 10).unwrap();
        assert_eq!(v.children.len(), 10);
        assert_eq!(v.more_children, 20);
        let listed: u64 = v.children.iter().map(|c| c.bytes).sum();
        assert_eq!(listed + v.more_children_bytes, a.nodes[0].bytes);
    }

    #[test]
    fn cancellation_stops_and_marks_incomplete() {
        let t = fixture();
        let a = analyze("a1", t.path(), &AtomicBool::new(true), |_| {});
        assert!(a.cancelled);
        assert!(a.summary().incomplete);
    }

    #[test]
    fn resolve_rejects_unknown_ids() {
        let t = fixture();
        let a = run(t.path());
        assert!(a.resolve(&RevealTarget::Folder { id: 9999 }).is_err());
        assert!(a.resolve(&RevealTarget::File { id: 9999 }).is_err());
        let (p, is_file) = a.resolve(&RevealTarget::File { id: 0 }).unwrap();
        assert!(is_file);
        assert!(p.ends_with("a.bin"));
    }

    #[cfg(unix)]
    #[test]
    fn continues_past_unreadable_folder() {
        use std::os::unix::fs::PermissionsExt;
        let t = fixture();
        let locked = t.path().join("locked");
        write(&locked.join("secret"), 100);
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
        let a = run(t.path());
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
        // Running as root ignores permissions; only assert when it applied.
        if a.dirs_unreadable > 0 {
            assert!(a.summary().incomplete);
            assert_eq!(a.nodes[0].bytes, 1515, "rest of the tree is still counted");
        }
    }

    #[cfg(unix)]
    #[test]
    fn skips_symlinked_folders() {
        let t = fixture();
        let outside = tempdir().unwrap();
        write(&outside.path().join("huge"), 10_000);
        std::os::unix::fs::symlink(outside.path(), t.path().join("link")).unwrap();
        let a = run(t.path());
        assert_eq!(a.nodes[0].bytes, 1515);
        assert_eq!(a.links_skipped, 1);
    }

    #[cfg(windows)]
    #[test]
    fn skips_junctions_and_symlinks() {
        let t = fixture();
        let outside = tempdir().unwrap();
        write(&outside.path().join("huge"), 10_000);
        // Junctions need no special privilege; `mklink /J` ships with Windows.
        let status = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(t.path().join("junction"))
            .arg(outside.path())
            .status();
        if !matches!(status, Ok(s) if s.success()) {
            return;
        }
        let a = run(t.path());
        assert_eq!(a.nodes[0].bytes, 1515, "junction target must not be counted");
        assert!(a.links_skipped >= 1);
    }

    #[cfg(windows)]
    #[test]
    fn continues_past_unreadable_folder() {
        // Deny listing to Everyone with icacls; skip if the tool is unavailable.
        let t = fixture();
        let locked = t.path().join("locked");
        write(&locked.join("secret"), 100);
        let deny = std::process::Command::new("icacls")
            .arg(&locked)
            .args(["/deny", "*S-1-1-0:(RD)"])
            .output();
        if !matches!(deny, Ok(ref o) if o.status.success()) {
            return;
        }
        let a = run(t.path());
        let _ = std::process::Command::new("icacls")
            .arg(&locked)
            .args(["/remove:d", "*S-1-1-0"])
            .output();
        assert!(a.dirs_unreadable >= 1);
        assert!(a.summary().incomplete);
        assert_eq!(a.nodes[0].bytes, 1515);
    }
}
