//! Tauri commands exposed to the frontend.

use crate::cleaner::{self, CleanItemResult, TrashRemover};
use crate::rules::{self, RuleInfo};
use crate::safety::{self, ProtectedPaths};
use crate::scanner::{self, CandidateRecord, ScanRequest, ScanSummary};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, State};

pub const PROGRESS_EVENT: &str = "scan://progress";

#[derive(Default)]
pub struct ScanState {
    /// Records from the most recent completed scan, keyed by candidate id.
    records: HashMap<String, CandidateRecord>,
    sizes: HashMap<String, u64>,
}

pub struct AppState {
    scan: Mutex<ScanState>,
    cancel: Arc<AtomicBool>,
    running: Arc<AtomicBool>,
}

impl Default for AppState {
    fn default() -> Self {
        AppState {
            scan: Mutex::new(ScanState::default()),
            cancel: Arc::new(AtomicBool::new(false)),
            running: Arc::new(AtomicBool::new(false)),
        }
    }
}

/// Clears the `running` flag even if the scan thread panics.
struct RunningGuard(Arc<AtomicBool>);
impl Drop for RunningGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

#[tauri::command]
pub fn list_rules() -> Vec<RuleInfo> {
    rules::all_rules()
}

/// Validate a folder picked by the user before adding it as a scan root.
/// Returns the normalized display path.
#[tauri::command]
pub fn validate_root(path: String) -> Result<String, String> {
    let protected = ProtectedPaths::system();
    scanner::validate_root(&path, &protected).map(|p| safety::display_path(&p))
}

#[tauri::command]
pub async fn start_scan(
    app: AppHandle,
    state: State<'_, AppState>,
    request: ScanRequest,
) -> Result<ScanSummary, String> {
    if state
        .running
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err("a scan is already running".into());
    }
    let guard = RunningGuard(state.running.clone());
    state.cancel.store(false, Ordering::SeqCst);
    let cancel = state.cancel.clone();

    let output = tauri::async_runtime::spawn_blocking(move || {
        let _guard = guard;
        let protected = ProtectedPaths::system();
        scanner::scan(&request, &protected, &cancel, |progress| {
            let _ = app.emit(PROGRESS_EVENT, progress);
        })
    })
    .await
    .map_err(|e| format!("scan task failed: {e}"))??;

    let mut scan = state.scan.lock().map_err(|_| "state poisoned")?;
    scan.sizes = output
        .summary
        .candidates
        .iter()
        .map(|c| (c.id.clone(), c.size_bytes))
        .collect();
    scan.records = output.records.into_iter().collect();
    Ok(output.summary)
}

#[tauri::command]
pub fn cancel_scan(state: State<'_, AppState>) {
    state.cancel.store(true, Ordering::SeqCst);
}

/// Move the selected candidates (by id from the latest scan) to the Trash.
#[tauri::command]
pub async fn clean_items(
    state: State<'_, AppState>,
    ids: Vec<String>,
) -> Result<Vec<CleanItemResult>, String> {
    if state.running.load(Ordering::SeqCst) {
        return Err("wait for the running scan to finish before cleaning".into());
    }
    let (records, sizes) = {
        let scan = state.scan.lock().map_err(|_| "state poisoned")?;
        let records: HashMap<_, _> = ids
            .iter()
            .filter_map(|id| scan.records.get(id).map(|r| (id.clone(), r.clone())))
            .collect();
        (records, scan.sizes.clone())
    };

    let results = tauri::async_runtime::spawn_blocking(move || {
        let protected = ProtectedPaths::system();
        cleaner::clean(&ids, &records, &sizes, &protected, &TrashRemover)
    })
    .await
    .map_err(|e| format!("cleanup task failed: {e}"))?;

    // Forget items that were removed so they cannot be "cleaned" twice.
    let mut scan = state.scan.lock().map_err(|_| "state poisoned")?;
    for r in &results {
        if r.status == cleaner::CleanStatus::Trashed {
            scan.records.remove(&r.id);
        }
    }
    Ok(results)
}
