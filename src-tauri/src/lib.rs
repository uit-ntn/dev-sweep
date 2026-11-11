//! DevSweep core: rule matching, scanning, Trash-first cleanup, and a
//! read-only disk analyzer with local snapshots.

pub mod cleaner;
pub mod commands;
pub mod disk_usage;
pub mod exclusions;
pub mod rules;
pub mod safety;
pub mod scanner;
pub mod snapshots;
pub mod workspace;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let state = commands::AppState::new(app.handle())?;
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_rules,
            commands::validate_root,
            commands::get_exclusions,
            commands::set_exclusions,
            commands::start_scan,
            commands::cancel_scan,
            commands::clean_items,
            commands::list_volumes,
            commands::analyze_volume,
            commands::cancel_analysis,
            commands::analysis_dir,
            commands::analysis_top_files,
            commands::reveal_in_explorer,
            commands::list_snapshots,
            commands::delete_snapshot,
            commands::clear_snapshots,
            commands::compare_snapshots,
        ])
        .run(tauri::generate_context!())
        .expect("error while running DevSweep");
}
