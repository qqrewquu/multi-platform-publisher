use super::traits::PlatformInfo;

pub fn info() -> PlatformInfo {
    PlatformInfo {
        id: "bilibili".into(),
        name: "哔哩哔哩".into(),
        name_en: "Bilibili".into(),
        login_url: "https://passport.bilibili.com/login".into(),
        upload_url: "https://member.bilibili.com/platform/upload/video/frame".into(),
        color: "#fb7299".into(),
    }
}
