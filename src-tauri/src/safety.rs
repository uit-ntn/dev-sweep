//! Safety primitives shared by the scanner and the cleaner:
//! protected-location checks, link / reparse-point detection, and a cheap
//! directory identity used to notice when a path was replaced after scanning.

use serde::Serialize;
use std::fs::Metadata;
use std::path::{Path, PathBuf};

/// Windows `FILE_ATTRIBUTE_REPARSE_POINT` (symlinks, junctions, mount points,
/// cloud placeholders, ...).
#[cfg(windows)]
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;

/// True when the entry is a symlink, junction or any other reparse point.
/// `meta` must come from `symlink_metadata` (i.e. links not followed).
pub fn is_link_like(meta: &Metadata) -> bool {
    if meta.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return true;
        }
    }
    false
}

/// Identifier of the device a path lives on, used to avoid crossing mount
/// boundaries. On Windows mounted folders are reparse points and are already
/// excluded by [`is_link_like`], so this returns `None`.
pub fn device_id(meta: &Metadata) -> Option<u64> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Some(meta.dev())
    }
    #[cfg(not(unix))]
    {
        let _ = meta;
        None
    }
}

/// A best-effort fingerprint of a directory taken at scan time. If any field
/// differs at cleanup time, the path is treated as replaced and skipped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DirIdentity {
    pub a: u64,
    pub b: u64,
    /// Creation (birth) time in nanoseconds where the platform reports it.
    /// Guards against inode reuse on Unix after delete + recreate.
    pub created: u64,
}

fn created_nanos(meta: &Metadata) -> u64 {
    meta.created()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

pub fn dir_identity(path: &Path, meta: &Metadata) -> DirIdentity {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let _ = path;
        DirIdentity {
            a: meta.dev(),
            b: meta.ino(),
            created: created_nanos(meta),
        }
    }
    #[cfg(windows)]
    {
        // Prefer the NTFS volume serial + file index, which changes whenever
        // the folder is deleted and recreated. Fall back to the creation
        // timestamp if the handle cannot be opened.
        windows_file_id(path, meta).unwrap_or_else(|| DirIdentity {
            a: 0,
            b: u64::MAX,
            created: created_nanos(meta),
        })
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = path;
        DirIdentity {
            a: 0,
            b: 0,
            created: created_nanos(meta),
        }
    }
}

#[cfg(windows)]
fn windows_file_id(path: &Path, meta: &Metadata) -> Option<DirIdentity> {
    use std::os::windows::fs::OpenOptionsExt;
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION, FILE_FLAG_BACKUP_SEMANTICS,
        FILE_FLAG_OPEN_REPARSE_POINT,
    };

    let file = std::fs::OpenOptions::new()
        .access_mode(0)
        // BACKUP_SEMANTICS is required to open a directory; OPEN_REPARSE_POINT
        // makes sure we identify the link itself, never its target.
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .ok()?;
    // SAFETY: the handle is valid for the lifetime of `file`, and `info` is a
    // plain-old-data out parameter.
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    let ok = unsafe { GetFileInformationByHandle(file.as_raw_handle() as _, &mut info) };
    if ok == 0 {
        return None;
    }
    Some(DirIdentity {
        a: info.dwVolumeSerialNumber as u64,
        b: ((info.nFileIndexHigh as u64) << 32) | info.nFileIndexLow as u64,
        created: created_nanos(meta),
    })
}

/// Strip the Windows verbatim prefix (`\\?\`) for display purposes.
pub fn display_path(p: &Path) -> String {
    let s = p.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else if let Some(rest) = s.strip_prefix(r"\\?\") {
        rest.to_string()
    } else {
        s.into_owned()
    }
}

fn env_path(var: &str) -> Option<PathBuf> {
    std::env::var_os(var)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .and_then(|p| p.canonicalize().ok())
}

/// Locations that may never be used as a scan root and never be removed.
/// Paths are canonicalized so they compare with canonicalized inputs.
pub struct ProtectedPaths {
    /// Exact paths that are protected (descendants are allowed as scan roots).
    exact: Vec<PathBuf>,
    /// Paths whose whole subtree is protected.
    subtree: Vec<PathBuf>,
}

impl ProtectedPaths {
    pub fn system() -> Self {
        let mut exact = Vec::new();
        let mut subtree = Vec::new();

        if let Some(home) = env_path("USERPROFILE").or_else(|| env_path("HOME")) {
            exact.push(home);
        }

        #[cfg(windows)]
        {
            for var in ["SystemRoot", "windir", "ProgramFiles", "ProgramFiles(x86)", "ProgramData"] {
                if let Some(p) = env_path(var) {
                    subtree.push(p);
                }
            }
            for var in ["APPDATA", "LOCALAPPDATA"] {
                if let Some(p) = env_path(var) {
                    exact.push(p);
                }
            }
            if let Some(home) = env_path("USERPROFILE") {
                for sub in ["AppData", "Documents", "Desktop", "Downloads", "OneDrive"] {
                    if let Ok(p) = home.join(sub).canonicalize() {
                        exact.push(p);
                    }
                }
            }
        }

        #[cfg(unix)]
        {
            for p in [
                "/bin", "/boot", "/dev", "/etc", "/lib", "/lib64", "/proc", "/sbin", "/sys",
                "/usr", "/var", "/System", "/Library", "/Applications", "/private",
            ] {
                if let Ok(c) = Path::new(p).canonicalize() {
                    subtree.push(c);
                }
            }
            for p in ["/home", "/Users", "/opt", "/mnt", "/media", "/Volumes"] {
                if let Ok(c) = Path::new(p).canonicalize() {
                    exact.push(c);
                }
            }
        }

        ProtectedPaths { exact, subtree }
    }

    #[cfg(test)]
    pub fn empty() -> Self {
        ProtectedPaths {
            exact: Vec::new(),
            subtree: Vec::new(),
        }
    }

    /// `path` must already be canonical.
    pub fn check(&self, path: &Path) -> Result<(), String> {
        if is_filesystem_root(path) {
            return Err("filesystem or drive roots cannot be scanned or cleaned".into());
        }
        if self.exact.iter().any(|p| p == path) {
            return Err("this location is protected; choose a more specific folder".into());
        }
        if self.subtree.iter().any(|p| path.starts_with(p)) {
            return Err("system locations are protected".into());
        }
        Ok(())
    }
}

/// A path with no parent (`/`, `C:\`, `\\?\C:\`, `\\server\share`).
pub fn is_filesystem_root(path: &Path) -> bool {
    path.parent().is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roots_are_rejected() {
        let p = ProtectedPaths::empty();
        #[cfg(windows)]
        assert!(p.check(Path::new(r"C:\")).is_err());
        #[cfg(unix)]
        assert!(p.check(Path::new("/")).is_err());
    }

    #[test]
    fn home_is_rejected_but_children_allowed() {
        let p = ProtectedPaths::system();
        if let Some(home) = env_path("USERPROFILE").or_else(|| env_path("HOME")) {
            assert!(p.check(&home).is_err());
            let child = home.join("some-projects-folder");
            assert!(p.check(&child).is_ok());
        }
    }

    #[test]
    fn display_strips_verbatim_prefix() {
        assert_eq!(display_path(Path::new(r"\\?\C:\work\app")), r"C:\work\app");
        assert_eq!(display_path(Path::new(r"\\?\UNC\srv\share")), r"\\srv\share");
    }
}
