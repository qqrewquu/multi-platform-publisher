use super::traits::PlatformInfo;

pub fn info() -> PlatformInfo {
    PlatformInfo {
        id: "douyin".into(),
        name: "抖音".into(),
        name_en: "Douyin".into(),
        login_url: "https://creator.douyin.com".into(),
        upload_url: "https://creator.douyin.com/creator-micro/content/upload".into(),
        color: "#fe2c55".into(),
    }
}
