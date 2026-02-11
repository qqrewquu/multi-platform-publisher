use anyhow::{bail, Context, Result};
use log::info;
use serde::Deserialize;
use std::collections::BTreeSet;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};

const DEBUG_PORT_START: u16 = 9300;
const DEBUG_PORT_END: u16 = 9800;

#[derive(Debug, Deserialize)]
struct CdpTarget {
    #[serde(rename = "type")]
    target_type: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChromeSessionMode {
    ReusedExisting,
    LaunchedNew,
}

impl ChromeSessionMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ReusedExisting => "reused_existing",
            Self::LaunchedNew => "launched_new",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ChromeSession {
    pub port: u16,
    pub mode: ChromeSessionMode,
}

/// Allocate an available debugging port by probing localhost listeners.
pub fn allocate_port() -> Result<u16> {
    for port in DEBUG_PORT_START..=DEBUG_PORT_END {
        if TcpListener::bind(("127.0.0.1", port)).is_ok() {
            return Ok(port);
        }
    }

    bail!(
        "No available Chrome debugging port in range {}-{}",
        DEBUG_PORT_START,
        DEBUG_PORT_END
    )
}

/// Detect Chrome installation path on the current OS
pub fn detect_chrome() -> Result<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        let paths = [
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
            "/Applications/Chromium.app/Contents/MacOS/Chromium",
        ];
        for p in &paths {
            let path = PathBuf::from(p);
            if path.exists() {
                return Ok(path);
            }
        }
        if let Ok(path) = which::which("google-chrome") {
            return Ok(path);
        }
    }

    #[cfg(target_os = "windows")]
    {
        let paths = [
            r"C:\Program Files\Google\Chrome\Application\chrome.exe",
            r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
        ];
        for p in &paths {
            let path = PathBuf::from(p);
            if path.exists() {
                return Ok(path);
            }
        }
        if let Ok(path) = which::which("chrome") {
            return Ok(path);
        }
    }

    #[cfg(target_os = "linux")]
    {
        let names = [
            "google-chrome",
            "google-chrome-stable",
            "chromium-browser",
            "chromium",
        ];
        for name in &names {
            if let Ok(path) = which::which(name) {
                return Ok(path);
            }
        }
    }

    bail!("Could not find Chrome browser. Please install Google Chrome.")
}

/// Get the base directory for storing Chrome profiles
pub fn get_profiles_base_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Cannot find home directory")?;
    let base = home.join(".multi-publisher").join("profiles");
    std::fs::create_dir_all(&base)?;
    Ok(base)
}

/// Create a new profile directory for a platform account
pub fn create_profile_dir(platform: &str, account_index: u32) -> Result<PathBuf> {
    let base = get_profiles_base_dir()?;
    let profile_dir = base.join(format!("{}-{}", platform, account_index));
    std::fs::create_dir_all(&profile_dir)?;
    Ok(profile_dir)
}

/// Get the next available account index for a platform
pub fn next_profile_index(platform: &str) -> Result<u32> {
    let base = get_profiles_base_dir()?;
    let mut max_index = 0u32;
    if let Ok(entries) = std::fs::read_dir(&base) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with(&format!("{}-", platform)) {
                if let Some(idx_str) = name.strip_prefix(&format!("{}-", platform)) {
                    if let Ok(idx) = idx_str.parse::<u32>() {
                        max_index = max_index.max(idx);
                    }
                }
            }
        }
    }
    Ok(max_index + 1)
}

/// Launch Chrome with a debugging port and return (Child, port)
pub fn launch_chrome_with_debug(
    chrome_path: &Path,
    profile_dir: &Path,
    url: &str,
) -> Result<(Child, u16)> {
    let port = allocate_port()?;
    info!(
        "[Chrome launch] preparing profile={} port={} url={}",
        profile_dir.display(),
        port,
        url
    );

    let child = Command::new(chrome_path)
        .arg(format!("--user-data-dir={}", profile_dir.display()))
        .arg(format!("--remote-debugging-port={}", port))
        .arg("--new-window")
        .arg("--no-first-run")
        .arg("--no-default-browser-check")
        .arg("--disable-default-apps")
        .arg("--deny-permission-prompts")
        .arg("--disable-background-timer-throttling")
        .arg("--disable-backgrounding-occluded-windows")
        .arg("--disable-renderer-backgrounding")
        .arg(format!("--window-size={},{}", 1280, 800))
        .arg(url)
        .spawn()
        .context("Failed to launch Chrome")?;

    info!(
        "Launched Chrome (PID: {}, port: {}) profile: {}",
        child.id(),
        port,
        profile_dir.display()
    );
    Ok((child, port))
}

/// Launch Chrome for login (no automation needed)
pub fn launch_chrome_for_login(
    chrome_path: &Path,
    profile_dir: &Path,
    login_url: &str,
) -> Result<Child> {
    let (child, _port) = launch_chrome_with_debug(chrome_path, profile_dir, login_url)?;
    Ok(child)
}

