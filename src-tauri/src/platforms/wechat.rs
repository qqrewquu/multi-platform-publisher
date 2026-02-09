use super::traits::PlatformInfo;

pub fn info() -> PlatformInfo {
    PlatformInfo {
        id: "wechat".into(),
        name: "微信视频号".into(),
        name_en: "WeChat Channels".into(),
        login_url: "https://channels.weixin.qq.com".into(),
        upload_url: "https://channels.weixin.qq.com/platform/post/create".into(),
        color: "#07c160".into(),
    }
}
