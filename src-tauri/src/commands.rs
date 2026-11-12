//! Tauri commands exposed to the frontend.
//!
//! Two independent flows with different permissions:
//!
//! * **Workspace** (`start_scan`, `clean_items`, ...): roots must pass
//!   `scanner::validate_root`, which refuses drive roots, the home folder and
//!   system locations. Cleanup accepts only session-scoped ids from the
//!   current scan.
//! * **Disk Analyzer** (`analyze_volume`, ...): may start at a drive root, but
//!   only one returned by `list_volumes`. It is read-only and its results use
//!   numeric node/file indices that the cleanup commands never accept.

use crate::ai_data::{self, DataItem};
use crate::cleaner::{self, CleanItemResult, CleanPolicy, TrashRemover};
use crate::disk_usage::{self, AnalysisProgress, AnalysisSummary, DirView, FileInfo, RevealTarget, VolumeInfo};
use crate::exclusions::{self, ExclusionPolicy, StoredSettings};
use crate::installed_apps::{self, Browser, CacheSize, Inventory};
use crate::rules::{self, RuleInfo};
use crate::safety::{self, ProtectedPaths};
use crate::scanner::{self, ScanRequest, ScanSummary};
use crate::snapshots::{self, Comparison, Snapshot, SnapshotMeta, SnapshotStore};
use crate::workspace::Workspace;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use tauri::{AppHandle, Emitter, Manager, State};

pub const SCAN_PROGRESS_EVENT: &str = "scan://progress";
pub const DISK_PROGRESS_EVENT: &str = "disk://progress";

#[tauri::command]
pub async fn inspect_ai_data() -> Result<Vec<DataItem>, String> {
    tauri::async_runtime::spawn_blocking(ai_data::local_data)
        .await
        .map_err(|e| format!("AI data inspection failed: {e}"))
}

#[tauri::command]
pub async fn inspect_ai_project(path: String) -> Result<Vec<DataItem>, String> {
    tauri::async_runtime::spawn_blocking(move || ai_data::project_data(&path))
        .await
        .map_err(|e| format!("Project inspection failed: {e}"))?
}

#[tauri::command]
pub async fn list_installed_apps() -> Result<Inventory, String> {
    tauri::async_runtime::spawn_blocking(installed_apps::list)
        .await
        .map_err(|e| format!("Application inventory task failed: {e}"))?
}

#[tauri::command]
pub async fn measure_app_cache(browser: Browser) -> Result<Vec<CacheSize>, String> {
    tauri::async_runtime::spawn_blocking(move || installed_apps::measure_cache(browser))
        .await
        .map_err(|e| format!("Cache measurement task failed: {e}"))?
}

pub struct AppState {
    workspace: Mutex<Workspace>,
    scan_cancel: Arc<AtomicBool>,
    settings_file: PathBuf,

    analysis: Mutex<Option<Arc<disk_usage::Analysis>>>,
    analyzing: Arc<AtomicBool>,
    analysis_cancel: Arc<AtomicBool>,
    analysis_counter: AtomicU64,
    snapshots: SnapshotStore,
}

impl AppState {
    pub fn new(app: &AppHandle) -> Result<Self, String> {
        let config = app.path().app_config_dir().map_err(|e| e.to_string())?;
        let data = app.path().app_data_dir().map_err(|e| e.to_string())?;
        let settings_file = config.join("settings.json");
        let settings = exclusions::load_settings(&settings_file);
        // Unparseable entries are dropped rather than blocking startup.
        let policy = ExclusionPolicy::from_paths(&settings.exclusions).unwrap_or_else(|_| {
            let valid: Vec<String> = settings
                .exclusions
                .iter()
                .filter(|p| Path::new(p.trim()).is_absolute())
                .cloned()
                .collect();
            ExclusionPolicy::from_paths(&valid).unwrap_or_default()
        });
        Ok(AppState {
            workspace: Mutex::new(Workspace::new(policy)),
            scan_cancel: Arc::new(AtomicBool::new(false)),
            settings_file,
            analysis: Mutex::new(None),
            analyzing: Arc::new(AtomicBool::new(false)),
            analysis_cancel: Arc::new(AtomicBool::new(false)),
            analysis_counter: AtomicU64::new(0),
            snapshots: SnapshotStore::new(snapshots::store_dir(&data)),
        })
    }

    fn ws(&self) -> Result<MutexGuard<'_, Workspace>, String> {
        self.workspace.lock().map_err(|_| "internal state poisoned".to_string())
    }
}

// ---------------------------------------------------------------------------
// Workspace flow
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn list_rules() -> Vec<RuleInfo> {
    rules::all_rules()
}