/// Ask Chrome to open a URL in a new window for the given profile.
/// This intentionally does not pass remote-debugging-port to avoid port mismatch
/// when reusing an already-running debuggable session.
pub fn open_url_in_profile_new_window(
    chrome_path: &Path,
    profile_dir: &Path,
    url: &str,
) -> Result<()> {
    info!(
        "[Chrome session] request new window profile={} url={}",
        profile_dir.display(),
        url
    );
    let child = Command::new(chrome_path)
        .arg(format!("--user-data-dir={}", profile_dir.display()))
        .arg("--new-window")
        .arg("--no-first-run")
        .arg("--no-default-browser-check")
        .arg("--disable-default-apps")
        .arg("--deny-permission-prompts")
        .arg(format!("--window-size={},{}", 1280, 800))
        .arg(url)
        .spawn()
        .context("Failed to request Chrome new window")?;

    info!(
        "[Chrome session] new window requested pid={} profile={} url={}",
        child.id(),
        profile_dir.display(),
        url
    );
    Ok(())
}

/// Prepare a usable Chrome session for one profile:
/// - Reuse existing debuggable session when possible.
/// - If profile is busy but not attachable, return PROFILE_BUSY.
/// - Otherwise launch a new Chrome instance.
pub async fn prepare_chrome_session(
    chrome_path: &Path,
    profile_dir: &Path,
    url: &str,
) -> Result<ChromeSession> {
    if let Some(port) = discover_profile_debug_port(profile_dir).await? {
        info!(
            "[Chrome session] reusing existing debuggable session profile={} port={}",
            profile_dir.display(),
            port
        );
        return Ok(ChromeSession {
            port,
            mode: ChromeSessionMode::ReusedExisting,
        });
    }

    if is_profile_busy(profile_dir) {
        bail!(
            "PROFILE_BUSY: 检测到该账号 Chrome 会话已占用且无法附加调试端口。请先关闭该账号已打开的 Chrome 窗口后重试。"
        );
    }

    let (_child, port) = launch_chrome_with_debug(chrome_path, profile_dir, url)?;
    Ok(ChromeSession {
        port,
        mode: ChromeSessionMode::LaunchedNew,
    })
}

/// Wait for Chrome debugging endpoint to be ready.
/// Returns the actual active debugging port (may differ after one-time rediscovery).
pub async fn wait_for_chrome_ready(
    session: &ChromeSession,
    profile_dir: &Path,
    timeout_secs: u64,
) -> Result<u16> {
    let start = std::time::Instant::now();
    let timeout = std::time::Duration::from_secs(timeout_secs);
    let mut active_port = session.port;
    let mut saw_version = false;
    let mut rediscovered_once = false;

    loop {
        if start.elapsed() > timeout {
            if saw_version {
                bail!(
                    "PROFILE_BUSY: Chrome 调试端口 {} 可访问，但在 {} 秒内没有可操作页面。请先关闭该账号已打开的 Chrome 窗口后重试。",
                    active_port,
                    timeout_secs
                );
            }

            if is_profile_busy(profile_dir) {
                bail!(
                    "PROFILE_BUSY: 检测到该账号 Chrome 会话被占用且无法附加（端口 {}）。请先关闭该账号已打开的 Chrome 窗口后重试。",
                    active_port
                );
            }

            bail!(
                "CHROME_NOT_READY: Chrome 在 {} 秒内未就绪（端口 {}）。请检查 Chrome 是否成功启动后重试。",
                timeout_secs,
                active_port
            );
        }

        let version_url = format!("http://127.0.0.1:{}/json/version", active_port);
        match reqwest::get(&version_url).await {
            Ok(resp) => {
                if resp.status().is_success() {
                    saw_version = true;
                    match has_page_target(active_port).await {
                        Ok(true) => {
                            info!(
                                "Chrome is ready on port {} (version endpoint + page target ready)",
                                active_port
                            );
                            return Ok(active_port);
                        }
                        Ok(false) => {
                            info!(
                                "Chrome version endpoint ready on port {}, waiting for page target...",
                                active_port
                            );
                        }
                        Err(e) => {
                            info!(
                                "Chrome version endpoint ready on port {}, page target check failed: {}",
                                active_port, e
                            );
                        }
                    }
                }
            }
            Err(_) => {}
        }

        if !rediscovered_once {
            if let Some(discovered_port) = discover_profile_debug_port(profile_dir).await? {
                if discovered_port != active_port {
                    info!(
                        "[Chrome ready] switched active port by profile rediscovery: {} -> {}",
                        active_port, discovered_port
                    );
                    active_port = discovered_port;
                }
            }
            rediscovered_once = true;
        }

        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
}

/// Discover an existing debuggable Chrome session port for a profile.
pub async fn discover_profile_debug_port(profile_dir: &Path) -> Result<Option<u16>> {
    let mut candidates: BTreeSet<u16> = BTreeSet::new();
    if let Some(port) = read_devtools_active_port(profile_dir) {
        candidates.insert(port);
    }
    for port in running_profile_debug_ports(profile_dir) {
        candidates.insert(port);
    }

    for port in candidates {
        if is_port_version_ready(port).await {
            return Ok(Some(port));
        }
    }

    Ok(None)
}

fn read_devtools_active_port(profile_dir: &Path) -> Option<u16> {
    let file = profile_dir.join("DevToolsActivePort");
    let body = std::fs::read_to_string(file).ok()?;
    let first_line = body.lines().next()?.trim();
    first_line.parse::<u16>().ok()
}

#[cfg(unix)]
fn running_profile_debug_ports(profile_dir: &Path) -> Vec<u16> {
    let output = match Command::new("ps").args(["-ax", "-o", "command="]).output() {
        Ok(output) => output,
        Err(_) => return Vec::new(),
    };
    if !output.status.success() {
        return Vec::new();
    }

    let mut ports = Vec::new();
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if !line.contains("--user-data-dir=") || !line.contains("--remote-debugging-port=") {
            continue;
        }
        if !matches_profile_user_data_dir(line, profile_dir) {
            continue;
        }
        if let Some(port) = extract_flag_u16(line, "--remote-debugging-port=") {
            ports.push(port);
        }
    }
    ports
}

