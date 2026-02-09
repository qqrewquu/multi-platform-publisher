use crate::browser::chrome;
use crate::database::Database;
use crate::database::queries;
use crate::platforms;
use tauri::State;

#[tauri::command]
pub fn get_accounts(db: State<'_, Database>) -> Result<Vec<queries::Account>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    queries::get_all_accounts(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn add_account(db: State<'_, Database>, platform: String, display_name: String) -> Result<queries::Account, String> {
    // Validate platform
    let platform_info = platforms::get_platform_info(&platform)
        .ok_or_else(|| format!("Unknown platform: {}", platform))?;

    // Create Chrome profile directory
    let index = chrome::next_profile_index(&platform).map_err(|e| e.to_string())?;
    let profile_dir = chrome::create_profile_dir(&platform, index).map_err(|e| e.to_string())?;
    let profile_dir_str = profile_dir.to_string_lossy().to_string();

    // Insert into database
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    let name = if display_name.is_empty() {
        format!("{} 账号 {}", platform_info.name, index)
    } else {
        display_name
    };
    let id = queries::insert_account(&conn, &platform, &name, &profile_dir_str)
        .map_err(|e| e.to_string())?;

    // Return the created account
    Ok(queries::Account {
        id,
        platform,
        display_name: name,
        avatar_url: None,
        chrome_profile_dir: profile_dir_str,
        is_logged_in: false,
        last_checked_at: None,
        created_at: chrono::Utc::now().to_rfc3339(),
    })
}

#[tauri::command]
pub fn delete_account(db: State<'_, Database>, account_id: i64) -> Result<(), String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    let profile_dir = queries::delete_account(&conn, account_id).map_err(|e| e.to_string())?;

    // Clean up Chrome profile directory
    let profile_path = std::path::PathBuf::from(&profile_dir);
    if let Err(e) = chrome::delete_profile(&profile_path) {
        log::warn!("Failed to delete Chrome profile {}: {}", profile_dir, e);
    }

    Ok(())
}

#[tauri::command]
pub fn update_account_name(db: State<'_, Database>, account_id: i64, display_name: String) -> Result<(), String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    queries::update_account_display_name(&conn, account_id, &display_name).map_err(|e| e.to_string())
}

/// Launch Chrome for the user to log in to a platform
#[tauri::command]
pub fn open_login(db: State<'_, Database>, account_id: i64) -> Result<(), String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;

    // Get account info
    let accounts = queries::get_all_accounts(&conn).map_err(|e| e.to_string())?;
    let account = accounts.iter()
        .find(|a| a.id == account_id)
        .ok_or_else(|| format!("Account {} not found", account_id))?;

    // Get platform info
    let platform_info = platforms::get_platform_info(&account.platform)
        .ok_or_else(|| format!("Unknown platform: {}", account.platform))?;

    // Detect Chrome
    let chrome_path = chrome::detect_chrome().map_err(|e| e.to_string())?;

    // Launch Chrome for login
    let profile_dir = std::path::PathBuf::from(&account.chrome_profile_dir);
    chrome::launch_chrome_for_login(&chrome_path, &profile_dir, &platform_info.login_url)
        .map_err(|e| e.to_string())?;

    Ok(())
}

/// Open a platform's creator page in Chrome (quick access)
#[tauri::command]
pub fn open_platform(db: State<'_, Database>, account_id: i64) -> Result<(), String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;

    let accounts = queries::get_all_accounts(&conn).map_err(|e| e.to_string())?;
    let account = accounts.iter()
        .find(|a| a.id == account_id)
        .ok_or_else(|| format!("Account {} not found", account_id))?;

    let platform_info = platforms::get_platform_info(&account.platform)
        .ok_or_else(|| format!("Unknown platform: {}", account.platform))?;

    let chrome_path = chrome::detect_chrome().map_err(|e| e.to_string())?;
    let profile_dir = std::path::PathBuf::from(&account.chrome_profile_dir);

    chrome::launch_chrome_with_debug(&chrome_path, &profile_dir, &platform_info.upload_url)
        .map_err(|e| e.to_string())?;

    Ok(())
}

/// Update login status for an account (check if cookies are still valid)
#[tauri::command]
pub fn update_login_status(db: State<'_, Database>, account_id: i64, is_logged_in: bool) -> Result<(), String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    queries::update_account_login_status(&conn, account_id, is_logged_in)
        .map_err(|e| e.to_string())
}
