//! Read-only size estimates for known AI tool data and project markers.
use crate::{safety, scanner};
use serde::Serialize;
use std::fs;
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::AtomicBool;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DataItem {
    tool: &'static str,
    kind: &'static str,
    path: String,
    bytes: u64,
    incomplete: bool,
}

type Target = (&'static str, &'static str, &'static str);

fn measure(root: &Path, targets: &[Target]) -> Vec<DataItem> {
    let cancel = AtomicBool::new(false);
    let mut items = Vec::new();
    'target: for &(tool, kind, relative) in targets {
        let path = root.join(relative);
        let mut current = root.to_path_buf();
        let mut components = Path::new(relative).components().peekable();
        while let Some(Component::Normal(part)) = components.next() {
            current.push(part);
            let meta = match fs::symlink_metadata(&current) {
                Ok(meta) if !safety::is_link_like(&meta) => meta,
                Ok(_) => continue 'target,
                Err(e) if e.kind() == ErrorKind::NotFound => continue 'target,
                Err(_) => {
                    items.push(DataItem {
                        tool, kind, path: safety::display_path(&path), bytes: 0, incomplete: true,
                    });
                    continue 'target;
                }
            };
            if components.peek().is_some() {
                if !meta.is_dir() {
                    continue 'target;
                }
                continue;
            }
            let (bytes, incomplete) = if meta.is_dir() {
                let size = scanner::measure_dir(&current, &cancel);
                (size.bytes, size.unreadable > 0)
            } else if meta.is_file() {
                (meta.len(), false)
            } else {
                continue 'target;
            };
            items.push(DataItem { tool, kind, path: safety::display_path(&path), bytes, incomplete });
        }
    }
    items
}

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).map(PathBuf::from).filter(|path| path.is_absolute())
}

pub fn local_data() -> Vec<DataItem> {
    let home = env_path("USERPROFILE").or_else(|| env_path("HOME"));
    let claude = env_path("CLAUDE_CONFIG_DIR").or_else(|| Some(home.as_ref()?.join(".claude")));
    let codex = env_path("CODEX_HOME").or_else(|| Some(home.as_ref()?.join(".codex")));
    let roaming = env_path("APPDATA");
    let local = env_path("LOCALAPPDATA");
    let locations: [(Option<PathBuf>, &[Target]); 4] = [
        (claude, &[
            ("Claude Code", "Project and session data", "projects"),
            ("Claude Code", "Debug logs", "debug"),
        ]),
        (codex, &[
            ("Codex", "Sessions", "sessions"),
            ("Codex", "Archived sessions", "archived_sessions"),
            ("Codex", "Logs", "log"),
        ]),
        (roaming, &[
            ("Claude Desktop", "Cache", "Claude/Cache"),
            ("Claude Desktop", "Cache", "Claude/Code Cache"),
            ("Claude Desktop", "Cache", "Claude/GPUCache"),
            ("Claude Desktop", "Logs", "Claude/logs"),
            ("Cursor", "Cache", "Cursor/Cache"),
            ("Cursor", "Cache", "Cursor/Code Cache"),
            ("Cursor", "Cache", "Cursor/GPUCache"),
            ("Cursor", "Logs", "Cursor/logs"),
            ("Cursor", "Workspace state", "Cursor/User/workspaceStorage"),
        ]),
        (local, &[
            ("ChatGPT Desktop", "Package cache", "Packages/OpenAI.ChatGPT-Desktop_2p2nqsd0c76g0/LocalCache"),
            ("ChatGPT Desktop", "Package temporary state", "Packages/OpenAI.ChatGPT-Desktop_2p2nqsd0c76g0/TempState"),
            ("ChatGPT / Codex Desktop", "Package cache", "Packages/OpenAI.Codex_2p2nqsd0c76g0/LocalCache"),
            ("ChatGPT / Codex Desktop", "Package temporary state", "Packages/OpenAI.Codex_2p2nqsd0c76g0/TempState"),
        ]),
    ];
    let mut items = Vec::new();
    for (root, targets) in locations {
        let Some(root) = root else { continue };
        match fs::symlink_metadata(&root) {
            Ok(meta) if meta.is_dir() && !safety::is_link_like(&meta) => {
                items.extend(measure(&root, targets));
            }
            Ok(_) => {}
            Err(e) if e.kind() == ErrorKind::NotFound => {}
            Err(_) => {
                for &(tool, kind, relative) in targets {
                    items.push(DataItem {
                        tool, kind, path: safety::display_path(&root.join(relative)),
                        bytes: 0, incomplete: true,
                    });
                }
            }
        }
    }
    items.sort_by(|a, b| b.bytes.cmp(&a.bytes).then(a.path.cmp(&b.path)));
    items
}

pub fn project_data(path: &str) -> Result<Vec<DataItem>, String> {
    let root = scanner::validate_root(path, &safety::ProtectedPaths::system())?;
    Ok(measure(&root, &[
        ("Claude Code", "Project settings and instructions", ".claude"),
        ("Claude Code", "Project instructions", "CLAUDE.md"),
        ("Claude Code", "Local instructions", "CLAUDE.local.md"),
        ("Codex", "Project settings", ".codex"),
        ("Coding agents", "Project instructions", "AGENTS.md"),
        ("Cursor", "Project settings and rules", ".cursor"),
        ("Cursor", "Legacy project rules", ".cursorrules"),
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_project_markers_and_ignores_unrelated_files() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        fs::create_dir(root.join(".claude")).unwrap();
        fs::write(root.join(".claude").join("settings.json"), b"{}").unwrap();
        fs::write(root.join("AGENTS.md"), b"rules").unwrap();
        fs::write(root.join("unrelated.txt"), b"unrelated").unwrap();
        let items = project_data(root.to_str().unwrap()).unwrap();

        assert_eq!(items.len(), 2);
        assert_eq!(items[0].tool, "Claude Code");
        assert_eq!(items[0].bytes, 2);
        assert_eq!(items[1].tool, "Coding agents");
        assert_eq!(items[1].bytes, 5);
        assert!(items.iter().all(|item| !item.incomplete));
    }
}
