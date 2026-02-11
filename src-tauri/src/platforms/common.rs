use crate::browser::automation;
use anyhow::{bail, Result};
use chromiumoxide::page::Page;
use log::{info, warn};
use std::path::Path;
use std::time::Duration;

pub const QUICK_SURFACE_WAIT_SECS: u64 = 2;
pub const FAST_SIGNAL_TIMEOUT_SECS: u64 = 2;
pub const SLOW_FALLBACK_SIGNAL_TIMEOUT_SECS: u64 = 6;
pub const FAST_POLL_INTERVAL_MS: u64 = 200;

pub struct PlatformPublishConfig {
    pub id: &'static str,
    pub name: &'static str,
    pub upload_url: &'static str,
    pub target_host: &'static str,
    pub allowed_paths: &'static [&'static str],
    pub surface_selectors: &'static [&'static str],
    pub surface_text_markers: &'static [&'static str],
    pub file_input_selectors: &'static [&'static str],
    pub drop_zone_selectors: &'static [&'static str],
    pub click_selectors: &'static [&'static str],
    pub title_selectors: &'static [&'static str],
    pub title_editable_selector: Option<&'static str>,
    pub description_selectors: &'static [&'static str],
    pub description_editable_selector: Option<&'static str>,
    pub tag_selectors: &'static [&'static str],
}

struct FillSummary {
    title_marker: String,
    description_marker: String,
    title_ok: bool,
    description_ok: bool,
    tags_added: usize,
    tags_total: usize,
}

