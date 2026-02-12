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
        "button[aria-label*='Create']",
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
        "ytcp-uploads-dialog",
        "ytcp-video-upload-progress",
        "[id*='upload']",
    ],
    pre_click_selectors: &[
        "ytcp-button#create-icon",
        "#create-icon",
        "button[aria-label*='Create']",
        "[aria-label*='Create']",
    ],
    click_selectors: &[
        "tp-yt-paper-item[test-id*='upload-video']",
        "[test-id*='upload-video']",
        "[role='menuitem'][aria-label*='Upload']",
        "[aria-label*='Upload videos']",
    ],
    click_text_markers: &["Upload videos", "Upload video", "上传视频", "Select files"],
    require_surface_ready: true,
    fill_failure_is_error: false,
    weak_ready_self_heal: false,
    weak_ready_min_body_text_len: 0,
    blocked_text_markers: &[],
    init_text_markers: &[],
    login_text_markers: &[],
    title_selectors: &[
        "#title-textarea #textbox",
        "#title-textarea [contenteditable='true']",
        "textarea#textbox",
        "input[aria-label*='Add a title']",
        "input[aria-label*='Title']",
        "[aria-label*='标题']",
    ],
    title_editable_selector: Some("#title-textarea #textbox, [contenteditable='true']"),
    description_selectors: &[
        "#description-textarea #textbox",
        "#description-textarea [contenteditable='true']",
        "textarea[aria-label*='Tell viewers about your video']",
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
