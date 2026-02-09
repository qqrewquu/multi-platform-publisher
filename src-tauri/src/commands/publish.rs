use crate::browser::chrome;
use crate::database::Database;
use crate::database::queries;
use crate::platforms;
use serde::{Deserialize, Serialize};
use tauri::State;

#[derive(Debug, Deserialize)]
pub struct PublishRequest {
    pub video_path: String,
    pub title: String,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub is_original: bool,
    pub manual_confirm: bool,
    pub account_ids: Vec<i64>,
}

#[derive(Debug, Serialize)]
pub struct PublishResult {
    pub task_id: i64,
    pub platform_tasks: Vec<PlatformTaskResult>,
}

#[derive(Debug, Serialize)]
pub struct PlatformTaskResult {
    pub account_id: i64,
    pub platform: String,
    pub status: String,
    pub message: Option<String>,
}

/// Create a publish task and launch Chrome for each platform
#[tauri::command]
pub fn create_publish_task(
    db: State<'_, Database>,
    request: PublishRequest,
) -> Result<PublishResult, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;

    // Validate video file exists
    if !std::path::Path::new(&request.video_path).exists() {
        return Err(format!("Video file not found: {}", request.video_path));
    }

    // Create the main task
    let tags_json = serde_json::to_string(&request.tags).unwrap_or_default();
    let task_id = queries::insert_publish_task(
        &conn,
        &request.video_path,
        &request.title,
        request.description.as_deref(),
        Some(&tags_json),
        request.is_original,
        None,
    ).map_err(|e| e.to_string())?;

    // Create platform tasks and launch Chrome for each
    let accounts = queries::get_all_accounts(&conn).map_err(|e| e.to_string())?;
    let chrome_path = chrome::detect_chrome().map_err(|e| e.to_string())?;

    let mut platform_tasks = Vec::new();

    for account_id in &request.account_ids {
        let account = accounts.iter()
            .find(|a| a.id == *account_id)
            .ok_or_else(|| format!("Account {} not found", account_id))?;

        // Insert platform task record
        let _platform_task_id = queries::insert_task_platform(&conn, task_id, *account_id)
            .map_err(|e| e.to_string())?;

        // Get platform info
        let platform_info = platforms::get_platform_info(&account.platform)
            .ok_or_else(|| format!("Unknown platform: {}", account.platform))?;

        // Launch Chrome for this platform
        let profile_dir = std::path::PathBuf::from(&account.chrome_profile_dir);
        let visible = request.manual_confirm; // Show Chrome if manual confirm is on

        match chrome::launch_chrome_for_publish(
            &chrome_path,
            &profile_dir,
            &platform_info.upload_url,
            visible,
        ) {
            Ok(_child) => {
                platform_tasks.push(PlatformTaskResult {
                    account_id: *account_id,
                    platform: account.platform.clone(),
                    status: "launched".into(),
                    message: Some(format!("Chrome launched for {}", platform_info.name)),
                });
            }
            Err(e) => {
                platform_tasks.push(PlatformTaskResult {
                    account_id: *account_id,
                    platform: account.platform.clone(),
                    status: "failed".into(),
                    message: Some(e.to_string()),
                });
            }
        }
    }

    // Update task status
    let all_launched = platform_tasks.iter().all(|t| t.status == "launched");
    let new_status = if all_launched { "publishing" } else { "partial" };
    queries::update_task_status(&conn, task_id, new_status).map_err(|e| e.to_string())?;

    Ok(PublishResult {
        task_id,
        platform_tasks,
    })
}

/// Get all publish tasks with their platform statuses
#[tauri::command]
pub fn get_publish_tasks(db: State<'_, Database>) -> Result<Vec<queries::PublishTask>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    queries::get_all_tasks(&conn).map_err(|e| e.to_string())
}
