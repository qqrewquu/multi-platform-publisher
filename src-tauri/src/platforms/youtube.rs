use super::common::{self, PlatformPublishConfig};
use super::traits::PlatformInfo;
use anyhow::Result;
use chromiumoxide::page::Page;

const YOUTUBE_CONFIG: PlatformPublishConfig = PlatformPublishConfig {
    id: "youtube",
    name: "YouTube",
    upload_url: "https://studio.youtube.com",
    target_host: "studio.youtube.com",
    allowed_paths: &[],
    surface_selectors: &[
        "ytcp-button#create-icon",
        "#create-icon",
        "[class*='upload']",
        "input[type='file']",
    ],
    surface_text_markers: &["Upload videos", "Select files", "上传视频", "选择文件"],
    file_input_selectors: &[
        "input[type='file'][accept*='video']",
        "input[type='file']",
    ],
    drop_zone_selectors: &[
        "[class*='upload']",
        "ytcp-video-upload-progress",
        "[id*='upload']",
    ],
    click_selectors: &[
        "ytcp-button#create-icon",
        "#create-icon",
        "button[aria-label*='Create']",
        "tp-yt-paper-item[test-id*='upload-video']",
        "[aria-label*='Upload videos']",
    ],
    title_selectors: &[
        "#title-textarea #textbox",
        "textarea#textbox",
        "input[aria-label*='Title']",
        "[aria-label*='标题']",
    ],
    title_editable_selector: Some("#title-textarea #textbox, [contenteditable='true']"),
    description_selectors: &[
        "#description-textarea #textbox",
        "textarea[aria-label*='Description']",
        "textarea[aria-label*='描述']",
        "[id*='description'] #textbox",
    ],
    description_editable_selector: Some("#description-textarea #textbox, [contenteditable='true']"),
    tag_selectors: &[
        "input[aria-label*='Tags']",
        "input[aria-label*='标签']",
        "#text-input input",
        "[class*='tags'] input",
    ],
};

pub fn info() -> PlatformInfo {
    PlatformInfo {
        id: "youtube".into(),
        name: "YouTube".into(),
        name_en: "YouTube".into(),
        login_url: "https://accounts.google.com".into(),
        upload_url: YOUTUBE_CONFIG.upload_url.into(),
        color: "#ff0000".into(),
    }
}

pub async fn auto_publish(
    page: &Page,
    video_path: &str,
    title: &str,
    description: &str,
    tags: &[String],
) -> Result<String> {
    common::auto_publish_with_config(page, video_path, title, description, tags, &YOUTUBE_CONFIG)
        .await
}