/// Validate a folder picked by the user before adding it as a scan root.
/// Returns the normalized display path. Drive roots are refused here.
#[tauri::command]
pub fn validate_root(path: String) -> Result<String, String> {
    let protected = ProtectedPaths::system();
    scanner::validate_root(&path, &protected).map(|p| safety::display_path(&p))
}

#[tauri::command]
pub fn get_exclusions(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    Ok(state.ws()?.policy().paths())
}

/// Replace the exclusion list. Persisted by the backend; invalidates the
/// current scan results if they were produced under a different list.
#[tauri::command]
pub fn set_exclusions(state: State<'_, AppState>, paths: Vec<String>) -> Result<Vec<String>, String> {
    let policy = ExclusionPolicy::from_paths(&paths)?;
    let saved = policy.paths();
    let mut ws = state.ws()?;
    ws.set_policy(policy)?;
    exclusions::save_settings(
        &state.settings_file,
        &StoredSettings {
            exclusions: saved.clone(),
        },
    )?;
    Ok(saved)
}

#[tauri::command]
pub async fn start_scan(
    app: AppHandle,
    state: State<'_, AppState>,
    request: ScanRequest,
) -> Result<ScanSummary, String> {
    let ticket = state.ws()?.begin_scan()?;
    state.scan_cancel.store(false, Ordering::SeqCst);
    let cancel = state.scan_cancel.clone();
    let session_id = ticket.session_id.clone();
    let policy = ticket.policy.clone();

    let joined = tauri::async_runtime::spawn_blocking(move || {
        let protected = ProtectedPaths::system();
        scanner::scan(&session_id, &request, &policy, &protected, &cancel, |progress| {
            let _ = app.emit(SCAN_PROGRESS_EVENT, progress);
        })
    })
    .await;

    let mut ws = state.ws()?;
    match joined {
        Ok(Ok(mut output)) => {
            ws.finish_scan(ticket, &mut output);
            Ok(output.summary)
        }
        Ok(Err(e)) => {
            ws.abort();
            Err(e)
        }
        Err(e) => {
            ws.abort();
            Err(format!("scan task failed: {e}"))
        }
    }
}

