use super::traits::PlatformInfo;

pub fn info() -> PlatformInfo {
    PlatformInfo {
        id: "xiaohongshu".into(),
        name: "小红书".into(),
        name_en: "Xiaohongshu".into(),
        login_url: "https://creator.xiaohongshu.com".into(),
        upload_url: "https://creator.xiaohongshu.com/publish/publish".into(),
        color: "#ff2442".into(),
    }
}
