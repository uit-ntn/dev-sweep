use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledApp {
    name: String,
    version: String,
    publisher: String,
    location: String,
    source: String,
    registration: String,
}

#[derive(Deserialize, Serialize)]
pub struct Inventory {
    apps: Vec<InstalledApp>,
    warnings: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Browser {
    Chrome,
    Edge,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheSize {
    path: String,
    bytes: u64,
    unreadable: u64,
}

#[cfg(windows)]
pub fn list() -> Result<Inventory, String> {
    use std::os::windows::process::CommandExt;
    use std::path::PathBuf;

    let powershell = std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .ok_or("Windows system directory is unavailable")?
        .join(r"System32\WindowsPowerShell\v1.0\powershell.exe");
    let output = std::process::Command::new(powershell)
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            include_str!("installed_apps.ps1"),
        ])
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
        .output()
        .map_err(|e| format!("Could not start Windows PowerShell: {e}"))?;
    if !output.status.success() {
        return Err("Windows could not read the installed application inventory.".into());
    }
    let mut inventory: Inventory = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("Could not read the application inventory: {e}"))?;
    inventory.apps.sort_by_cached_key(|app| {
        (
            app.name.to_lowercase(),
            app.publisher.to_lowercase(),
            app.version.clone(),
            app.location.to_lowercase(),
            app.registration.clone(),
        )
    });
    inventory.apps.dedup_by(|a, b| {
        a.name.eq_ignore_ascii_case(&b.name)
            && a.publisher.eq_ignore_ascii_case(&b.publisher)
            && a.version == b.version
            && a.location.eq_ignore_ascii_case(&b.location)
            && a.registration == b.registration
    });
    Ok(inventory)
}

#[cfg(windows)]
pub fn measure_cache(browser: Browser) -> Result<Vec<CacheSize>, String> {
    use crate::{safety, scanner};
    use std::fs;
    use std::io::ErrorKind;
    use std::path::PathBuf;
    use std::sync::atomic::AtomicBool;

    let mut root = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .ok_or("Local application data directory is unavailable")?
        .canonicalize()
        .map_err(|e| format!("Could not resolve local application data: {e}"))?;
    let components = match browser {
        Browser::Chrome => ["Google", "Chrome", "User Data"],
        Browser::Edge => ["Microsoft", "Edge", "User Data"],
    };
    for part in components {
        root.push(part);
        match fs::symlink_metadata(&root) {
            Ok(meta) if meta.is_dir() && !safety::is_link_like(&meta) => {}
            Ok(_) => return Err("The browser data folder is redirected or is not a directory.".into()),
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(format!("Could not inspect the browser data folder: {e}")),
        }
    }

    let cancel = AtomicBool::new(false);
    let mut caches = Vec::new();
    for profile in fs::read_dir(&root).map_err(|e| format!("Could not list browser profiles: {e}"))? {
        let profile = profile.map_err(|e| format!("Could not read a browser profile: {e}"))?;
        let name = profile.file_name();
        let name = name.to_string_lossy();
        if name != "Default"
            && !name.strip_prefix("Profile ").is_some_and(|n| {
                !n.is_empty() && n.bytes().all(|c| c.is_ascii_digit())
            })
        {
            continue;
        }
        let meta = profile.metadata().map_err(|e| format!("Could not inspect a browser profile: {e}"))?;
        if !meta.is_dir() || safety::is_link_like(&meta) {
            continue;
        }
        for name in ["Cache", "Code Cache", "GPUCache"] {
            let path = profile.path().join(name);
            match fs::symlink_metadata(&path) {
                Ok(meta) if meta.is_dir() && !safety::is_link_like(&meta) => {
                    let size = scanner::measure_dir(&path, &cancel);
                    caches.push(CacheSize {
                        path: safety::display_path(&path),
                        bytes: size.bytes,
                        unreadable: size.unreadable,
                    });
                }
                Ok(_) => {}
                Err(e) if e.kind() == ErrorKind::NotFound => {}
                Err(_) => caches.push(CacheSize {
                    path: safety::display_path(&path),
                    bytes: 0,
                    unreadable: 1,
                }),
            }
        }
    }
    caches.sort_by(|a, b| b.bytes.cmp(&a.bytes).then(a.path.cmp(&b.path)));
    Ok(caches)
}

#[cfg(not(windows))]
pub fn list() -> Result<Inventory, String> {
    Err("Installed application inventory is available on Windows.".into())
}

#[cfg(not(windows))]
pub fn measure_cache(_: Browser) -> Result<Vec<CacheSize>, String> {
    Err("Browser cache inspection is available on Windows.".into())
}
