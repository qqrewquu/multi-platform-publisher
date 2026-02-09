use crate::browser::chrome;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ChromeStatus {
    pub found: bool,
    pub path: Option<String>,
    pub error: Option<String>,
}

#[tauri::command]
pub fn detect_chrome() -> ChromeStatus {
    match chrome::detect_chrome() {
        Ok(path) => ChromeStatus {
            found: true,
            path: Some(path.to_string_lossy().to_string()),
            error: None,
        },
        Err(e) => ChromeStatus {
            found: false,
            path: None,
            error: Some(e.to_string()),
        },
    }
}

#[tauri::command]
pub fn get_platforms() -> Vec<crate::platforms::traits::PlatformInfo> {
    crate::platforms::all_platforms()
}
