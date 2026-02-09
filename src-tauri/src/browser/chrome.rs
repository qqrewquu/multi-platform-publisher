use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use std::process::{Command, Child};
use std::sync::atomic::{AtomicU16, Ordering};
use log::info;

/// Port counter for allocating unique debugging ports
static NEXT_PORT: AtomicU16 = AtomicU16::new(9300);

/// Allocate a unique debugging port
pub fn allocate_port() -> u16 {
    NEXT_PORT.fetch_add(1, Ordering::SeqCst)
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
        let names = ["google-chrome", "google-chrome-stable", "chromium-browser", "chromium"];
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
    let port = allocate_port();

    let child = Command::new(chrome_path)
        .arg(format!("--user-data-dir={}", profile_dir.display()))
        .arg(format!("--remote-debugging-port={}", port))
        .arg("--no-first-run")
        .arg("--no-default-browser-check")
        .arg("--disable-default-apps")
        .arg("--disable-background-timer-throttling")
        .arg("--disable-backgrounding-occluded-windows")
        .arg("--disable-renderer-backgrounding")
        .arg(format!("--window-size={},{}", 1280, 800))
        .arg(url)
        .spawn()
        .context("Failed to launch Chrome")?;

    info!("Launched Chrome (PID: {}, port: {}) profile: {}", child.id(), port, profile_dir.display());
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

/// Wait for Chrome debugging endpoint to be ready
pub async fn wait_for_chrome_ready(port: u16, timeout_secs: u64) -> Result<String> {
    let url = format!("http://127.0.0.1:{}/json/version", port);
    let start = std::time::Instant::now();
    let timeout = std::time::Duration::from_secs(timeout_secs);

    loop {
        if start.elapsed() > timeout {
            bail!("Chrome did not become ready within {} seconds on port {}", timeout_secs, port);
        }

        match reqwest::get(&url).await {
            Ok(resp) => {
                if resp.status().is_success() {
                    let body = resp.text().await.unwrap_or_default();
                    info!("Chrome is ready on port {}", port);
                    return Ok(body);
                }
            }
            Err(_) => {}
        }

        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
}

/// Delete a Chrome profile directory
pub fn delete_profile(profile_dir: &Path) -> Result<()> {
    if profile_dir.exists() {
        std::fs::remove_dir_all(profile_dir)?;
        info!("Deleted Chrome profile: {}", profile_dir.display());
    }
    Ok(())
}