pub async fn auto_publish_with_config(
    page: &Page,
    video_path: &str,
    title: &str,
    description: &str,
    tags: &[String],
    cfg: &PlatformPublishConfig,
) -> Result<String> {
    info!("开始 {} 自动发布：{}", cfg.name, video_path);
    let file_ext = Path::new(video_path)
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if file_ext == "mov" {
        warn!(
            "[{}上传] 当前文件扩展名为 .mov，部分场景兼容性较差，建议优先使用 .mp4",
            cfg.name
        );
    }

    ensure_upload_context(page, cfg).await?;

    info!("[{}上传] 第1步：快速确认上传页面就绪...", cfg.name);
    if !wait_for_upload_surface_brief(page, cfg, QUICK_SURFACE_WAIT_SECS).await {
        warn!(
            "[{}上传] 在 {} 秒内未确认到上传区域，继续尝试上传动作。",
            cfg.name, QUICK_SURFACE_WAIT_SECS
        );
    }

    info!("[{}上传] 第2步：上传视频文件...", cfg.name);
    let mut upload_signal: Option<String> = None;
    let mut upload_action_performed = false;
    let mut upload_diagnostics = vec![format!(
        "file_ext={}",
        if file_ext.is_empty() {
            "unknown"
        } else {
            &file_ext
        }
    )];

    for selector in cfg.file_input_selectors {
        info!("[{}上传] 策略A：文件选择器拦截，选择器：{}", cfg.name, selector);
        let count = selector_match_count(page, selector).await;
        if count <= 0 {
            upload_diagnostics.push(format!("A:{} count=0", selector));
            continue;
        }
        upload_diagnostics.push(format!("A:{} count={}", selector, count));

        match automation::upload_file_via_file_chooser(page, video_path, selector).await {
            Ok(()) => {
                upload_action_performed = true;
                if let Some(signal) = wait_for_upload_signal(page, cfg, FAST_SIGNAL_TIMEOUT_SECS).await {
                    upload_signal = Some(signal.clone());
                    upload_diagnostics.push(format!("A:{} signal={}", selector, signal));
                    break;
                }
                upload_diagnostics.push(format!(
                    "A:{} no_signal_fast({}s)",
                    selector, FAST_SIGNAL_TIMEOUT_SECS
                ));
            }
            Err(e) => {
                upload_diagnostics.push(format!("A:{} failed={}", selector, e));
            }
        }
    }

    if upload_signal.is_none() {
        info!(
            "[{}上传] 策略A失败，尝试策略B：setFileInputFiles + 事件派发...",
            cfg.name
        );
        for selector in cfg.file_input_selectors {
            let count = selector_match_count(page, selector).await;
            if count <= 0 {
                upload_diagnostics.push(format!("B:{} count=0", selector));
                continue;
            }

            match automation::set_file_input(page, selector, video_path).await {
                Ok(()) => {
                    upload_action_performed = true;
                    let dispatch_js = format!(
                        r#"
                        (function() {{
                            const el = document.querySelector('{}');
                            if (!el) return 'not_found';
                            el.dispatchEvent(new Event('change', {{ bubbles: true }}));
                            el.dispatchEvent(new Event('input', {{ bubbles: true }}));
                            return 'dispatched:files=' + (el.files ? el.files.length : 0);
                        }})()"#,
                        escape_js_single(selector)
                    );
                    let dispatch_result: String = page
                        .evaluate(dispatch_js.as_str())
                        .await
                        .map(|v| v.into_value().unwrap_or_else(|_| "error".to_string()))
                        .unwrap_or_else(|_| "error".to_string());
                    upload_diagnostics.push(format!("B:{} dispatch={}", selector, dispatch_result));

                    if let Some(signal) = wait_for_upload_signal(page, cfg, FAST_SIGNAL_TIMEOUT_SECS).await {
                        upload_signal = Some(signal.clone());
                        upload_diagnostics.push(format!("B:{} signal={}", selector, signal));
                        break;
                    }
                    upload_diagnostics.push(format!(
                        "B:{} no_signal_fast({}s)",
                        selector, FAST_SIGNAL_TIMEOUT_SECS
                    ));
                }
                Err(e) => {
                    upload_diagnostics.push(format!("B:{} failed={}", selector, e));
                }
            }
        }
    }

    if upload_signal.is_none() {
        info!("[{}上传] 策略B失败，尝试策略C：拖拽上传...", cfg.name);
        match automation::upload_file_via_drag_drop(page, video_path, cfg.drop_zone_selectors).await {
            Ok(selector) => {
                upload_action_performed = true;
                upload_diagnostics.push(format!("C:drag_drop selector={}", selector));
                if let Some(signal) = wait_for_upload_signal(page, cfg, FAST_SIGNAL_TIMEOUT_SECS).await {
                    upload_signal = Some(signal.clone());
                    upload_diagnostics.push(format!("C:signal={}", signal));
                } else {
                    upload_diagnostics
                        .push(format!("C:no_signal_fast({}s)", FAST_SIGNAL_TIMEOUT_SECS));
                }
            }
            Err(e) => {
                upload_diagnostics.push(format!("C:failed={}", e));
            }
        }
    }

    if upload_signal.is_none() {
        info!("[{}上传] 策略C后仍未触发，尝试策略D：点击上传入口...", cfg.name);
        match automation::upload_file_via_click_to_open_file_chooser(page, video_path, cfg.click_selectors)
            .await
        {
            Ok(marker) => {
                upload_action_performed = true;
                upload_diagnostics.push(format!("D:clicked={}", marker));
                if let Some(signal) = wait_for_upload_signal(page, cfg, FAST_SIGNAL_TIMEOUT_SECS).await {
                    upload_signal = Some(signal.clone());
                    upload_diagnostics.push(format!("D:signal={}", signal));
                } else {
                    upload_diagnostics
                        .push(format!("D:no_signal_fast({}s)", FAST_SIGNAL_TIMEOUT_SECS));
                }
            }
            Err(e) => {
                upload_diagnostics.push(format!("D:failed={}", e));
            }
        }
    }

    if upload_signal.is_none() && !upload_action_performed {
        bail!(
            "[{}上传] 所有上传策略均失败，请手动上传。诊断：{}",
            cfg.name,
            upload_diagnostics.join(" | ")
        );
    }

    if upload_signal.is_none() {
        if let Some(signal) = wait_for_upload_signal(page, cfg, SLOW_FALLBACK_SIGNAL_TIMEOUT_SECS).await {
            upload_diagnostics.push(format!("fallback:signal={}", signal));
            upload_signal = Some(signal);
        } else {
            upload_diagnostics.push(format!(
                "fallback:no_signal({}s)",
                SLOW_FALLBACK_SIGNAL_TIMEOUT_SECS
            ));
        }
    }

    let started_signal = match upload_signal {
        Some(signal) => signal,
        None => {
            bail!(
                "[{}上传] 已执行上传动作，但在快速检测与兜底检测中都未检测到上传信号。诊断：{}",
                cfg.name,
                upload_diagnostics.join(" | ")
            )
        }
    };

    let fill_summary = fill_basic_fields(page, title, description, tags, cfg).await;
    info!(
        "[{}填表] title={} desc={} tags={}/{}",
        cfg.name,
        fill_summary.title_marker,
        fill_summary.description_marker,
        fill_summary.tags_added,
        fill_summary.tags_total
    );

    if !fill_summary.title_ok && !fill_summary.description_ok {
        bail!(
            "[{}填表] 上传已触发（signal={}），但标题和描述均未命中可编辑字段。诊断：{}",
            cfg.name,
            started_signal,
            upload_diagnostics.join(" | ")
        );
    }

    if fill_summary.tags_total > 0 && fill_summary.tags_added < fill_summary.tags_total {
        warn!(
            "[{}填表] 标签填充部分失败：{}/{}",
            cfg.name, fill_summary.tags_added, fill_summary.tags_total
        );
    }

    Ok(format!(
        "{};fill=title:{},desc:{},tags:{}/{}",
        started_signal,
        marker_status(&fill_summary.title_marker),
        marker_status(&fill_summary.description_marker),
        fill_summary.tags_added,
        fill_summary.tags_total
    ))
}

