use crate::browser::{automation, chrome};
use crate::database::queries;
use crate::database::Database;
use crate::platforms;
use log::info;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Instant;
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
    pub error_code: Option<String>,
    pub action_hint: Option<String>,
    pub debug_port_used: Option<u16>,
    pub session_mode: Option<String>,
    pub automation_phase: Option<String>,
}

const ACTION_HINT_CLOSE_WINDOW: &str = "请先关闭该账号已打开的 Chrome 窗口后重试。";
const ACTION_HINT_CHECK_CHROME: &str = "请确认 Chrome 已成功打开并停留在目标平台页面后重试。";
const ACTION_HINT_AUTOMATION_TIMEOUT: &str = "上传可能已开始，请在 Chrome 页面继续并重试提交。";
const ACTION_HINT_TARGET_PAGE_NOT_FOUND: &str =
    "未定位到目标平台上传页，已尝试新开窗口。请在 Chrome 打开对应平台上传页后重试。";
const ACTION_HINT_TARGET_PAGE_NOT_READY: &str = "页面未完成加载，请等待页面稳定后重试。";
const ACTION_HINT_LOGIN_REQUIRED: &str = "请先在 Chrome 完成微信扫码登录，再重试上传。";
const ACTION_HINT_WECHAT_CHOOSER_NOT_OPENED: &str =
    "微信上传入口暂不可交互，已多轮重试仍未触发文件选择器。请稍等页面稳定后重试。";
const ACTION_HINT_WECHAT_UPLOAD_SIGNAL_TIMEOUT: &str =
    "微信已完成文件注入，但未观测到上传信号。请在 Chrome 页面确认是否已开始上传。";
const AUTOMATION_TIMEOUT_SECS: u64 = 45;

#[derive(Debug, Clone)]
struct PlatformAutomationError {
    code: String,
    message: String,
    action_hint: Option<String>,
    debug_port_used: Option<u16>,
}

impl PlatformAutomationError {
    fn from_raw(raw: &str) -> Self {
        let (code, action_hint) = classify_error(raw);
        Self {
            code: code.to_string(),
            message: strip_error_code_prefix(raw),
            action_hint,
            debug_port_used: None,
        }
    }

    fn with_debug_port(mut self, port: u16) -> Self {
        self.debug_port_used = Some(port);
        self
    }
}

#[derive(Debug, Clone)]
struct AutomationSuccess {
    message: String,
    debug_port_used: u16,
    automation_phase: &'static str,
}

