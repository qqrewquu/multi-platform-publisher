use crate::browser::{chrome, automation};
use crate::database::Database;
use crate::database::queries;
use crate::platforms;
use serde::{Deserialize, Serialize};
use tauri::State;
use log::info;

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

#[derive(Debug, Clone, Serialize)]
pub struct PublishResult {
    pub task_id: i64,
    pub platform_tasks: Vec<PlatformTaskResult>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlatformTaskResult {
    pub account_id: i64,
    pub platform: String,
    pub status: String,
    pub message: Option<String>,
}

/// Create a publish task and automate Chrome for each platform
#[tauri::command]
pub async fn create_publish_task(
    db: State<'_, Database>,
    request: PublishRequest,
) -> Result<PublishResult, String> {
    // Validate video file exists
    if !std::path::Path::new(&request.video_path).exists() {
        return Err(format!("Video file not found: {}", request.video_path));
    }

    // Create the main task in DB
    let (task_id, accounts_info) = {
        let conn = db.conn.lock().map_err(|e| e.to_string())?;

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

        let accounts = queries::get_all_accounts(&conn).map_err(|e| e.to_string())?;

        // Collect account info for publishing
        let mut accounts_info = Vec::new();
        for account_id in &request.account_ids {
            let account = accounts.iter()
                .find(|a| a.id == *account_id)
                .ok_or_else(|| format!("Account {} not found", account_id))?;

            queries::insert_task_platform(&conn, task_id, *account_id)
                .map_err(|e| e.to_string())?;

            accounts_info.push((
                account.id,
                account.platform.clone(),
                account.chrome_profile_dir.clone(),
            ));
        }

        (task_id, accounts_info)
    };

    // Detect Chrome
    let chrome_path = chrome::detect_chrome().map_err(|e| e.to_string())?;

    let mut platform_tasks = Vec::new();

    // Process each platform
    for (account_id, platform, profile_dir_str) in &accounts_info {
        let platform_info = platforms::get_platform_info(platform)
            .ok_or_else(|| format!("Unknown platform: {}", platform))?;

        let profile_dir = std::path::PathBuf::from(profile_dir_str);

        info!("Publishing to {} (account {})", platform_info.name, account_id);

        // Launch Chrome with debugging port
        let launch_result = chrome::launch_chrome_with_debug(
            &chrome_path,
            &profile_dir,
            &platform_info.upload_url,
        );

        match launch_result {
            Ok((_child, port)) => {
                // Wait for Chrome to be ready, then automate
                let automation_result = automate_platform(
                    port,
                    platform,
                    &request.video_path,
                    &request.title,
                    request.description.as_deref().unwrap_or(""),
                    &request.tags,
                ).await;

                match automation_result {
                    Ok(msg) => {
                        platform_tasks.push(PlatformTaskResult {
                            account_id: *account_id,
                            platform: platform.clone(),
                            status: "automated".into(),
                            message: Some(msg),
                        });
                    }
                    Err(e) => {
                        info!("Automation failed for {}: {}", platform_info.name, e);
                        platform_tasks.push(PlatformTaskResult {
                            account_id: *account_id,
                            platform: platform.clone(),
                            status: "launched".into(),
                            message: Some(format!(
                                "Chrome 已打开 {}，但自动填充失败：{}。请手动操作。",
                                platform_info.name, e
                            )),
                        });
                    }
                }
            }
            Err(e) => {
                platform_tasks.push(PlatformTaskResult {
                    account_id: *account_id,
                    platform: platform.clone(),
                    status: "failed".into(),
                    message: Some(e.to_string()),
                });
            }
        }
    }

    // Update task status
    {
        let conn = db.conn.lock().map_err(|e| e.to_string())?;
        let has_automated = platform_tasks.iter().any(|t| t.status == "automated");
        let new_status = if has_automated { "publishing" } else { "partial" };
        queries::update_task_status(&conn, task_id, new_status).map_err(|e| e.to_string())?;
    }

    Ok(PublishResult {
        task_id,
        platform_tasks,
    })
}

/// Run platform-specific automation via CDP
async fn automate_platform(
    port: u16,
    platform: &str,
    video_path: &str,
    title: &str,
    description: &str,
    tags: &[String],
) -> Result<String, String> {
    // Wait for Chrome to be ready
    info!("Waiting for Chrome on port {} to be ready...", port);
    chrome::wait_for_chrome_ready(port, 30).await
        .map_err(|e| format!("Chrome not ready: {}", e))?;

    // Give Chrome a moment to load the page
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    // Connect via CDP
    info!("Connecting to Chrome via CDP on port {}...", port);
    let (_browser, page) = automation::connect_to_chrome(port).await
        .map_err(|e| format!("CDP connection failed: {}", e))?;

    // Wait for page to load
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Run platform-specific automation
    match platform {
        "douyin" => {
            crate::platforms::douyin::auto_publish(&page, video_path, title, description, tags).await
                .map_err(|e| format!("Douyin automation error: {}", e))?;
            Ok(format!("抖音：视频已上传，表单已填写。请在 Chrome 中检查并点击发布。"))
        }
        _ => {
            // For other platforms, just report Chrome is open
            Ok(format!("Chrome 已打开到平台上传页面。请手动完成操作。"))
        }
    }
}

/// Get all publish tasks
#[tauri::command]
pub fn get_publish_tasks(db: State<'_, Database>) -> Result<Vec<queries::PublishTask>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    queries::get_all_tasks(&conn).map_err(|e| e.to_string())
}
