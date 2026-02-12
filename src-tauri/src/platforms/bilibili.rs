use super::common::{self, PlatformPublishConfig};
use super::traits::PlatformInfo;
use anyhow::Result;
use chromiumoxide::page::Page;

const BILIBILI_CONFIG: PlatformPublishConfig = PlatformPublishConfig {
    id: "bilibili",
    name: "哔哩哔哩",
    upload_url: "https://member.bilibili.com/platform/upload/video/frame",
    target_host: "member.bilibili.com",
    allowed_paths: &["/platform/upload", "/video/frame", "/article"],
    surface_selectors: &[
        "[class*='upload']",
        "[class*='drag']",
        "[class*='drop']",
        "[class*='bcc-upload']",
    ],
    surface_text_markers: &["上传视频", "拖拽视频", "选择视频", "投稿"],
    file_input_selectors: &[
        "input[type='file'][accept*='video']",
        "[class*='upload'] input[type='file']",
        "input[type='file']",
    ],
    drop_zone_selectors: &[
        "[class*='upload']",
        "[class*='drag']",
        "[class*='drop']",
        "[class*='bcc-upload']",
    ],
    pre_click_selectors: &[],
    click_selectors: &[
        "button[class*='upload']",
        "[class*='upload-btn']",
        "[class*='upload'] button",
        "[class*='drag']",
    ],
    click_text_markers: &["上传视频", "选择视频", "上传文件", "投稿"],
    require_surface_ready: true,
    fill_failure_is_error: true,
    weak_ready_self_heal: false,
    weak_ready_min_body_text_len: 0,
    blocked_text_markers: &[],
    init_text_markers: &[],
    login_text_markers: &[],
    title_selectors: &[
        "input[placeholder*='标题']",
        "input[placeholder*='稿件标题']",
        "[class*='title'] input",
        "input[name*='title']",
    ],
    title_editable_selector: Some("[contenteditable='true']"),
    description_selectors: &[
        "textarea[placeholder*='简介']",
        "textarea[placeholder*='描述']",
        "[class*='desc'] textarea",
        "textarea[name*='desc']",
    ],
    description_editable_selector: Some("[contenteditable='true']"),
    tag_selectors: &[
        "input[placeholder*='标签']",
        "input[placeholder*='Enter']",
        "[class*='tag'] input",
        "input[name*='tag']",
    ],
};

pub fn info() -> PlatformInfo {
    PlatformInfo {
        id: "bilibili".into(),
        name: "哔哩哔哩".into(),
        name_en: "Bilibili".into(),
        login_url: "https://passport.bilibili.com/login".into(),
        upload_url: BILIBILI_CONFIG.upload_url.into(),
        color: "#fb7299".into(),
    }
}

pub async fn auto_publish(
    page: &Page,
    video_path: &str,
    title: &str,
    description: &str,
    tags: &[String],
) -> Result<String> {
    common::auto_publish_with_config(page, video_path, title, description, tags, &BILIBILI_CONFIG)
        .await
}