/// Create a publish task and automate Chrome for each platform
#[tauri::command]
pub async fn create_publish_task(
    db: State<'_, Database>,
    request: PublishRequest,
) -> Result<PublishResult, String> {
    // Validate video file exists
    let video_path = Path::new(&request.video_path);
    if !video_path.exists() {
        return Err(format!("Video file not found: {}", request.video_path));
    }
    let metadata = std::fs::metadata(video_path)
        .map_err(|e| format!("Failed to read video file metadata: {}", e))?;
    if !metadata.is_file() {
        return Err(format!("Video path is not a file: {}", request.video_path));
    }
    if metadata.len() == 0 {
        return Err(format!("Video file is empty: {}", request.video_path));
    }
    info!(
        "Validated video file: path={} size_mb={:.2}",
        request.video_path,
        metadata.len() as f64 / (1024.0 * 1024.0)
    );

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
        )
        .map_err(|e| e.to_string())?;

        let accounts = queries::get_all_accounts(&conn).map_err(|e| e.to_string())?;

        // Collect account info for publishing
        let mut accounts_info = Vec::new();
        for account_id in &request.account_ids {
            let account = accounts
                .iter()
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

        info!(
            "Publishing to {} (account {})",
            platform_info.name, account_id
        );

        let session_result =
            chrome::prepare_chrome_session(&chrome_path, &profile_dir, &platform_info.upload_url)
                .await;

        match session_result {
            Ok(session) => {
                let session_mode = Some(session.mode.as_str().to_string());
                let automation_result = tokio::time::timeout(
                    std::time::Duration::from_secs(AUTOMATION_TIMEOUT_SECS),
                    automate_platform(
                        &chrome_path,
                        &session,
                        &profile_dir,
                        platform,
                        &platform_info.upload_url,
                        &request.video_path,
                        &request.title,
                        request.description.as_deref().unwrap_or(""),
                        &request.tags,
                    ),
                )
                .await;

                match automation_result {
                    Ok(Ok(success)) => {
                        let status = if success.automation_phase == "manual_continue" {
                            "launched"
                        } else {
                            "automated"
                        };
                        platform_tasks.push(PlatformTaskResult {
                            account_id: *account_id,
                            platform: platform.clone(),
                            status: status.into(),
                            message: Some(success.message),
                            error_code: None,
                            action_hint: None,
                            debug_port_used: Some(success.debug_port_used),
                            session_mode,
                            automation_phase: Some(success.automation_phase.into()),
                        });
                    }
                    Ok(Err(err)) => {
                        info!(
                            "Automation failed for {}: {}",
                            platform_info.name, err.message
                        );
                        platform_tasks.push(PlatformTaskResult {
                            account_id: *account_id,
                            platform: platform.clone(),
                            status: "launched".into(),
                            message: Some(format!(
                                "Chrome 已打开 {}，但自动填充失败：{}。请手动操作。",
                                platform_info.name, err.message
                            )),
                            error_code: Some(err.code),
                            action_hint: err.action_hint,
                            debug_port_used: err.debug_port_used.or(Some(session.port)),
                            session_mode,
                            automation_phase: Some("automation_failed".into()),
                        });
                    }
                    Err(_) => {
                        platform_tasks.push(PlatformTaskResult {
                            account_id: *account_id,
                            platform: platform.clone(),
                            status: "launched".into(),
                            message: Some(format!(
                                "Chrome 已打开 {}，自动化处理超时（{} 秒）。请手动继续。",
                                platform_info.name, AUTOMATION_TIMEOUT_SECS
                            )),
                            error_code: Some("AUTOMATION_TIMEOUT".into()),
                            action_hint: Some(ACTION_HINT_AUTOMATION_TIMEOUT.into()),
                            debug_port_used: Some(session.port),
                            session_mode,
                            automation_phase: Some("timeout".into()),
                        });
                    }
                }
            }
            Err(e) => {
                let err = PlatformAutomationError::from_raw(&e.to_string());
                let status = if err.code == "PROFILE_BUSY" {
                    "launched"
                } else {
                    "failed"
                };
                let phase = if err.code == "PROFILE_BUSY" {
                    "manual_continue"
                } else {
                    "automation_failed"
                };
                platform_tasks.push(PlatformTaskResult {
                    account_id: *account_id,
                    platform: platform.clone(),
                    status: status.into(),
                    message: Some(err.message),
                    error_code: Some(err.code),
                    action_hint: err.action_hint,
                    debug_port_used: err.debug_port_used,
                    session_mode: Some("manual_only".into()),
                    automation_phase: Some(phase.into()),
                });
            }
        }
    }

    // Update task status
    {
        let conn = db.conn.lock().map_err(|e| e.to_string())?;
        let has_automated = platform_tasks.iter().any(|t| t.status == "automated");
        let new_status = if has_automated {
            "publishing"
        } else {
            "partial"
        };
        queries::update_task_status(&conn, task_id, new_status).map_err(|e| e.to_string())?;
    }

    Ok(PublishResult {
        task_id,
        platform_tasks,
    })
}

