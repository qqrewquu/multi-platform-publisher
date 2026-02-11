mod browser;
mod commands;
mod database;
mod platforms;

use database::Database;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::init();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // Initialize database in app data directory
            let app_data_dir = app
                .path()
                .app_data_dir()
                .expect("Failed to get app data dir");
            let db = Database::new(&app_data_dir).expect("Failed to initialize database");
            app.manage(db);

            log::info!(
                "MultiPublisher initialized. DB at: {}",
                app_data_dir.display()
            );
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Chrome
            commands::chrome::detect_chrome,
            commands::chrome::get_platforms,
            // Accounts
            commands::accounts::get_accounts,
            commands::accounts::add_account,
            commands::accounts::delete_account,
            commands::accounts::update_account_name,
            commands::accounts::open_login,
            commands::accounts::open_platform,
            commands::accounts::update_login_status,
            // Publish
            commands::publish::create_publish_task,
            commands::publish::get_publish_tasks,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
