use super::traits::PlatformInfo;

pub fn info() -> PlatformInfo {
    PlatformInfo {
        id: "youtube".into(),
        name: "YouTube".into(),
        name_en: "YouTube".into(),
        login_url: "https://accounts.google.com".into(),
        upload_url: "https://studio.youtube.com".into(),
        color: "#ff0000".into(),
    }
}
