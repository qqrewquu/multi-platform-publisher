use super::common::{self, PlatformPublishConfig};
use super::traits::PlatformInfo;
use anyhow::Result;
use chromiumoxide::page::Page;

const WECHAT_CONFIG: PlatformPublishConfig = PlatformPublishConfig {
    id: "wechat",
    name: "微信视频号",
    upload_url: "https://channels.weixin.qq.com/platform/post/create",
    target_host: "channels.weixin.qq.com",
    allowed_paths: &["/platform/post/create", "/platform/post"],
    surface_selectors: &[
        "[class*='upload']",
        "[class*='drag']",
        "[class*='drop']",
        "[class*='post-create']",
    ],
    surface_text_markers: &["上传视频", "拖拽", "发布视频", "发表视频"],
    file_input_selectors: &[
        "input[type='file'][accept*='video']",
        "[class*='upload'] input[type='file']",
        "input[type='file']",
    ],
    drop_zone_selectors: &[
        "[class*='upload']",
        "[class*='drag']",
        "[class*='drop']",
        "[class*='post-create']",
    ],
    click_selectors: &[
        "button[class*='upload']",
        "[class*='upload-btn']",
        "[class*='upload'] button",
        "[class*='drag']",
        "[role='button']",
    ],
    title_selectors: &[
        "input[placeholder*='标题']",
        "input[placeholder*='描述']",
        "[class*='title'] input",
        "input[type='text']",
    ],
    title_editable_selector: Some("[contenteditable='true']"),
    description_selectors: &[
        "textarea[placeholder*='描述']",
        "textarea[placeholder*='内容']",
        "[class*='desc'] textarea",
        "[class*='content'] textarea",
    ],
    description_editable_selector: Some("[contenteditable='true']"),
    tag_selectors: &[
        "input[placeholder*='标签']",
        "input[placeholder*='话题']",
        "[class*='tag'] input",
        "[class*='topic'] input",
    ],
};

pub fn info() -> PlatformInfo {
    PlatformInfo {
        id: "wechat".into(),
        name: "微信视频号".into(),
        name_en: "WeChat Channels".into(),
        login_url: "https://channels.weixin.qq.com".into(),
        upload_url: WECHAT_CONFIG.upload_url.into(),
        color: "#07c160".into(),
    }
}

pub async fn auto_publish(
    page: &Page,
    video_path: &str,
    title: &str,
    description: &str,
    tags: &[String],
) -> Result<String> {
    common::auto_publish_with_config(page, video_path, title, description, tags, &WECHAT_CONFIG)
        .await
}