async fn fill_basic_fields(
    page: &Page,
    title: &str,
    description: &str,
    tags: &[String],
    cfg: &PlatformPublishConfig,
) -> FillSummary {
    let title_marker = match automation::fill_text_input(
        page,
        title,
        cfg.title_selectors,
        cfg.title_editable_selector,
    )
    .await
    {
        Ok(marker) => marker,
        Err(e) => {
            warn!("[{}填表] 标题填写执行异常：{}", cfg.name, e);
            "error".to_string()
        }
    };

    let description_marker = match automation::fill_text_input(
        page,
        description,
        cfg.description_selectors,
        cfg.description_editable_selector,
    )
    .await
    {
        Ok(marker) => marker,
        Err(e) => {
            warn!("[{}填表] 描述填写执行异常：{}", cfg.name, e);
            "error".to_string()
        }
    };

    let tags_total = tags.len();
    let tags_added = if tags_total == 0 {
        0
    } else {
        match automation::add_tags_via_input(page, tags, cfg.tag_selectors).await {
            Ok(count) => count,
            Err(e) => {
                warn!("[{}填表] 标签填写执行异常：{}", cfg.name, e);
                0
            }
        }
    };

    FillSummary {
        title_ok: is_fill_success(&title_marker),
        description_ok: is_fill_success(&description_marker),
        title_marker,
        description_marker,
        tags_added,
        tags_total,
    }
}

async fn ensure_upload_context(page: &Page, cfg: &PlatformPublishConfig) -> Result<()> {
    let before_url = current_url(page).await;
    info!("[{}上传] 页面守卫：当前 URL={}", cfg.name, before_url);

    if !is_target_url(&before_url, cfg) {
        let nav_js = format!("window.location.href = '{}'; 'navigating';", cfg.upload_url);
        page.evaluate(nav_js.as_str()).await.map_err(|e| {
            anyhow::anyhow!(
                "TARGET_PAGE_NOT_READY: 跳转 {} 上传页失败：{}",
                cfg.name,
                e
            )
        })?;
    }

    let timeout = Duration::from_secs(15);
    let start = std::time::Instant::now();
    let mut last_url = before_url;
    loop {
        let host_ok = last_url.contains(cfg.target_host);
        let path_ok = path_allowed(&last_url, cfg.allowed_paths);
        let surface_ok = has_upload_surface(page, cfg).await;

        if host_ok && path_ok && surface_ok {
            info!("[{}上传] 页面守卫通过：{}", cfg.name, last_url);
            return Ok(());
        }

        if start.elapsed() > timeout {
            bail!(
                "TARGET_PAGE_NOT_READY: {} 上传页未就绪。当前URL={}（期望 host={} path={:?}）",
                cfg.name,
                last_url,
                cfg.target_host,
                cfg.allowed_paths
            );
        }

        tokio::time::sleep(Duration::from_millis(FAST_POLL_INTERVAL_MS)).await;
        last_url = current_url(page).await;
    }
}

