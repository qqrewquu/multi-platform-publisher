use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use std::process::{Command, Child};
use log::info;

/// Detect Chrome installation path on the current OS
pub fn detect_chrome() -> Result<PathBuf> {
    // macOS paths
    #[cfg(target_os = "macos")]
    {
        let paths = [
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
            "/Applications/Chromium.app/Contents/MacOS/Chromium",
        ];
        for p in &paths {
            let path = PathBuf::from(p);
            if path.exists() {
                info!("Found Chrome at: {}", path.display());
                return Ok(path);
            }
        }
        // Try which
        if let Ok(path) = which::which("google-chrome") {
            return Ok(path);
        }
    }

    // Windows paths
    #[cfg(target_os = "windows")]
    {
        let paths = [
            r"C:\Program Files\Google\Chrome\Application\chrome.exe",
            r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
        ];
        for p in &paths {
            let path = PathBuf::from(p);
            if path.exists() {
                info!("Found Chrome at: {}", path.display());
                return Ok(path);
            }
        }
        // Try PATH
        if let Ok(path) = which::which("chrome") {
            return Ok(path);
        }
    }

    // Linux paths
    #[cfg(target_os = "linux")]
    {
        let names = ["google-chrome", "google-chrome-stable", "chromium-browser", "chromium"];
        for name in &names {
            if let Ok(path) = which::which(name) {
                info!("Found Chrome at: {}", path.display());
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
    info!("Created Chrome profile at: {}", profile_dir.display());
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

/// Launch Chrome browser with a specific profile and navigate to a URL
pub fn launch_chrome(
    chrome_path: &Path,
    profile_dir: &Path,
    url: &str,
    debugging_port: u16,
) -> Result<Child> {
    let child = Command::new(chrome_path)
        .arg(format!("--user-data-dir={}", profile_dir.display()))
        .arg(format!("--remote-debugging-port={}", debugging_port))
        .arg("--no-first-run")
        .arg("--no-default-browser-check")
        .arg("--disable-default-apps")
        .arg(format!("--window-size={},{}", 1280, 800))
        .arg(url)
        .spawn()
        .context("Failed to launch Chrome")?;

    info!("Launched Chrome (PID: {}) with profile: {}", child.id(), profile_dir.display());
    Ok(child)
}

/// Launch Chrome for login (visible mode, no headless)
pub fn launch_chrome_for_login(
    chrome_path: &Path,
    profile_dir: &Path,
    login_url: &str,
) -> Result<Child> {
    // Use port 0 to auto-select, or a fixed range based on profile
    let port = 9222; // We'll use different ports for concurrent sessions later
    launch_chrome(chrome_path, profile_dir, login_url, port)
}

/// Launch Chrome for publishing (can be visible or headless based on settings)
pub fn launch_chrome_for_publish(
    chrome_path: &Path,
    profile_dir: &Path,
    upload_url: &str,
    visible: bool,
) -> Result<Child> {
    let port = 9223;
    if visible {
        launch_chrome(chrome_path, profile_dir, upload_url, port)
    } else {
        // Headless mode for automatic publishing
        let child = Command::new(chrome_path)
            .arg(format!("--user-data-dir={}", profile_dir.display()))
            .arg(format!("--remote-debugging-port={}", port))
            .arg("--no-first-run")
            .arg("--no-default-browser-check")
            .arg("--disable-default-apps")
            .arg("--headless=new")
            .arg(format!("--window-size={},{}", 1280, 800))
            .arg(upload_url)
            .spawn()
            .context("Failed to launch Chrome in headless mode")?;

        info!("Launched Chrome headless (PID: {})", child.id());
        Ok(child)
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