#[tauri::command]
pub fn cancel_scan(state: State<'_, AppState>) {
    state.scan_cancel.store(true, Ordering::SeqCst);
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VolumeSpace {
    pub volume: String,
    pub free_before: u64,
    pub free_after: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanReport {
    pub session_id: String,
    pub results: Vec<CleanItemResult>,
    /// Free space measured by the OS before and after, per affected volume.
    /// Empty where the platform cannot measure it.
    pub free_space: Vec<VolumeSpace>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanRequest {
    pub session_id: String,
    pub ids: Vec<String>,
    /// Caution items the user ticked "I have inspected this" for, one by one.
    #[serde(default)]
    pub acknowledged_caution: Vec<String>,
}

/// Move the selected candidates of the current session to the Trash.
#[tauri::command]
pub async fn clean_items(state: State<'_, AppState>, request: CleanRequest) -> Result<CleanReport, String> {
    let job = state
        .ws()?
        .begin_clean(&request.session_id, request.ids, request.acknowledged_caution)?;
    let job = Arc::new(job);
    let worker_job = job.clone();

    let joined = tauri::async_runtime::spawn_blocking(move || {
        let job = worker_job;
        let volumes: BTreeSet<PathBuf> = job
            .records
            .values()
            .filter_map(|r| disk_usage::volume_root_of(&r.path))
            .collect();
        let before: Vec<(PathBuf, Option<u64>)> = volumes
            .iter()
            .map(|v| (v.clone(), disk_usage::volume_space(v).map(|s| s.1)))
            .collect();

        let protected = ProtectedPaths::system();
        let policy = CleanPolicy {
            exclusions: &job.policy,
            protected: &protected,
            acknowledged: &job.acknowledged,
        };
        let results = cleaner::clean(&job.ids, &job.records, &job.sizes, &policy, &TrashRemover);

        let free_space = before
            .into_iter()
            .filter_map(|(v, b)| {
                let after = disk_usage::volume_space(&v)?.1;
                Some(VolumeSpace {
                    volume: safety::display_path(&v),
                    free_before: b?,
                    free_after: after,
                })
            })
            .collect();
        (results, free_space)
    })
    .await;

    let mut ws = state.ws()?;
    match joined {
        Ok((results, free_space)) => {
            ws.finish_clean(&job, &results);
            Ok(CleanReport {
                session_id: job.session_id.clone(),
                results,
                free_space,
            })
        }
        Err(e) => {
            ws.abort();
            Err(format!("cleanup task failed: {e}"))
        }
    }
}

// ---------------------------------------------------------------------------
// Disk Analyzer flow (read-only)
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn list_volumes() -> Vec<VolumeInfo> {
    disk_usage::list_volumes()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalyzeResult {
    pub summary: AnalysisSummary,
    pub snapshot: Option<SnapshotMeta>,
    pub snapshot_error: Option<String>,
}

/// Clears the `analyzing` flag even if the task panics.
struct FlagGuard(Arc<AtomicBool>);
impl Drop for FlagGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

#[tauri::command]
pub async fn analyze_volume(app: AppHandle, state: State<'_, AppState>, root: String) -> Result<AnalyzeResult, String> {
    // Permission check specific to this command: only an enumerated local
    // volume root is accepted — never an arbitrary path from the UI.
    let volume = disk_usage::list_volumes()
        .into_iter()
        .find(|v| v.root.eq_ignore_ascii_case(root.trim()))
        .ok_or("not a local volume")?;

    if state
        .analyzing
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err("an analysis is already running".into());
    }
    let guard = FlagGuard(state.analyzing.clone());
    state.analysis_cancel.store(false, Ordering::SeqCst);
    // Drop the previous tree first to bound memory use.
    *state.analysis.lock().map_err(|_| "internal state poisoned")? = None;

    let n = state.analysis_counter.fetch_add(1, Ordering::SeqCst) + 1;
    let id = format!("a{}-{n}", disk_usage::now_ms());
    let cancel = state.analysis_cancel.clone();
    let root_path = PathBuf::from(&volume.root);

    let analysis = tauri::async_runtime::spawn_blocking(move || {
        let _guard = guard;
        disk_usage::analyze(&id, &root_path, &cancel, |p: &AnalysisProgress| {
            let _ = app.emit(DISK_PROGRESS_EVENT, p);
        })
    })
    .await
    .map_err(|e| format!("analysis task failed: {e}"))?;

    let summary = analysis.summary();
    let (mut snapshot, mut snapshot_error) = (None, None);
    if !analysis.cancelled {
        match disk_usage::volume_space(Path::new(&volume.root)) {
            Some((total, free)) => {
                if let Some(snap) = Snapshot::from_analysis(&analysis, &volume, free, total) {
                    match state.snapshots.save(&snap) {
                        Ok(()) => snapshot = Some(snap.meta()),
                        Err(e) => snapshot_error = Some(e),
                    }
                }
            }
            None => snapshot_error = Some("could not read free space for this volume".into()),
        }
    }
    *state.analysis.lock().map_err(|_| "internal state poisoned")? = Some(Arc::new(analysis));
    Ok(AnalyzeResult {
        summary,
        snapshot,
        snapshot_error,
    })
}

#[tauri::command]
pub fn cancel_analysis(state: State<'_, AppState>) {
    state.analysis_cancel.store(true, Ordering::SeqCst);
}

fn current_analysis(state: &AppState, analysis_id: &str) -> Result<Arc<disk_usage::Analysis>, String> {
    state
        .analysis
        .lock()
        .map_err(|_| "internal state poisoned".to_string())?
        .as_ref()
        .filter(|a| a.id == analysis_id)
        .cloned()
        .ok_or_else(|| "this analysis is no longer available; analyze again".to_string())
}

#[tauri::command]
pub fn analysis_dir(state: State<'_, AppState>, analysis_id: String, node_id: u32, limit: Option<usize>) -> Result<DirView, String> {
    current_analysis(&state, &analysis_id)?.dir_view(node_id, limit.unwrap_or(100))
}

#[tauri::command]
pub fn analysis_top_files(
    state: State<'_, AppState>,
    analysis_id: String,
    node_id: u32,
    limit: Option<usize>,
) -> Result<Vec<FileInfo>, String> {
    Ok(current_analysis(&state, &analysis_id)?.top_files(node_id, limit.unwrap_or(50)))
}

/// Open a folder, or select a file, in Explorer. The path is resolved from
/// the analysis tree by index and re-checked; the UI never sends a path.
#[tauri::command]
pub fn reveal_in_explorer(state: State<'_, AppState>, analysis_id: String, target: RevealTarget) -> Result<(), String> {
    let analysis = current_analysis(&state, &analysis_id)?;
    let (path, is_file) = analysis.resolve(&target)?;
    disk_usage::reveal(&path, is_file)
}

#[tauri::command]
pub fn list_snapshots(state: State<'_, AppState>) -> Vec<SnapshotMeta> {
    state.snapshots.list()
}

#[tauri::command]
pub fn delete_snapshot(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.snapshots.delete(&id)
}

#[tauri::command]
pub fn clear_snapshots(state: State<'_, AppState>) -> Result<usize, String> {
    state.snapshots.clear()
}

#[tauri::command]
pub fn compare_snapshots(
    state: State<'_, AppState>,
    first_id: String,
    second_id: String,
    limit: Option<usize>,
) -> Result<Comparison, String> {
    let a = state.snapshots.load(&first_id)?;
    let b = state.snapshots.load(&second_id)?;
    snapshots::compare(&a, &b, limit.unwrap_or(50))
}
