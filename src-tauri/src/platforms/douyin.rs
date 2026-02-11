use super::common::{self, PlatformPublishConfig};
use super::traits::PlatformInfo;
use anyhow::Result;
use chromiumoxide::page::Page;

const DOUYIN_CONFIG: PlatformPublishConfig = PlatformPublishConfig {
    id: "douyin",
    name: "抖音",
    upload_url: "https://creator.douyin.com/creator-micro/content/upload",
    target_host: "creator.douyin.com",
    allowed_paths: &[
        "/creator-micro/content/upload",
        "/creator-micro/content/post/video",
    ],
    surface_selectors: &[
        "div[class*='upload']",
        "div[class*='container-drag']",
        "div[class*='content-upload']",
    ],
    surface_text_markers: &["上传视频", "点击上传", "重新上传", "更换视频"],
    file_input_selectors: &[
        "div[class^='container'] input[type='file']",
        "div[class^='container'] input",
        "input[type='file'][accept*='video']",
        "[class*='upload'] input[type='file']",
        "input[type='file']",
    ],
    drop_zone_selectors: &[
        "div[class*='container-drag']",
        "div[class*='upload-zone']",
        "div[class*='upload-area']",
        "div[class*='drag']",
        "div[class*='uploader']",
        "div[class*='upload-btn']",
        "div[class*='content-upload']",
    ],
    click_selectors: &[
        "button[class*='upload']",
        "div[class*='upload-btn']",
        "div[class*='upload'] button",
        "[data-e2e*='upload']",
        "[class*='drag']",
    ],
    title_selectors: &[
        "input[placeholder*='标题']",
        "input[placeholder*='title']",
        ".title-input input",
        "[class*='title'] input[type='text']",
    ],
    title_editable_selector: Some("[contenteditable='true']"),
    description_selectors: &[
        "textarea[placeholder*='描述']",
        "textarea[placeholder*='简介']",
        "[class*='desc'] textarea",
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
        id: "douyin".into(),
        name: "抖音".into(),
        name_en: "Douyin".into(),
        login_url: "https://creator.douyin.com".into(),
        upload_url: DOUYIN_CONFIG.upload_url.into(),
        color: "#fe2c55".into(),
    }
}

pub async fn auto_publish(
    page: &Page,
    video_path: &str,
    title: &str,
    description: &str,
    tags: &[String],
) -> Result<String> {
    common::auto_publish_with_config(page, video_path, title, description, tags, &DOUYIN_CONFIG).await
}
