use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformInfo {
    pub id: String,
    pub name: String,
    pub name_en: String,
    pub login_url: String,
    pub upload_url: String,
    pub color: String,
}