#[cfg(not(unix))]
fn running_profile_debug_ports(_profile_dir: &Path) -> Vec<u16> {
    Vec::new()
}

fn matches_profile_user_data_dir(cmdline: &str, profile_dir: &Path) -> bool {
    let profile = profile_dir.to_string_lossy();
    let plain = format!("--user-data-dir={}", profile);
    let double_quote = format!("--user-data-dir=\"{}\"", profile);
    let single_quote = format!("--user-data-dir='{}'", profile);
    cmdline.contains(&plain) || cmdline.contains(&double_quote) || cmdline.contains(&single_quote)
}

fn extract_flag_u16(cmdline: &str, prefix: &str) -> Option<u16> {
    for token in cmdline.split_whitespace() {
        if let Some(raw) = token.strip_prefix(prefix) {
            let trimmed = raw.trim_matches('"').trim_matches('\'');
            if let Ok(val) = trimmed.parse::<u16>() {
                return Some(val);
            }
        }
    }
    None
}

async fn is_port_version_ready(port: u16) -> bool {
    let version_url = format!("http://127.0.0.1:{}/json/version", port);
    match reqwest::get(&version_url).await {
        Ok(resp) => resp.status().is_success(),
        Err(_) => false,
    }
}

fn has_singleton_artifacts(profile_dir: &Path) -> bool {
    ["SingletonLock", "SingletonCookie", "SingletonSocket"]
        .iter()
        .any(|name| profile_dir.join(name).exists())
}

fn singleton_lock_pid(profile_dir: &Path) -> Option<u32> {
    let lock_path = profile_dir.join("SingletonLock");
    let target = std::fs::read_link(lock_path).ok()?;
    let name = target.file_name()?.to_string_lossy();
    let pid_part = name.rsplit('-').next()?;
    pid_part.parse::<u32>().ok()
}

#[cfg(unix)]
fn is_pid_running(pid: u32) -> bool {
    let pid_text = pid.to_string();
    let output = match Command::new("ps")
        .args(["-p", &pid_text, "-o", "pid="])
        .output()
    {
        Ok(output) => output,
        Err(_) => return false,
    };
    if !output.status.success() {
        return false;
    }
    !String::from_utf8_lossy(&output.stdout).trim().is_empty()
}

#[cfg(not(unix))]
fn is_pid_running(_pid: u32) -> bool {
    false
}

pub fn is_profile_busy(profile_dir: &Path) -> bool {
    if !has_singleton_artifacts(profile_dir) {
        return false;
    }

    match singleton_lock_pid(profile_dir) {
        Some(pid) => is_pid_running(pid),
        None => true,
    }
}

async fn has_page_target(port: u16) -> Result<bool> {
    let list_url = format!("http://127.0.0.1:{}/json/list", port);
    let resp = reqwest::get(&list_url)
        .await
        .context("请求 Chrome json/list 失败")?;

    if !resp.status().is_success() {
        return Ok(false);
    }

    let body = resp.text().await.unwrap_or_default();
    let targets: Vec<CdpTarget> = serde_json::from_str(&body).unwrap_or_default();
    Ok(targets.iter().any(|target| target.target_type == "page"))
}

/// Delete a Chrome profile directory
pub fn delete_profile(profile_dir: &Path) -> Result<()> {
    if profile_dir.exists() {
        std::fs::remove_dir_all(profile_dir)?;
        info!("Deleted Chrome profile: {}", profile_dir.display());
    }
    Ok(())
}
