//! DevSweep core: rule matching, scanning and Trash-first cleanup.

pub mod cleaner;
pub mod commands;
pub mod rules;
pub mod safety;
pub mod scanner;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(commands::AppState::default())
        .invoke_handler(tauri::generate_handler![
            commands::list_rules,
            commands::validate_root,
            commands::start_scan,
            commands::cancel_scan,
            commands::clean_items,
        ])
        .run(tauri::generate_context!())
        .expect("error while running DevSweep");
}
