use super::common::{self, PlatformPublishConfig};
use super::traits::PlatformInfo;
use anyhow::Result;
use chromiumoxide::page::Page;

const XIAOHONGSHU_CONFIG: PlatformPublishConfig = PlatformPublishConfig {
    id: "xiaohongshu",
    name: "小红书",
    upload_url: "https://creator.xiaohongshu.com/publish/publish",
    target_host: "creator.xiaohongshu.com",
    allowed_paths: &["/publish/publish", "/publish"],
    surface_selectors: &[
        "[class*='upload']",
        "[class*='drag']",
        "[class*='drop']",
        "[data-testid*='upload']",
    ],
    surface_text_markers: &["上传视频", "点击上传", "拖拽", "发布笔记"],
    file_input_selectors: &[
        "input[type='file'][accept*='video']",
        "[class*='upload'] input[type='file']",
        "input[type='file']",
    ],
    drop_zone_selectors: &[
        "[class*='upload']",
        "[class*='drag']",
        "[class*='drop']",
        "[class*='content-upload']",
    ],
    click_selectors: &[
        "button[class*='upload']",
        "[class*='upload-btn']",
        "[class*='upload'] button",
        "[data-testid*='upload']",
        "[role='button']",
    ],
    title_selectors: &[
        "input[placeholder*='标题']",
        "input[placeholder*='添加标题']",
        "[class*='title'] input",
        "input[maxlength='20']",
    ],
    title_editable_selector: Some("[contenteditable='true']"),
    description_selectors: &[
        "textarea[placeholder*='描述']",
        "textarea[placeholder*='正文']",
        "[class*='desc'] textarea",
        "[class*='content'] textarea",
    ],
    description_editable_selector: Some("[contenteditable='true']"),
    tag_selectors: &[
        "input[placeholder*='话题']",
        "input[placeholder*='标签']",
        "[class*='tag'] input",
        "[class*='topic'] input",
    ],
};

pub fn info() -> PlatformInfo {
    PlatformInfo {
        id: "xiaohongshu".into(),
        name: "小红书".into(),
        name_en: "Xiaohongshu".into(),
        login_url: "https://creator.xiaohongshu.com".into(),
        upload_url: XIAOHONGSHU_CONFIG.upload_url.into(),
        color: "#ff2442".into(),
    }
}

pub async fn auto_publish(
    page: &Page,
    video_path: &str,
    title: &str,
    description: &str,
    tags: &[String],
) -> Result<String> {
    common::auto_publish_with_config(
        page,
        video_path,
        title,
        description,
        tags,
        &XIAOHONGSHU_CONFIG,
    )
    .await
}