/// Run platform-specific automation via CDP
async fn automate_platform(
    _chrome_path: &Path,
    session: &chrome::ChromeSession,
    profile_dir: &Path,
    platform: &str,
    upload_url: &str,
    video_path: &str,
    title: &str,
    description: &str,
    tags: &[String],
) -> Result<AutomationSuccess, PlatformAutomationError> {
    // Wait for Chrome to be ready
    info!(
        "Waiting for Chrome to be ready: port={} mode={} platform={} target_url={}",
        session.port,
        session.mode.as_str(),
        platform,
        upload_url
    );
    let session_ready_start = Instant::now();
    let ready_port = chrome::wait_for_chrome_ready(session, profile_dir, 30)
        .await
        .map_err(|e| {
            PlatformAutomationError::from_raw(&e.to_string()).with_debug_port(session.port)
        })?;
    let session_ready_ms = session_ready_start.elapsed().as_millis();
    info!(
        "[Automation timing] platform={} session_ready_ms={} ready_port={} session_mode={}",
        platform,
        session_ready_ms,
        ready_port,
        session.mode.as_str()
    );

    // Connect via CDP
    let cdp_connect_start = Instant::now();
    info!("Connecting to Chrome via CDP on port {}...", ready_port);
    let (_browser, page) = automation::connect_to_chrome(ready_port, upload_url)
        .await
        .map_err(|e| {
            PlatformAutomationError::from_raw(&e.to_string()).with_debug_port(ready_port)
        })?;
    let cdp_connect_ms = cdp_connect_start.elapsed().as_millis();
    info!(
        "[Automation timing] platform={} cdp_connect_ms={} ready_port={}",
        platform, cdp_connect_ms, ready_port
    );

    // Run platform-specific automation
    let upload_trigger_start = Instant::now();
    let upload_signal = match platform {
        "douyin" => crate::platforms::douyin::auto_publish(&page, video_path, title, description, tags)
            .await,
        "xiaohongshu" => {
            crate::platforms::xiaohongshu::auto_publish(&page, video_path, title, description, tags)
                .await
        }
        "bilibili" => {
            crate::platforms::bilibili::auto_publish(&page, video_path, title, description, tags)
                .await
        }
        "wechat" => crate::platforms::wechat::auto_publish(&page, video_path, title, description, tags)
            .await,
        "youtube" => {
            crate::platforms::youtube::auto_publish(&page, video_path, title, description, tags)
                .await
        }
        _ => {
            return Ok(AutomationSuccess {
                message: "Chrome 已打开到平台上传页面。请手动完成操作。".into(),
                debug_port_used: ready_port,
                automation_phase: "manual_continue",
            });
        }
    }
    .map_err(|e| {
        let normalized = normalize_platform_error(e.to_string());
        PlatformAutomationError::from_raw(&normalized).with_debug_port(ready_port)
    })?;

    let upload_trigger_ms = upload_trigger_start.elapsed().as_millis();
    info!(
        "[Automation timing] platform={} upload_trigger_ms={} signal={} ready_port={} session_mode={}",
        platform,
        upload_trigger_ms,
        upload_signal,
        ready_port,
        session.mode.as_str()
    );

    let platform_name = platform_display_name(platform);
    Ok(AutomationSuccess {
        message: format!(
            "{}：已触发上传并尝试填写基础信息（{}）。请在 Chrome 继续检查并发布。",
            platform_name, upload_signal
        ),
        debug_port_used: ready_port,
        automation_phase: "upload_started",
    })
}

fn normalize_platform_error(raw: String) -> String {
    let upper = raw.to_uppercase();
    if upper.contains("TARGET_PAGE_NOT_FOUND")
        || upper.contains("TARGET_PAGE_NOT_READY")
        || upper.contains("LOGIN_REQUIRED")
        || upper.contains("WECHAT_CHOOSER_NOT_OPENED")
        || upper.contains("WECHAT_UPLOAD_SIGNAL_TIMEOUT")
        || upper.contains("PROFILE_BUSY")
        || upper.contains("CDP_NO_PAGE")
        || upper.contains("CHROME_NOT_READY")
        || upper.contains("AUTOMATION_TIMEOUT")
    {
        raw
    } else {
        format!("AUTOMATION_FAILED: {}", raw)
    }
}

fn platform_display_name(platform: &str) -> &'static str {
    match platform {
        "douyin" => "抖音",
        "xiaohongshu" => "小红书",
        "bilibili" => "哔哩哔哩",
        "wechat" => "微信视频号",
        "youtube" => "YouTube",
        _ => "平台",
    }
}