async fn wait_for_upload_surface_brief(
    page: &Page,
    cfg: &PlatformPublishConfig,
    timeout_secs: u64,
) -> bool {
    if has_upload_surface(page, cfg).await {
        return true;
    }

    let timeout = Duration::from_secs(timeout_secs);
    let start = std::time::Instant::now();
    while start.elapsed() <= timeout {
        tokio::time::sleep(Duration::from_millis(FAST_POLL_INTERVAL_MS)).await;
        if has_upload_surface(page, cfg).await {
            return true;
        }
    }
    false
}

async fn wait_for_upload_signal(
    page: &Page,
    cfg: &PlatformPublishConfig,
    timeout_secs: u64,
) -> Option<String> {
    automation::wait_for_upload_start_signal(
        page,
        cfg.id,
        timeout_secs,
        Duration::from_millis(FAST_POLL_INTERVAL_MS),
    )
    .await
}

async fn has_upload_surface(page: &Page, cfg: &PlatformPublishConfig) -> bool {
    let selectors_array = js_array(cfg.surface_selectors);
    let text_markers_array = js_array(cfg.surface_text_markers);
    let js = format!(
        r#"
        (function() {{
            const hasInput = document.querySelectorAll("input[type='file']").length > 0;
            if (hasInput) return true;

            const selectors = [{}];
            for (const sel of selectors) {{
                try {{
                    if (document.querySelector(sel)) return true;
                }} catch (_) {{}}
            }}

            const text = (document.body && document.body.innerText) ? document.body.innerText : '';
            const markers = [{}];
            for (const marker of markers) {{
                if (marker && text.includes(marker)) return true;
            }}

            return false;
        }})()
    "#,
        selectors_array, text_markers_array
    );

    page.evaluate(js.as_str())
        .await
        .map(|v| v.into_value().unwrap_or(false))
        .unwrap_or(false)
}

async fn selector_match_count(page: &Page, selector: &str) -> i64 {
    let js = format!(
        r#"
        (function() {{
            try {{
                return document.querySelectorAll('{}').length;
            }} catch (_) {{
                return -1;
            }}
        }})()
        "#,
        escape_js_single(selector)
    );

    page.evaluate(js.as_str())
        .await
        .map(|v| v.into_value().unwrap_or(0))
        .unwrap_or(0)
}

async fn current_url(page: &Page) -> String {
    page.evaluate("window.location.href")
        .await
        .map(|v| v.into_value().unwrap_or_else(|_| String::new()))
        .unwrap_or_default()
}

fn is_target_url(url: &str, cfg: &PlatformPublishConfig) -> bool {
    url.contains(cfg.target_host) && path_allowed(url, cfg.allowed_paths)
}

fn path_allowed(url: &str, allowed_paths: &[&str]) -> bool {
    if allowed_paths.is_empty() {
        return true;
    }
    allowed_paths.iter().any(|path| url.contains(path))
}

fn marker_status(marker: &str) -> &'static str {
    if is_fill_success(marker) {
        "ok"
    } else if marker == "skipped_empty" {
        "skip"
    } else {
        "miss"
    }
}

fn is_fill_success(marker: &str) -> bool {
    marker.starts_with("input:") || marker.starts_with("editable")
}

fn js_array(values: &[&str]) -> String {
    values
        .iter()
        .map(|value| format!("'{}'", escape_js_single(value)))
        .collect::<Vec<_>>()
        .join(",")
}

fn escape_js_single(input: &str) -> String {
    input.replace('\\', "\\\\").replace('\'', "\\'")
}
