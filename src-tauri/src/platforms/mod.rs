mod common;
pub mod bilibili;
pub mod douyin;
pub mod traits;
pub mod wechat;
pub mod xiaohongshu;
pub mod youtube;

pub use traits::PlatformInfo;

/// Get platform info by platform ID
pub fn get_platform_info(platform: &str) -> Option<PlatformInfo> {
    match platform {
        "bilibili" => Some(bilibili::info()),
        "douyin" => Some(douyin::info()),
        "xiaohongshu" => Some(xiaohongshu::info()),
        "wechat" => Some(wechat::info()),
        "youtube" => Some(youtube::info()),
        _ => None,
    }
}

/// Get all supported platforms
pub fn all_platforms() -> Vec<PlatformInfo> {
    vec![
        douyin::info(),
        xiaohongshu::info(),
        bilibili::info(),
        wechat::info(),
        youtube::info(),
    ]
}