fn classify_error(raw: &str) -> (&'static str, Option<String>) {
    let upper = raw.to_uppercase();
    if upper.contains("TARGET_PAGE_NOT_FOUND") {
        return (
            "TARGET_PAGE_NOT_FOUND",
            Some(ACTION_HINT_TARGET_PAGE_NOT_FOUND.to_string()),
        );
    }
    if upper.contains("TARGET_PAGE_NOT_READY") {
        return (
            "TARGET_PAGE_NOT_READY",
            Some(ACTION_HINT_TARGET_PAGE_NOT_READY.to_string()),
        );
    }
    if upper.contains("LOGIN_REQUIRED") {
        return (
            "LOGIN_REQUIRED",
            Some(ACTION_HINT_LOGIN_REQUIRED.to_string()),
        );
    }
    if upper.contains("WECHAT_CHOOSER_NOT_OPENED") {
        return (
            "WECHAT_CHOOSER_NOT_OPENED",
            Some(ACTION_HINT_WECHAT_CHOOSER_NOT_OPENED.to_string()),
        );
    }
    if upper.contains("WECHAT_UPLOAD_SIGNAL_TIMEOUT") {
        return (
            "WECHAT_UPLOAD_SIGNAL_TIMEOUT",
            Some(ACTION_HINT_WECHAT_UPLOAD_SIGNAL_TIMEOUT.to_string()),
        );
    }
    if upper.contains("PROFILE_BUSY") {
        return ("PROFILE_BUSY", Some(ACTION_HINT_CLOSE_WINDOW.to_string()));
    }
    if upper.contains("CDP_NO_PAGE") {
        return ("CDP_NO_PAGE", Some(ACTION_HINT_CLOSE_WINDOW.to_string()));
    }
    if upper.contains("没有可操作页面") || upper.contains("没有找到页面") {
        return ("CDP_NO_PAGE", Some(ACTION_HINT_CLOSE_WINDOW.to_string()));
    }
    if upper.contains("CHROME_NOT_READY") {
        return (
            "CHROME_NOT_READY",
            Some(ACTION_HINT_CHECK_CHROME.to_string()),
        );
    }
    if upper.contains("连接 CHROME 端口")
        || upper.contains("CDP CONNECTION FAILED")
        || upper.contains("CHROME 调试端口")
    {
        return (
            "CHROME_NOT_READY",
            Some(ACTION_HINT_CHECK_CHROME.to_string()),
        );
    }
    if upper.contains("AUTOMATION_FAILED") {
        return (
            "AUTOMATION_FAILED",
            Some("请在 Chrome 页面手动完成上传并继续发布。".to_string()),
        );
    }
    if upper.contains("AUTOMATION_TIMEOUT") {
        return (
            "AUTOMATION_TIMEOUT",
            Some(ACTION_HINT_AUTOMATION_TIMEOUT.to_string()),
        );
    }

    ("UNKNOWN", None)
}

fn strip_error_code_prefix(raw: &str) -> String {
    let candidates = [
        "TARGET_PAGE_NOT_FOUND:",
        "TARGET_PAGE_NOT_READY:",
        "LOGIN_REQUIRED:",
        "WECHAT_CHOOSER_NOT_OPENED:",
        "WECHAT_UPLOAD_SIGNAL_TIMEOUT:",
        "PROFILE_BUSY:",
        "CDP_NO_PAGE:",
        "CHROME_NOT_READY:",
        "AUTOMATION_FAILED:",
        "AUTOMATION_TIMEOUT:",
    ];
    let upper = raw.to_uppercase();
    for prefix in candidates {
        if upper.starts_with(prefix) {
            return raw[prefix.len()..].trim().to_string();
        }
    }
    raw.to_string()
}

/// Get all publish tasks
#[tauri::command]
pub fn get_publish_tasks(db: State<'_, Database>) -> Result<Vec<queries::PublishTask>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    queries::get_all_tasks(&conn).map_err(|e| e.to_string())
}
