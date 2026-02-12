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
        "[class*='uploader']",
        "[class*='post-create']",
    ],
    surface_text_markers: &[
        "上传视频",
        "拖拽",
        "发布视频",
        "发表视频",
        "上传时长",
        "20GB",
        "MP4",
        "上传时长8小时内",
        "大小不超过20GB",
        "格式为MP4/H.264格式",
        "点击或拖拽上传",
    ],
    file_input_selectors: &[
        "input[type='file'][accept*='video']",
        "[class*='uploader'] input[type='file']",
        "[class*='upload'] input[type='file']",
        "input[type='file']",
    ],
    drop_zone_selectors: &[
        "[class*='upload']",
        "[class*='drag']",
        "[class*='drop']",
        "[class*='uploader']",
        "[class*='post-create']",
    ],
    pre_click_selectors: &[],
    click_selectors: &[
        "[role='button'][aria-label*='上传']",
        "button[aria-label*='上传']",
        "label[for*='upload']",
        "[class*='upload'] [role='button']",
        "[class*='upload'] button",
        "button[class*='upload']",
        "[class*='uploader'] button",
        "[class*='upload-btn']",
        "[class*='post-create'] [role='button']",
        "[class*='post-create'] button",
        "[class*='upload']",
        "[class*='drag']",
    ],
    click_text_markers: &[
        "上传",
        "拖拽",
        "上传时长",
        "20GB",
        "MP4",
        "点击上传",
        "选择文件",
        "上传时长8小时内",
        "大小不超过20GB",
        "格式为MP4/H.264格式",
        "点击或拖拽上传",
    ],
    require_surface_ready: false,
    fill_failure_is_error: false,
    weak_ready_self_heal: true,
    weak_ready_min_body_text_len: 0,
    blocked_text_markers: &["暂时无法使用该功能了", "页面加载失败", "请稍后再试", "网络异常"],
    init_text_markers: &["页面初始化中", "初始化中", "正在初始化"],
    login_text_markers: &[
        "扫码登录",
        "微信扫码",
        "请使用微信扫码登录",
        "请在手机上确认登录",
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
