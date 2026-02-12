use anyhow::{bail, Context, Result};
use chromiumoxide::browser::Browser;
use chromiumoxide::cdp::browser_protocol::dom::{
    BackendNodeId, GetDocumentParams, QuerySelectorParams, SetFileInputFilesParams,
};
use chromiumoxide::cdp::browser_protocol::input::{
    DispatchDragEventParams, DispatchDragEventType, DispatchMouseEventParams,
    DispatchMouseEventType, DragData, MouseButton,
};
use chromiumoxide::cdp::browser_protocol::page::{
    EventFileChooserOpened, SetInterceptFileChooserDialogParams,
};
use chromiumoxide::page::Page;
use futures::StreamExt;
use log::{info, warn};
use std::time::{Duration, Instant};

const STRICT_TARGET_SCORE: i32 = 70;
const CDP_INITIAL_TARGET_WAIT_SECS: u64 = 2;
const CDP_TARGET_RETRY_WAIT_SECS: u64 = 3;

pub struct UploadOptions {
    pub platform: &'static str,
    pub candidate_selectors: Vec<&'static str>,
    pub success_timeout_secs: u64,
    pub attempt_timeout_secs: u64,
}

pub struct UploadAttemptReport {
    pub selected_selector: String,
    pub attempted_selectors: Vec<String>,
    pub start_url: String,
    pub end_url: String,
    pub detected_signal: String,
    pub elapsed_ms: u128,
}

#[derive(Debug, Clone)]
pub struct ClickChooserUploadResult {
    pub marker: String,
    pub chooser_opened: bool,
    pub chooser_event_state: String,
    pub click_method: String,
    pub click_round: u8,
    pub clicked_context: String,
    pub signal_source: String,
    pub file_set: bool,
}

/// 连接到已运行的 Chrome 实例（通过 CDP）
pub async fn connect_to_chrome(port: u16, expected_url: &str) -> Result<(Browser, Page)> {
    let debug_url = format!("http://127.0.0.1:{}", port);

    let (browser, mut handler) = Browser::connect(&debug_url)
        .await
        .context(format!("连接 Chrome 端口 {} 失败", port))?;

    // 启动事件处理器
    tokio::spawn(async move { while let Some(_event) = handler.next().await {} });

    let expected_host = extract_host(expected_url);
    let strict_target = !expected_host.is_empty();
    let create_url = if expected_url.is_empty() {
        "about:blank"
    } else {
        expected_url
    };

    // 先轮询等待页面；严格模式下需要至少一个目标 host 页面出现
    let mut pages = wait_for_pages_or_target(
        &browser,
        &expected_host,
        strict_target,
        CDP_INITIAL_TARGET_WAIT_SECS,
    )
    .await?;
    let mut created_target_page = false;
    let mut redirected_existing_page = false;
    let mut page_open_reason = "none".to_string();

    loop {
        if pages.is_empty() {
            if created_target_page {
                bail!(
                    "CDP_NO_PAGE: Chrome 调试端口 {} 可用，但没有可操作页面。请先关闭该账号已打开的 Chrome 窗口后重试。page_open_reason={}",
                    port,
                    page_open_reason
                );
            }
            info!(
                "[CDP] 端口 {} 暂无页面，尝试主动创建新页面：{}（page_open_reason=create_new_page:no_page）",
                port, create_url
            );
            match browser.new_page(create_url.to_string()).await {
                Ok(_) => {
                    created_target_page = true;
                    page_open_reason = "create_new_page:no_page".to_string();
                    pages = wait_for_pages_or_target(
                        &browser,
                        &expected_host,
                        strict_target,
                        CDP_TARGET_RETRY_WAIT_SECS,
                    )
                    .await?;
                    continue;
                }
                Err(e) => {
                    warn!(
                        "[CDP] 主动创建页面失败（port={} url={} page_open_reason=create_new_page:no_page）：{}",
                        port, create_url, e
                    );
                    bail!(
                        "TARGET_PAGE_NOT_FOUND: 无法创建目标页面。port={} expected_url={} page_open_reason=create_new_page:no_page error={}",
                        port,
                        expected_url,
                        e
                    );
                }
            }
        }

        let selection = select_best_page(&pages, expected_url, &expected_host).await;
        for probe in &selection.observed {
            info!(
                "[CDP] page[{}] url={} score={} ready={} visible={} focus={} title={} body_text_len={}",
                probe.idx,
                probe.url,
                probe.score,
                if probe.ready_complete {
                    "complete"
                } else {
                    "not_complete"
                },
                probe.visible,
                probe.focused,
                probe.title,
                probe.body_text_len
            );
        }
        let top_score = selection
            .observed
            .iter()
            .map(|probe| probe.score)
            .max()
            .unwrap_or(i32::MIN);
        let top_candidates = selection
            .observed
            .iter()
            .filter(|probe| probe.score == top_score)
            .collect::<Vec<_>>();
        if top_candidates.len() > 1 {
            warn!(
                "[CDP] 候选冲突：top_score={} candidates={}",
                top_score,
                top_candidates
                    .iter()
                    .map(|probe| format!(
                        "idx:{} body_text_len:{} visible:{} focus:{}",
                        probe.idx, probe.body_text_len, probe.visible, probe.focused
                ))
                    .collect::<Vec<_>>()
                    .join(" | ")
            );
        }

        if !strict_target || selection.score >= STRICT_TARGET_SCORE {
            let page = pages
                .into_iter()
                .nth(selection.idx)
                .context("Chrome 页面选择失败")?;
            let selected_probe = selection.selected_probe();
            if selected_probe.map(|probe| probe.body_text_len == 0).unwrap_or(false) {
                warn!(
                    "已连接到 Chrome CDP（弱就绪页面），端口 {}，选中页面 idx={} url={} expected={} tie_break={} page_open_reason={}",
                    port, selection.idx, selection.url, expected_url, selection.tie_break, page_open_reason
                );
            } else {
                info!(
                    "已连接到 Chrome CDP，端口 {}，选中页面 idx={} url={} expected={} tie_break={} page_open_reason={}",
                    port, selection.idx, selection.url, expected_url, selection.tie_break, page_open_reason
                );
            }
            return Ok((browser, page));
        }

        if !redirected_existing_page && !expected_url.is_empty() {
            redirected_existing_page = true;
            if let Some(current_page) = pages.get(selection.idx) {
                match redirect_page_to_expected_url(current_page, expected_url).await {
                    Ok(redirect_marker) => {
                        page_open_reason = format!("redirect_existing_page:{}", redirect_marker);
                        info!(
                            "[CDP] 未命中目标页，先重定向现有页：idx={} from={} to={} page_open_reason={}",
                            selection.idx, selection.url, expected_url, page_open_reason
                        );
                        pages = wait_for_pages_or_target(
                            &browser,
                            &expected_host,
                            strict_target,
                            CDP_TARGET_RETRY_WAIT_SECS,
                        )
                        .await?;
                        continue;
                    }
                    Err(e) => {
                        page_open_reason = "redirect_existing_page:failed".to_string();
                        warn!(
                            "[CDP] 重定向现有页失败：idx={} from={} to={} page_open_reason={} error={}",
                            selection.idx, selection.url, expected_url, page_open_reason, e
                        );
                    }
                }
            } else {
                page_open_reason = "redirect_existing_page:page_index_miss".to_string();
            }
        }

        if !created_target_page {
            info!(
                "[CDP] 未命中目标 host={}，重定向后仍不匹配，尝试创建新页：{}（page_open_reason=create_new_page:after_redirect_or_miss）",
                expected_host, create_url
            );
            match browser.new_page(create_url.to_string()).await {
                Ok(_) => {
                    created_target_page = true;
                    page_open_reason = "create_new_page:after_redirect_or_miss".to_string();
                    pages = wait_for_pages_or_target(
                        &browser,
                        &expected_host,
                        strict_target,
                        CDP_TARGET_RETRY_WAIT_SECS,
                    )
                    .await?;
                    continue;
                }
                Err(e) => {
                    warn!(
                        "[CDP] 创建目标页失败（port={} host={} url={} page_open_reason=create_new_page:after_redirect_or_miss）：{}",
                        port, expected_host, create_url, e
                    );
                    bail!(
                        "TARGET_PAGE_NOT_FOUND: 创建目标页失败。port={} expected_url={} expected_host={} page_open_reason=create_new_page:after_redirect_or_miss error={}",
                        port,
                        expected_url,
                        expected_host,
                        e
                    );
                }
            }
        }

        if created_target_page {
            let observed_urls = selection
                .observed
                .iter()
                .map(|probe| probe.url.clone())
                .collect::<Vec<_>>()
                .join(" | ");
            bail!(
                "TARGET_PAGE_NOT_FOUND: 未定位到目标页面。port={} expected_url={} expected_host={} observed_pages={} page_open_reason={}",
                port,
                expected_url,
                expected_host,
                observed_urls,
                page_open_reason
            );
        }
    }
}

async fn redirect_page_to_expected_url(page: &Page, expected_url: &str) -> Result<String> {
    let target = serde_json::to_string(expected_url).unwrap_or_else(|_| "\"\"".to_string());
    let script = format!(
        r#"
        (function() {{
            const target = {};
            if (!target) return 'skip_empty';
            if ((window.location.href || '') === target) return 'already_target';
            try {{
                window.location.replace(target);
                return 'location.replace';
            }} catch (e) {{
                return 'error:' + String(e || '');
            }}
        }})()
        "#,
        target
    );

    let result: String = page
        .evaluate(script.as_str())
        .await
        .map(|v| v.into_value().unwrap_or_else(|_| "error:evaluate_failed".to_string()))
        .unwrap_or_else(|_| "error:evaluate_failed".to_string());

    if result.starts_with("error:") {
        bail!("{}", result);
    }

    Ok(result)
}

struct PageSelection {
    idx: usize,
    score: i32,
    url: String,
    tie_break: String,
    observed: Vec<PageProbe>,
}

#[derive(Clone)]
struct PageProbe {
    idx: usize,
    url: String,
    score: i32,
    exact_url_match: bool,
    ready_complete: bool,
    visible: bool,
    focused: bool,
    body_text_len: usize,
    title: String,
    body_excerpt: String,
}

impl PageSelection {
    fn selected_probe(&self) -> Option<&PageProbe> {
        self.observed.iter().find(|probe| probe.idx == self.idx)
    }
}

async fn wait_for_pages_or_target(
    browser: &Browser,
    expected_host: &str,
    strict_target: bool,
    timeout_secs: u64,
) -> Result<Vec<Page>> {
    let start = Instant::now();
    let timeout = Duration::from_secs(timeout_secs);
    let poll = Duration::from_millis(500);

    loop {
        let pages = browser.pages().await.context("获取页面列表失败")?;
        if !pages.is_empty()
            && (!strict_target || has_target_host_page(&pages, expected_host).await)
        {
            return Ok(pages);
        }

        if start.elapsed() > timeout {
            // 超时时仍返回当前页面列表（可能非空但不匹配目标 host）
            return Ok(pages);
        }

        tokio::time::sleep(poll).await;
    }
}

async fn has_target_host_page(pages: &[Page], expected_host: &str) -> bool {
    if expected_host.is_empty() {
        return true;
    }

    for page in pages {
        let url: String = page
            .evaluate("window.location.href")
            .await
            .map(|v| v.into_value().unwrap_or_else(|_| String::new()))
            .unwrap_or_default();
        if url.contains(expected_host) {
            return true;
        }
    }
    false
}

async fn select_best_page(
    pages: &[Page],
    expected_url: &str,
    expected_host: &str,
) -> PageSelection {
    let mut selected: Option<PageProbe> = None;
    let mut observed = Vec::with_capacity(pages.len());

    for (idx, page) in pages.iter().enumerate() {
        let probe = probe_page(page, idx, expected_url, expected_host).await;

        if selected
            .as_ref()
            .map(|current| page_probe_rank(&probe) > page_probe_rank(current))
            .unwrap_or(true)
        {
            selected = Some(PageProbe {
                idx: probe.idx,
                url: probe.url.clone(),
                score: probe.score,
                exact_url_match: probe.exact_url_match,
                ready_complete: probe.ready_complete,
                visible: probe.visible,
                focused: probe.focused,
                body_text_len: probe.body_text_len,
                title: probe.title.clone(),
                body_excerpt: probe.body_excerpt.clone(),
            });
        }
        observed.push(probe);
    }

    let selected = selected.unwrap_or_else(|| PageProbe {
        idx: 0,
        url: String::new(),
        score: i32::MIN,
        exact_url_match: false,
        ready_complete: false,
        visible: false,
        focused: false,
        body_text_len: 0,
        title: String::new(),
        body_excerpt: String::new(),
    });

    let tie_break = format!(
        "score={} exact_url_match={} ready_complete={} visible={} focused={} body_non_empty={} idx={} body_excerpt={}",
        selected.score,
        selected.exact_url_match,
        selected.ready_complete,
        selected.visible,
        selected.focused,
        selected.body_text_len > 0,
        selected.idx,
        selected.body_excerpt
    );

    PageSelection {
        idx: selected.idx,
        score: selected.score,
        url: selected.url,
        tie_break,
        observed,
    }
}

fn page_probe_rank(probe: &PageProbe) -> (i32, i32, i32, i32, i32, i32, usize) {
    (
        probe.score,
        bool_rank(probe.exact_url_match),
        bool_rank(probe.ready_complete),
        bool_rank(probe.visible),
        bool_rank(probe.focused),
        bool_rank(probe.body_text_len > 0),
        probe.idx,
    )
}

fn bool_rank(value: bool) -> i32 {
    if value { 1 } else { 0 }
}

async fn probe_page(page: &Page, idx: usize, expected_url: &str, expected_host: &str) -> PageProbe {
    let fallback_url: String = page
        .evaluate("window.location.href")
        .await
        .map(|v| v.into_value().unwrap_or_else(|_| String::new()))
        .unwrap_or_default();

    let probe_js = r#"
        (function() {
            const href = window.location.href || '';
            const title = (document.title || '').replace(/\s+/g, ' ').trim().slice(0, 80);
            const ready = document.readyState || '';
            const visible = (document.visibilityState || '') === 'visible';
            const focused = typeof document.hasFocus === 'function' ? !!document.hasFocus() : false;
            const bodyText = (document.body && document.body.innerText)
                ? document.body.innerText.replace(/\s+/g, ' ').trim()
                : '';
            return JSON.stringify({
                href,
                title,
                ready,
                visible,
                focused,
                body_text_len: bodyText.length,
                body_excerpt: bodyText.slice(0, 80)
            });
        })()
    "#;

    let raw: String = page
        .evaluate(probe_js)
        .await
        .map(|v| v.into_value().unwrap_or_else(|_| "{}".into()))
        .unwrap_or_else(|_| "{}".into());
    let parsed: serde_json::Value = serde_json::from_str(&raw).unwrap_or_else(|_| serde_json::json!({}));

    let mut url = parsed
        .get("href")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if url.is_empty() {
        url = fallback_url;
    }

    let title = parsed
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let ready_state = parsed
        .get("ready")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let body_text_len = parsed
        .get("body_text_len")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    let body_excerpt = parsed
        .get("body_excerpt")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let score = score_url_match(&url, expected_url, expected_host);

    PageProbe {
        idx,
        exact_url_match: !expected_url.is_empty() && url == expected_url,
        ready_complete: ready_state == "complete",
        visible: parsed
            .get("visible")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        focused: parsed
            .get("focused")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        body_text_len,
        score,
        url,
        title,
        body_excerpt,
    }
}

/// Backward-compatible upload entry, now using strategy-based uploader.
pub async fn upload_file(page: &Page, file_path: &str) -> Result<()> {
    let opts = UploadOptions {
        platform: "generic",
        candidate_selectors: vec!["input[type='file']"],
        success_timeout_secs: 8,
        attempt_timeout_secs: 3,
    };
    upload_file_with_strategies(page, file_path, opts).await?;
    Ok(())
}

/// Upload file with ordered selector strategies and platform-aware start-signal checks.
pub async fn upload_file_with_strategies(
    page: &Page,
    file_path: &str,
    opts: UploadOptions,
) -> Result<UploadAttemptReport> {
    if opts.candidate_selectors.is_empty() {
        bail!("平台 {} 未配置上传选择器", opts.platform);
    }

    let start_url = current_url(page).await;
    let global_start = Instant::now();
    let mut attempted_selectors: Vec<String> = Vec::new();

    for (idx, selector) in opts.candidate_selectors.iter().enumerate() {
        if global_start.elapsed() > Duration::from_secs(opts.success_timeout_secs) {
            attempted_selectors.push(format!(
                "selector={} skipped(global_timeout={}s)",
                selector, opts.success_timeout_secs
            ));
            break;
        }

        let count = selector_match_count(page, selector).await;
        if count <= 0 {
            let result = format!("selector={} miss(count=0)", selector);
            info!("[{}-upload] attempt={} {}", opts.platform, idx + 1, result);
            attempted_selectors.push(result);
            continue;
        }

        match set_file_input(page, selector, file_path).await {
            Ok(()) => {
                info!(
                    "[{}-upload] attempt={} selector={} set_file_ok",
                    opts.platform,
                    idx + 1,
                    selector
                );
            }
            Err(e) => {
                let result = format!("selector={} set_failed:{}", selector, e);
                warn!("[{}-upload] attempt={} {}", opts.platform, idx + 1, result);
                attempted_selectors.push(result);
                continue;
            }
        }

        let signal = wait_for_upload_start_signal(
            page,
            opts.platform,
            opts.attempt_timeout_secs,
            Duration::from_millis(500),
        )
        .await;

        if let Some(signal) = signal {
            let end_url = current_url(page).await;
            info!(
                "[{}-upload] started via selector={} signal={}",
                opts.platform, selector, signal
            );
            return Ok(UploadAttemptReport {
                selected_selector: (*selector).to_string(),
                attempted_selectors,
                start_url,
                end_url,
                detected_signal: signal,
                elapsed_ms: global_start.elapsed().as_millis(),
            });
        }

        let result = format!(
            "selector={} no_signal(timeout={}s)",
            selector, opts.attempt_timeout_secs
        );
        info!("[{}-upload] attempt={} {}", opts.platform, idx + 1, result);
        attempted_selectors.push(result);
    }

    let current = current_url(page).await;
    let selector_counts = gather_selector_counts(page, &opts.candidate_selectors).await;
    let input_summary = gather_file_inputs_summary(page).await;

    bail!(
        "平台 {} 未检测到上传开始信号。当前URL={} 选择器匹配={} 尝试记录={} 文件输入={}",
        opts.platform,
        current,
        selector_counts,
        attempted_selectors.join(" | "),
        input_summary,
    );
}

pub async fn wait_for_upload_start_signal(
    page: &Page,
    platform: &str,
    timeout_secs: u64,
    poll_every: Duration,
) -> Option<String> {
    let start = Instant::now();
    while start.elapsed() <= Duration::from_secs(timeout_secs) {
        if let Some(signal) = detect_upload_start_signal(page, platform).await {
            return Some(signal);
        }
        tokio::time::sleep(poll_every).await;
    }
    None
}

async fn detect_upload_start_signal(page: &Page, platform: &str) -> Option<String> {
    let js = match platform {
        "douyin" => {
            r#"
            (function() {
                const href = window.location.href || '';
                if (href.includes('/creator-micro/content/post/video')) {
                    return 'url:post_video';
                }

                const fileInputs = Array.from(document.querySelectorAll('input[type="file"]'));
                for (const input of fileInputs) {
                    if (input && input.files && input.files.length > 0) {
                        return 'file:selected:' + input.files.length;
                    }
                }

                const progress = document.querySelector('[class*="progress"], .progress-div, [class*="upload-progress"], [class*="percent"]');
                if (progress) {
                    const text = ((progress.textContent || '').trim().replace(/\s+/g, ' ')).slice(0, 60);
                    return text ? ('progress:' + text) : 'progress:visible';
                }

                const pageText = (document.body && document.body.innerText) ? document.body.innerText : '';
                if (pageText.includes('上传中') || pageText.includes('处理中') || pageText.includes('转码中') || pageText.includes('校验中')) {
                    return 'text:uploading';
                }
                if (pageText.includes('重新上传') || pageText.includes('更换视频')) {
                    return 'text:replace-video';
                }
                return '';
            })()
            "#
        }
        "xiaohongshu" => {
            r#"
            (function() {
                const href = window.location.href || '';
                if (href.includes('/publish/post') || href.includes('/publish/edit') || href.includes('/publish/success')) {
                    return 'url:post';
                }

                const fileInputs = Array.from(document.querySelectorAll('input[type="file"]'));
                for (const input of fileInputs) {
                    if (input && input.files && input.files.length > 0) {
                        return 'file:selected:' + input.files.length;
                    }
                }

                const progress = document.querySelector('[class*="progress"], [class*="upload-progress"], [class*="percent"], [class*="loading"]');
                if (progress) {
                    const text = ((progress.textContent || '').trim().replace(/\s+/g, ' ')).slice(0, 60);
                    return text ? ('progress:' + text) : 'progress:visible';
                }

                const pageText = (document.body && document.body.innerText) ? document.body.innerText : '';
                if (pageText.includes('上传中') || pageText.includes('处理中') || pageText.includes('发布中') || pageText.includes('正在上传')) {
                    return 'text:uploading';
                }
                if (pageText.includes('重新上传') || pageText.includes('更换视频')) {
                    return 'text:replace-video';
                }
                return '';
            })()
            "#
        }
        "bilibili" => {
            r#"
            (function() {
                const href = window.location.href || '';
                if (href.includes('/upload-manager') || href.includes('/video/edit') || href.includes('/archive')) {
                    return 'url:post';
                }

                const fileInputs = Array.from(document.querySelectorAll('input[type="file"]'));
                for (const input of fileInputs) {
                    if (input && input.files && input.files.length > 0) {
                        return 'file:selected:' + input.files.length;
                    }
                }

                const progress = document.querySelector('[class*="progress"], [class*="upload-progress"], [class*="percent"], [class*="uploading"]');
                if (progress) {
                    const text = ((progress.textContent || '').trim().replace(/\s+/g, ' ')).slice(0, 60);
                    return text ? ('progress:' + text) : 'progress:visible';
                }

                const pageText = (document.body && document.body.innerText) ? document.body.innerText : '';
                if (pageText.includes('上传中') || pageText.includes('处理中') || pageText.includes('转码中') || pageText.includes('投稿')) {
                    return 'text:uploading';
                }
                return '';
            })()
            "#
        }
        "wechat" => {
            r#"
            (function() {
                const normalize = (value) => (value || '').replace(/\s+/g, ' ').trim();
                const progressSelectors = [
                    "[class*='upload-progress']",
                    "[class*='uploader']",
                    "[class*='uploading']",
                    "[class*='progress']",
                    "[class*='percent']",
                    "[class*='loading']"
                ];
                const uploadTextMarkers = ['上传中', '处理中', '校验中', '转码中', '发布中', '正在上传', '重新上传', '更换视频'];
                const initTextMarkers = ['页面初始化中', '初始化中', '正在初始化'];
                const loginTextMarkers = ['扫码登录', '微信扫码', '请使用微信扫码登录', '请在手机上确认登录'];
                const maxFrameDepth = 3;
                const maxShadowDepth = 4;

                function collectFrames(doc, path, depth, result) {
                    result.push({ doc, path: 'frame:' + path });
                    if (depth >= maxFrameDepth) return;

                    let iframes = [];
                    try {
                        iframes = Array.from(doc.querySelectorAll('iframe'));
                    } catch (_) {
                        iframes = [];
                    }

                    for (let i = 0; i < iframes.length; i += 1) {
                        let childDoc = null;
                        try {
                            childDoc = iframes[i].contentDocument;
                        } catch (_) {
                            childDoc = null;
                        }
                        if (!childDoc) continue;
                        collectFrames(childDoc, path + '/' + i, depth + 1, result);
                    }
                }

                function collectRoots(root, framePath, shadowPath, depth, roots) {
                    const context = shadowPath ? (framePath + '|' + shadowPath) : framePath;
                    roots.push({ root, context });
                    if (depth >= maxShadowDepth) return;

                    let nodes = [];
                    try {
                        nodes = typeof root.querySelectorAll === 'function'
                            ? Array.from(root.querySelectorAll('*'))
                            : [];
                    } catch (_) {
                        nodes = [];
                    }

                    for (let i = 0; i < nodes.length; i += 1) {
                        const el = nodes[i];
                        if (!el || !el.shadowRoot) continue;
                        const tag = (el.tagName || 'shadow').toLowerCase();
                        const nextShadowPath = shadowPath
                            ? (shadowPath + '/shadow:' + tag + '[' + i + ']')
                            : ('shadow:' + tag + '[' + i + ']');
                        collectRoots(el.shadowRoot, framePath, nextShadowPath, depth + 1, roots);
                    }
                }

                function probeContext(rootCtx) {
                    let fileInputs = [];
                    try {
                        fileInputs = Array.from(rootCtx.root.querySelectorAll("input[type='file']"));
                    } catch (_) {
                        fileInputs = [];
                    }
                    for (const input of fileInputs) {
                        if (input && input.files && input.files.length > 0) {
                            return 'file:selected:' + input.files.length + '@' + rootCtx.context;
                        }
                    }

                    let contextText = '';
                    try {
                        contextText = rootCtx.root.body
                            ? (rootCtx.root.body.innerText || '')
                            : (rootCtx.root.textContent || '');
                    } catch (_) {
                        contextText = '';
                    }
                    const normalized = normalize(contextText);

                    for (const marker of initTextMarkers) {
                        if (marker && normalized.includes(marker)) {
                            return '';
                        }
                    }
                    for (const marker of loginTextMarkers) {
                        if (marker && normalized.includes(marker)) {
                            return '';
                        }
                    }

                    for (const selector of progressSelectors) {
                        let node = null;
                        try {
                            node = rootCtx.root.querySelector(selector);
                        } catch (_) {
                            node = null;
                        }
                        if (!node) continue;
                        const text = normalize(node.textContent || '').slice(0, 60);
                        if (text && uploadTextMarkers.some((marker) => text.includes(marker))) {
                            return ('progress:' + text) + '@' + rootCtx.context;
                        }
                    }

                    for (const marker of uploadTextMarkers) {
                        if (marker && normalized.includes(marker)) {
                            return 'text:uploading@' + rootCtx.context;
                        }
                    }
                    return '';
                }

                const frames = [];
                collectFrames(document, 'top', 0, frames);
                for (const frameCtx of frames) {
                    const roots = [];
                    collectRoots(frameCtx.doc, frameCtx.path, '', 0, roots);
                    for (const rootCtx of roots) {
                        const hit = probeContext(rootCtx);
                        if (hit) return hit;
                    }
                }
                return '';
            })()
            "#
        }
        "youtube" => {
            r#"
            (function() {
                const href = window.location.href || '';
                if (href.includes('studio.youtube.com') && (href.includes('/upload') || href.includes('uploading') || href.includes('video_id='))) {
                    return 'url:upload';
                }

                const fileInputs = Array.from(document.querySelectorAll('input[type="file"]'));
                for (const input of fileInputs) {
                    if (input && input.files && input.files.length > 0) {
                        return 'file:selected:' + input.files.length;
                    }
                }

                const progress = document.querySelector('ytcp-video-upload-progress, ytcp-upload-progress, ytcp-uploads-dialog, [id*="progress"], [class*="progress"], [class*="upload-progress"]');
                if (progress) {
                    const text = ((progress.textContent || '').trim().replace(/\s+/g, ' ')).slice(0, 80);
                    return text ? ('progress:' + text) : 'progress:visible';
                }

                const pageText = (document.body && document.body.innerText) ? document.body.innerText : '';
                if (pageText.includes('Uploading') || pageText.includes('Processing') || pageText.includes('Checking') || pageText.includes('Checks complete') || pageText.includes('Upload complete') || pageText.includes('上传中') || pageText.includes('处理中')) {
                    return 'text:uploading';
                }
                return '';
            })()
            "#
        }
        _ => {
            r#"
            (function() {
                const input = document.querySelector('input[type="file"]');
                if (input && input.files && input.files.length > 0) {
                    return 'file:selected';
                }
                return '';
            })()
            "#
        }
    };

    let signal: String = page
        .evaluate(js)
        .await
        .map(|v| v.into_value().unwrap_or_else(|_| String::new()))
        .unwrap_or_default();

    if signal.is_empty() {
        None
    } else {
        Some(signal)
    }
}

fn extract_host(url: &str) -> String {
    url.split("//")
        .nth(1)
        .and_then(|rest| rest.split('/').next())
        .unwrap_or("")
        .to_string()
}

fn score_url_match(url: &str, expected_url: &str, expected_host: &str) -> i32 {
    if !expected_url.is_empty() && url == expected_url {
        return 100;
    }
    if !expected_url.is_empty() && url.contains(expected_url) {
        return 90;
    }
    if !expected_host.is_empty() && url.contains(expected_host) {
        return 70;
    }
    if url.starts_with("http://") || url.starts_with("https://") {
        return 10;
    }
    0
}

pub async fn set_file_input(page: &Page, selector: &str, file_path: &str) -> Result<()> {
    // Keep inputs interactable in case site toggles hidden state.
    let make_visible_js = format!(
        r#"
        (function() {{
            const nodes = document.querySelectorAll('{}');
            nodes.forEach((input) => {{
                if (input && input.style) {{
                    input.style.display = 'block';
                    input.style.opacity = '1';
                    input.style.visibility = 'visible';
                }}
            }});
            return nodes.length;
        }})()
        "#,
        escape_js_single(selector)
    );
    page.evaluate(make_visible_js.as_str()).await.ok();

    let doc = page
        .execute(GetDocumentParams::builder().depth(0).build())
        .await
        .context("获取文档失败")?;
    let root_node_id = doc.result.root.node_id;

    let query = QuerySelectorParams::new(root_node_id, selector);
    let query_result = page
        .execute(query)
        .await
        .with_context(|| format!("查询选择器 {} 失败", selector))?;

    let node_id = query_result.result.node_id;

    let mut set_files = SetFileInputFilesParams::new(vec![file_path.to_string()]);
    set_files.node_id = Some(node_id);
    page.execute(set_files)
        .await
        .with_context(|| format!("通过 CDP 设置文件失败，选择器 {}", selector))?;

    Ok(())
}

async fn current_url(page: &Page) -> String {
    page.evaluate("window.location.href")
        .await
        .map(|v| v.into_value().unwrap_or_else(|_| String::new()))
        .unwrap_or_default()
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

async fn gather_selector_counts(page: &Page, selectors: &[&'static str]) -> String {
    let mut parts = Vec::with_capacity(selectors.len());
    for selector in selectors {
        let count = selector_match_count(page, selector).await;
        parts.push(format!("{}:{}", selector, count));
    }
    parts.join(",")
}

async fn gather_file_inputs_summary(page: &Page) -> String {
    let js = r#"
        (function() {
            const inputs = Array.from(document.querySelectorAll('input[type="file"]')).slice(0, 3);
            return JSON.stringify(inputs.map((el) => ({
                className: el.className || '',
                id: el.id || '',
                accept: el.getAttribute('accept') || '',
                style: el.getAttribute('style') || '',
            })));
        })()
    "#;

    page.evaluate(js)
        .await
        .map(|v| v.into_value().unwrap_or_else(|_| "[]".to_string()))
        .unwrap_or_else(|_| "[]".to_string())
}

pub async fn fill_text_input(
    page: &Page,
    value: &str,
    selectors: &[&str],
    editable_selector: Option<&str>,
) -> Result<String> {
    if value.trim().is_empty() {
        return Ok("skipped_empty".to_string());
    }

    let selectors_js = js_string_array(selectors);
    let value_json = serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string());
    let editable_json =
        serde_json::to_string(&editable_selector.unwrap_or("")).unwrap_or_else(|_| "\"\"".into());

    let script = format!(
        r#"
        (function() {{
            const value = {};
            const selectors = [{}];
            for (const sel of selectors) {{
                let el = null;
                try {{
                    el = document.querySelector(sel);
                }} catch (_) {{
                    el = null;
                }}
                if (!el) continue;

                if (typeof el.focus === 'function') el.focus();
                if ('value' in el) {{
                    el.value = value;
                }} else {{
                    el.textContent = value;
                }}
                el.dispatchEvent(new Event('input', {{ bubbles: true }}));
                el.dispatchEvent(new Event('change', {{ bubbles: true }}));
                return 'input:' + sel;
            }}

            const editableSelector = {};
            const editableNodes = editableSelector
                ? document.querySelectorAll(editableSelector)
                : document.querySelectorAll('[contenteditable=\"true\"]');
            for (const el of editableNodes) {{
                const rect = el.getBoundingClientRect();
                if (!rect || rect.width < 10 || rect.height < 10) continue;
                if (typeof el.focus === 'function') el.focus();
                el.textContent = value;
                el.dispatchEvent(new Event('input', {{ bubbles: true }}));
                el.dispatchEvent(new Event('change', {{ bubbles: true }}));
                return 'editable';
            }}

            return 'not_found';
        }})()
        "#,
        value_json, selectors_js, editable_json
    );

    let result: String = page
        .evaluate(script.as_str())
        .await
        .map(|v| v.into_value().unwrap_or_else(|_| "error".to_string()))
        .unwrap_or_else(|_| "error".to_string());

    Ok(result)
}

pub async fn add_tags_via_input(page: &Page, tags: &[String], selectors: &[&str]) -> Result<usize> {
    if tags.is_empty() || selectors.is_empty() {
        return Ok(0);
    }

    let selectors_js = js_string_array(selectors);
    let tags_json = serde_json::to_string(tags).unwrap_or_else(|_| "[]".to_string());
    let script = format!(
        r#"
        (function() {{
            const selectors = [{}];
            const tags = {};
            let added = 0;

            for (const rawTag of tags) {{
                const tag = (rawTag || '').trim();
                if (!tag) continue;

                let target = null;
                for (const sel of selectors) {{
                    try {{
                        target = document.querySelector(sel);
                    }} catch (_) {{
                        target = null;
                    }}
                    if (target) break;
                }}

                if (!target) continue;
                if (typeof target.focus === 'function') target.focus();

                if ('value' in target) {{
                    target.value = tag;
                }} else {{
                    target.textContent = tag;
                }}
                target.dispatchEvent(new Event('input', {{ bubbles: true }}));
                target.dispatchEvent(new Event('change', {{ bubbles: true }}));
                target.dispatchEvent(
                    new KeyboardEvent('keydown', {{
                        key: 'Enter',
                        code: 'Enter',
                        keyCode: 13,
                        which: 13,
                        bubbles: true
                    }})
                );
                target.dispatchEvent(
                    new KeyboardEvent('keyup', {{
                        key: 'Enter',
                        code: 'Enter',
                        keyCode: 13,
                        which: 13,
                        bubbles: true
                    }})
                );
                added += 1;
            }}
            return added;
        }})()
        "#,
        selectors_js, tags_json
    );

    let added: i64 = page
        .evaluate(script.as_str())
        .await
        .map(|v| v.into_value().unwrap_or(0))
        .unwrap_or(0);

    Ok(added.max(0) as usize)
}

fn js_string_array(values: &[&str]) -> String {
    values
        .iter()
        .map(|value| format!("'{}'", escape_js_single(value)))
        .collect::<Vec<_>>()
        .join(",")
}

fn escape_js_single(input: &str) -> String {
    input.replace('\\', "\\\\").replace('\'', "\\'")
}

/// 执行 JavaScript 并返回字符串结果
pub async fn execute_js(page: &Page, script: &str) -> Result<String> {
    let result = page
        .evaluate(script)
        .await
        .context("执行 JavaScript 失败")?;
    Ok(format!("{:?}", result.value()))
}

/// Wait a specified duration
pub async fn wait(secs: f64) {
    tokio::time::sleep(Duration::from_secs_f64(secs)).await;
}

async fn disable_file_chooser_intercept(page: &Page) {
    let _ = page
        .execute(SetInterceptFileChooserDialogParams { enabled: false })
        .await;
}

#[derive(Clone, Debug)]
struct GeometryClickCandidate {
    x: f64,
    y: f64,
    score: f64,
    context: String,
    frame_path: String,
    reason: String,
}

fn parse_geometry_click_candidates(raw: &str) -> Vec<GeometryClickCandidate> {
    let parsed: serde_json::Value = serde_json::from_str(raw).unwrap_or_else(|_| serde_json::json!([]));
    let Some(items) = parsed.as_array() else {
        return Vec::new();
    };

    let mut candidates = Vec::new();
    for item in items.iter().take(3) {
        let Some(x) = item.get("x").and_then(|v| v.as_f64()) else {
            continue;
        };
        let Some(y) = item.get("y").and_then(|v| v.as_f64()) else {
            continue;
        };
        let score = item.get("score").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let context = item
            .get("context")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let frame_path = item
            .get("frame_path")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let reason = item
            .get("reason")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        candidates.push(GeometryClickCandidate {
            x,
            y,
            score,
            context,
            frame_path,
            reason,
        });
    }
    candidates
}

fn build_wechat_retry_candidates(
    click_x: Option<f64>,
    click_y: Option<f64>,
    clicked_context: &str,
    frame_path: &str,
    geometry_candidates: &[GeometryClickCandidate],
) -> Vec<GeometryClickCandidate> {
    let mut retry_candidates = Vec::new();
    if let (Some(x), Some(y)) = (click_x, click_y) {
        retry_candidates.push(GeometryClickCandidate {
            x,
            y,
            score: 0.0,
            context: clicked_context.to_string(),
            frame_path: frame_path.to_string(),
            reason: "selected_point".to_string(),
        });
    }
    for candidate in geometry_candidates {
        let duplicated = retry_candidates.iter().any(|existing| {
            (existing.x - candidate.x).abs() < 1.0 && (existing.y - candidate.y).abs() < 1.0
        });
        if duplicated {
            continue;
        }
        retry_candidates.push(candidate.clone());
    }
    retry_candidates
}

async fn cdp_mouse_left_click(page: &Page, x: f64, y: f64) -> Result<()> {
    page.execute(DispatchMouseEventParams::new(
        DispatchMouseEventType::MouseMoved,
        x,
        y,
    ))
    .await
    .context("[CDP鼠标点击] 发送 mouseMoved 失败")?;

    let mut pressed = DispatchMouseEventParams::new(DispatchMouseEventType::MousePressed, x, y);
    pressed.button = Some(MouseButton::Left);
    pressed.buttons = Some(1);
    pressed.click_count = Some(1);
    page.execute(pressed)
        .await
        .context("[CDP鼠标点击] 发送 mousePressed 失败")?;

    let mut released = DispatchMouseEventParams::new(DispatchMouseEventType::MouseReleased, x, y);
    released.button = Some(MouseButton::Left);
    released.buttons = Some(0);
    released.click_count = Some(1);
    page.execute(released)
        .await
        .context("[CDP鼠标点击] 发送 mouseReleased 失败")?;

    Ok(())
}

async fn js_click_geometry_candidate(
    page: &Page,
    frame_path: &str,
    x: f64,
    y: f64,
) -> Result<String> {
    let frame_path_json = serde_json::to_string(frame_path).unwrap_or_else(|_| "\"top\"".to_string());
    let js = format!(
        r#"
        (function() {{
            const framePath = {};
            const x = {};
            const y = {};

            function resolveDoc(path) {{
                let doc = document;
                if (!path || path === 'top') return doc;
                const parts = String(path).split('/').slice(1);
                for (const raw of parts) {{
                    const idx = Number(raw);
                    if (!Number.isFinite(idx)) return null;
                    let iframes = [];
                    try {{
                        iframes = Array.from(doc.querySelectorAll('iframe'));
                    }} catch (_) {{
                        return null;
                    }}
                    const frame = iframes[idx];
                    if (!frame || !frame.contentDocument) return null;
                    doc = frame.contentDocument;
                }}
                return doc;
            }}

            function elementFromPointDeep(root, px, py, depth) {{
                if (!root || depth > 6) return null;
                let el = null;
                try {{
                    el = root.elementFromPoint(px, py);
                }} catch (_) {{
                    el = null;
                }}
                if (!el) return null;
                if (el.shadowRoot) {{
                    const inner = elementFromPointDeep(el.shadowRoot, px, py, depth + 1);
                    if (inner) return inner;
                }}
                return el;
            }}

            function isVisible(el) {{
                if (!el) return false;
                const rect = el.getBoundingClientRect();
                const style = window.getComputedStyle(el);
                return !!rect
                    && rect.width >= 6
                    && rect.height >= 6
                    && style
                    && style.visibility !== 'hidden'
                    && style.display !== 'none';
            }}

            function isClickable(el) {{
                if (!el) return false;
                const tag = (el.tagName || '').toLowerCase();
                if (tag === 'button' || tag === 'input' || tag === 'label' || tag === 'a') return true;
                const role = (el.getAttribute('role') || '').toLowerCase();
                if (role === 'button') return true;
                const tabindex = el.getAttribute('tabindex');
                if (tabindex !== null && tabindex !== '-1') return true;
                if (typeof el.onclick === 'function' || el.hasAttribute('onclick')) return true;
                const style = window.getComputedStyle(el);
                return !!style && style.cursor === 'pointer';
            }}

            function findClickableAncestor(node) {{
                let current = node;
                for (let depth = 0; current && depth < 8; depth += 1) {{
                    if (isClickable(current) && isVisible(current)) return current;
                    if (current.parentElement) {{
                        current = current.parentElement;
                        continue;
                    }}
                    const root = typeof current.getRootNode === 'function' ? current.getRootNode() : null;
                    current = root && root.host ? root.host : null;
                }}
                return node;
            }}

            function clickChain(el) {{
                const steps = [];
                const events = [
                    ['pointerdown', () => new PointerEvent('pointerdown', {{ bubbles: true, cancelable: true, composed: true, pointerType: 'mouse', isPrimary: true }})],
                    ['mousedown', () => new MouseEvent('mousedown', {{ bubbles: true, cancelable: true, composed: true, button: 0 }})],
                    ['mouseup', () => new MouseEvent('mouseup', {{ bubbles: true, cancelable: true, composed: true, button: 0 }})],
                    ['click', () => new MouseEvent('click', {{ bubbles: true, cancelable: true, composed: true, button: 0 }})],
                ];
                for (const [name, build] of events) {{
                    try {{
                        el.dispatchEvent(build());
                        steps.push(name);
                    }} catch (_) {{}}
                }}
                try {{
                    el.click();
                    steps.push('el.click()');
                }} catch (_) {{}}
                return steps.join('>');
            }}

            const doc = resolveDoc(framePath);
            if (!doc) return 'frame_not_found';
            const el = elementFromPointDeep(doc, x, y, 0);
            if (!el) return 'point_miss';
            const clickable = findClickableAncestor(el);
            if (!clickable || !isVisible(clickable)) return 'not_clickable';
            const chain = clickChain(clickable);
            return chain || 'clicked';
        }})()
        "#,
        frame_path_json, x, y
    );

    let result: String = page
        .evaluate(js.as_str())
        .await
        .map(|v| v.into_value().unwrap_or_else(|_| "error".to_string()))
        .unwrap_or_else(|_| "error".to_string());
    Ok(result)
}

/// 通过拦截浏览器原生文件选择对话框来上传文件。
///
/// 流程：
/// 1. 启用文件选择器拦截（Page.setInterceptFileChooserDialog）
/// 2. 点击 <input type="file"> 元素触发原生文件对话框
/// 3. 浏览器触发 EventFileChooserOpened 事件，携带 backend_node_id
/// 4. 用 backend_node_id 调用 DOM.setFileInputFiles 设置文件
///
/// 这样 Chrome 会触发真正的原生 change/input 事件，React 应用（如抖音）可以检测到。
pub async fn upload_file_via_file_chooser(
    page: &Page,
    file_path: &str,
    input_selector: &str,
) -> Result<()> {
    info!(
        "[文件选择器] 开始文件选择器上传：选择器={} 文件={}",
        input_selector, file_path
    );

    // 第1步：启用文件选择器拦截
    let intercept_params = SetInterceptFileChooserDialogParams { enabled: true };
    page.execute(intercept_params)
        .await
        .context("[文件选择器] 启用文件选择器拦截失败")?;
    info!("[文件选择器] 文件选择器拦截已启用");

    // 第2步：在点击之前开始监听 EventFileChooserOpened 事件
    let mut event_stream = page
        .event_listener::<EventFileChooserOpened>()
        .await
        .context("[文件选择器] 创建事件监听器失败")?;

    // 第3步：点击 input 元素触发原生文件对话框
    // 需要先让它可见、可点击
    let make_clickable_js = format!(
        r#"
        (function() {{
            const input = document.querySelector('{}');
            if (!input) return 'not_found';
            input.style.display = 'block';
            input.style.opacity = '1';
            input.style.visibility = 'visible';
            input.style.position = 'absolute';
            input.style.width = '200px';
            input.style.height = '200px';
            input.style.zIndex = '99999';
            input.style.top = '0';
            input.style.left = '0';
            input.click();
            return 'clicked';
        }})()"#,
        escape_js_single(input_selector)
    );
    let click_result: String = page
        .evaluate(make_clickable_js.as_str())
        .await
        .map(|v| v.into_value().unwrap_or_else(|_| "error".into()))
        .unwrap_or_else(|_| "error".into());
    info!("[文件选择器] 点击结果：{}", click_result);

    if click_result == "not_found" {
        disable_file_chooser_intercept(page).await;
        bail!("[文件选择器] 找不到 input 元素：{}", input_selector);
    }

    // 第4步：等待 EventFileChooserOpened 事件（带超时）
    info!("[文件选择器] 等待文件选择器打开事件...");
    let event = tokio::time::timeout(Duration::from_secs(5), event_stream.next()).await;

    let backend_node_id: Option<BackendNodeId> = match event {
        Ok(Some(evt)) => {
            info!(
                "[文件选择器] 收到文件选择器打开事件：mode={:?}, backend_node_id={:?}",
                evt.mode, evt.backend_node_id
            );
            evt.backend_node_id
        }
        Ok(None) => {
            warn!("[文件选择器] 事件流结束，未收到事件");
            None
        }
        Err(_) => {
            warn!("[文件选择器] 等待文件选择器事件超时");
            None
        }
    };

    // 第5步：用 backend_node_id 设置文件（会触发真正的原生事件）
    let mut set_files = SetFileInputFilesParams::new(vec![file_path.to_string()]);
    if let Some(bn_id) = backend_node_id {
        set_files.backend_node_id = Some(bn_id);
        info!(
            "[文件选择器] 使用 backend_node_id：{:?}",
            set_files.backend_node_id
        );
    } else {
        // 降级方案：通过选择器查询节点
        info!("[文件选择器] 没有 backend_node_id，降级使用 querySelector");
        let doc = page
            .execute(GetDocumentParams::builder().depth(0).build())
            .await
            .context("[文件选择器] 获取文档失败")?;
        let root_node_id = doc.result.root.node_id;
        let query = QuerySelectorParams::new(root_node_id, input_selector);
        let query_result = page
            .execute(query)
            .await
            .context("[文件选择器] 查询选择器失败")?;
        if *query_result.result.node_id.inner() <= 0 {
            disable_file_chooser_intercept(page).await;
            bail!(
                "[文件选择器] 通过选择器查询到的节点无效：{}",
                input_selector
            );
        }
        set_files.node_id = Some(query_result.result.node_id);
    }

    let set_result = page
        .execute(set_files)
        .await
        .context("[文件选择器] 通过 CDP 设置文件失败");
    disable_file_chooser_intercept(page).await;
    set_result?;
    info!("[文件选择器] 文件设置成功");

    info!("[文件选择器] 文件选择器拦截已关闭");

    Ok(())
}

/// 点击上传按钮触发文件选择器，再使用 backend_node_id 设置文件。
/// 适用于页面把 input[type=file] 隐藏在复杂组件内、无法稳定直接选中 input 的场景。
pub async fn click_first_visible(page: &Page, selectors: &[&str]) -> Result<String> {
    if selectors.is_empty() {
        bail!("[点击预处理] 选择器列表为空");
    }

    let selector_array = js_string_array(selectors);
    let click_js = format!(
        r#"
        (function() {{
            const selectors = [{}];
            const selectorHits = [];
            for (const sel of selectors) {{
                let nodes = [];
                try {{
                    nodes = Array.from(document.querySelectorAll(sel));
                }} catch (_) {{
                    selectorHits.push(sel + ':ERR');
                    continue;
                }}
                selectorHits.push(sel + ':' + nodes.length);
                for (const el of nodes) {{
                    const rect = el.getBoundingClientRect();
                    const style = window.getComputedStyle(el);
                    const visible = !!rect
                        && rect.width >= 6
                        && rect.height >= 6
                        && style
                        && style.visibility !== 'hidden'
                        && style.display !== 'none';
                    if (!visible) continue;
                    try {{
                        el.click();
                        return JSON.stringify({{
                            status: 'clicked',
                            marker: 'selector:' + sel,
                            selector_hits: selectorHits.join(',')
                        }});
                    }} catch (e) {{
                        return JSON.stringify({{
                            status: 'error',
                            marker: 'selector:' + sel,
                            selector_hits: selectorHits.join(','),
                            error: String(e || '')
                        }});
                    }}
                }}
            }}
            return JSON.stringify({{
                status: 'not_found',
                marker: 'not_found',
                selector_hits: selectorHits.join(',')
            }});
        }})()
        "#,
        selector_array
    );

    let probe_json: String = page
        .evaluate(click_js.as_str())
        .await
        .map(|v| v.into_value().unwrap_or_else(|_| "{}".into()))
        .unwrap_or_else(|_| "{}".into());
    let parsed: serde_json::Value = serde_json::from_str(&probe_json).unwrap_or_else(|_| {
        serde_json::json!({
            "status": "error",
            "marker": "parse_error",
            "selector_hits": ""
        })
    });

    let status = parsed
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("error");
    let marker = parsed
        .get("marker")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let selector_hits = parsed
        .get("selector_hits")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if status == "clicked" {
        return Ok(marker.to_string());
    }

    let current = current_url(page).await;
    bail!(
        "[点击预处理] 未命中可点击入口（status={} marker={} selector_hits={} current_url={}）",
        status,
        marker,
        selector_hits,
        current
    );
}

/// 点击上传按钮触发文件选择器，再使用 backend_node_id 设置文件。
/// 适用于页面把 input[type=file] 隐藏在复杂组件内、无法稳定直接选中 input 的场景。
pub async fn upload_file_via_click_to_open_file_chooser(
    page: &Page,
    file_path: &str,
    platform: &str,
    click_selectors: &[&str],
    click_text_markers: &[&str],
) -> Result<ClickChooserUploadResult> {
    info!(
        "[文件选择器-点击触发] 开始：platform={} selectors={} text_markers={} file={}",
        platform,
        click_selectors.join(", "),
        click_text_markers.join(", "),
        file_path
    );

    page.execute(SetInterceptFileChooserDialogParams { enabled: true })
        .await
        .context("[文件选择器-点击触发] 启用文件选择器拦截失败")?;

    let mut event_stream = page
        .event_listener::<EventFileChooserOpened>()
        .await
        .context("[文件选择器-点击触发] 创建事件监听器失败")?;

    let selector_json = serde_json::to_string(click_selectors).unwrap_or_else(|_| "[]".to_string());
    let marker_json =
        serde_json::to_string(click_text_markers).unwrap_or_else(|_| "[]".to_string());

    let click_js = if platform == "wechat" {
        r#"
        (function() {
            const selectors = __SELECTORS__;
            const textMarkers = __MARKERS__;
            const selectorHits = [];
            const candidateSummary = [];
            const geometryTopSummary = [];
            const normalize = (value) => (value || '').replace(/\s+/g, ' ').trim();
            const markers = textMarkers
                .map((m) => normalize(m).toLowerCase())
                .filter(Boolean);
            const blockedWords = ['暂时无法使用该功能了', '页面加载失败', '请稍后再试', '网络异常'];
            const initWords = ['页面初始化中', '初始化中', '正在初始化'];
            const loginWords = ['扫码登录', '微信扫码', '请使用微信扫码登录', '请在手机上确认登录'];
            const hotspotTextMarkers = ['上传时长', '20GB', 'MP4', 'H.264', '点击上传', '选择文件', '拖拽', '点击或拖拽上传'];
            const geometryWords = ['上传时长', '20GB', 'MP4', 'H.264', '点击上传', '选择文件', '拖拽', '点击或拖拽上传', '上传视频'];
            const negativeContainerWords = ['视频管理', '发表动态', '内容管理', '草稿箱', '视频号助手', '通知中心', '首页'];
            const pageTitle = normalize(document.title || '').slice(0, 80);
            const pageText = document.body ? normalize(document.body.innerText || '') : '';
            const fileInputCount = document.querySelectorAll("input[type='file']").length;
            const blockedTextHit = blockedWords.find((kw) => kw && pageText.includes(kw)) || '';
            const initTextHit = initWords.find((kw) => kw && pageText.includes(kw)) || '';
            const loginTextHit = loginWords.find((kw) => kw && pageText.includes(kw)) || '';
            const guardState = blockedTextHit
                ? 'blocked'
                : (loginTextHit ? 'login_required' : (initTextHit ? 'init_pending' : 'ready'));
            const weakReadyProbe = `title=${pageTitle};body_text_len=${pageText.length};file_input_count=${fileInputCount}`;
            const maxFrameDepth = 3;
            const maxShadowDepth = 4;
            const selectorScanLimit = 4200;
            const textScanLimit = 5200;
            const hotspotScanLimit = 2400;
            const geometryScanLimit = 9000;
            const viewportWidth = Math.max(1, (window.visualViewport && window.visualViewport.width) || window.innerWidth || 1280);
            const viewportHeight = Math.max(1, (window.visualViewport && window.visualViewport.height) || window.innerHeight || 720);
            const viewportCenterX = viewportWidth / 2;
            const viewportCenterY = viewportHeight / 2;
            const viewportDiagonal = Math.max(1, Math.sqrt(viewportWidth * viewportWidth + viewportHeight * viewportHeight));
            let textHitCount = 0;
            let selectorScannedNodes = 0;
            let textScannedNodes = 0;
            let hotspotScannedNodes = 0;
            let geometryScannedNodes = 0;
            let shadowRootCount = 0;
            let geometryCandidateCount = 0;
            let geometryCandidatesEncoded = '[]';

            function isVisible(el) {
                if (!el) return false;
                const rect = el.getBoundingClientRect();
                const style = window.getComputedStyle(el);
                return !!rect
                    && rect.width >= 6
                    && rect.height >= 6
                    && style
                    && style.visibility !== 'hidden'
                    && style.display !== 'none';
            }

            function nodeText(el) {
                if (!el) return '';
                return normalize(
                    el.innerText
                    || el.textContent
                    || el.getAttribute('aria-label')
                    || el.getAttribute('title')
                    || ''
                );
            }

            function isClickable(el) {
                if (!el) return false;
                const tag = (el.tagName || '').toLowerCase();
                if (tag === 'button' || tag === 'input' || tag === 'label' || tag === 'a') return true;
                const role = (el.getAttribute('role') || '').toLowerCase();
                if (role === 'button') return true;
                const tabindex = el.getAttribute('tabindex');
                if (tabindex !== null && tabindex !== '-1') return true;
                if (typeof el.onclick === 'function' || el.hasAttribute('onclick')) return true;
                const style = window.getComputedStyle(el);
                return !!style && style.cursor === 'pointer';
            }

            function findClickableAncestor(node) {
                let current = node;
                for (let depth = 0; current && depth < 8; depth += 1) {
                    if (isClickable(current) && isVisible(current)) return current;
                    if (current.parentElement) {
                        current = current.parentElement;
                        continue;
                    }
                    const root = typeof current.getRootNode === 'function' ? current.getRootNode() : null;
                    current = root && root.host ? root.host : null;
                }
                return null;
            }

            function clickWithFallback(el) {
                const clickChain = [];
                const steps = [
                    ['pointerdown', () => new PointerEvent('pointerdown', { bubbles: true, cancelable: true, composed: true, pointerType: 'mouse', isPrimary: true })],
                    ['mousedown', () => new MouseEvent('mousedown', { bubbles: true, cancelable: true, composed: true, button: 0 })],
                    ['mouseup', () => new MouseEvent('mouseup', { bubbles: true, cancelable: true, composed: true, button: 0 })],
                    ['click', () => new MouseEvent('click', { bubbles: true, cancelable: true, composed: true, button: 0 })],
                ];
                for (const [name, build] of steps) {
                    try {
                        el.dispatchEvent(build());
                        clickChain.push(name);
                    } catch (_) {}
                }
                try {
                    el.click();
                    clickChain.push('el.click()');
                } catch (_) {}
                return clickChain.join('>');
            }

            function centerPoint(el) {
                if (!el) return null;
                const rect = el.getBoundingClientRect();
                if (!rect || rect.width <= 0 || rect.height <= 0) return null;
                return {
                    x: rect.left + rect.width / 2,
                    y: rect.top + rect.height / 2,
                    width: rect.width,
                    height: rect.height,
                };
            }

            function pushSelectorHit(context, selector, value) {
                if (selectorHits.length >= 80) return;
                selectorHits.push(context + '|' + selector + ':' + value);
            }

            function collectFrameContexts(doc, path, depth, frames) {
                frames.push({ doc, framePath: path, context: 'frame:' + path });
                if (depth >= maxFrameDepth) return;

                let iframes = [];
                try {
                    iframes = Array.from(doc.querySelectorAll('iframe'));
                } catch (_) {
                    iframes = [];
                }

                for (let i = 0; i < iframes.length; i += 1) {
                    let childDoc = null;
                    try {
                        childDoc = iframes[i].contentDocument;
                    } catch (_) {
                        childDoc = null;
                    }
                    if (!childDoc) continue;
                    collectFrameContexts(childDoc, path + '/' + i, depth + 1, frames);
                }
            }

            function collectRoots(root, framePath, shadowPath, depth, roots) {
                const context = shadowPath ? ('frame:' + framePath + '|' + shadowPath) : ('frame:' + framePath);
                roots.push({ root, context, framePath });
                if (depth >= maxShadowDepth) return;

                let nodes = [];
                try {
                    nodes = typeof root.querySelectorAll === 'function'
                        ? Array.from(root.querySelectorAll('*'))
                        : [];
                } catch (_) {
                    nodes = [];
                }

                for (let i = 0; i < nodes.length; i += 1) {
                    const el = nodes[i];
                    if (!el || !el.shadowRoot) continue;
                    shadowRootCount += 1;
                    const tag = (el.tagName || 'shadow').toLowerCase();
                    const nextShadowPath = shadowPath
                        ? (shadowPath + '/shadow:' + tag + '[' + i + ']')
                        : ('shadow:' + tag + '[' + i + ']');
                    collectRoots(el.shadowRoot, framePath, nextShadowPath, depth + 1, roots);
                }
            }

            function contextPriority(context) {
                if (!context) return 0;
                if (context.includes('shadow:wujie-app')) return 30;
                if (context.includes('wujie-app')) return 16;
                if (context.includes('frame:top')) return 6;
                return 0;
            }

            function attributeText(el) {
                if (!el) return '';
                return normalize(
                    (el.getAttribute && el.getAttribute('aria-label')) || ''
                    || (el.getAttribute && el.getAttribute('title')) || ''
                    || ''
                );
            }

            function containerTextHit(text) {
                const normalized = (text || '').toLowerCase();
                return negativeContainerWords.find((kw) => normalized.includes(kw.toLowerCase())) || '';
            }

            const frameContexts = [];
            collectFrameContexts(document, 'top', 0, frameContexts);
            const frameCount = frameContexts.length;

            function basePayload(status, marker, rootCtx, clickChain, point) {
                const totalScanned = selectorScannedNodes + textScannedNodes + hotspotScannedNodes + geometryScannedNodes;
                return {
                    status,
                    marker,
                    frame_count: frameCount,
                    frame_path: rootCtx ? rootCtx.framePath : '',
                    shadow_root_count: shadowRootCount,
                    clicked_context: rootCtx ? rootCtx.context : '',
                    selector_hits: selectorHits.join(','),
                    text_hit_count: textHitCount,
                    scanned_nodes: totalScanned,
                    selector_scanned_nodes: selectorScannedNodes,
                    text_scanned_nodes: textScannedNodes,
                    hotspot_scanned_nodes: hotspotScannedNodes,
                    geometry_scanned_nodes: geometryScannedNodes,
                    candidate_summary: candidateSummary.join(' / '),
                    blocked_text_hit: blockedTextHit,
                    init_text_hit: initTextHit,
                    login_text_hit: loginTextHit,
                    guard_state: guardState,
                    weak_ready_probe: weakReadyProbe,
                    click_chain: clickChain || '',
                    geometry_candidate_count: geometryCandidateCount,
                    geometry_top_summary: geometryTopSummary.join(' / '),
                    geometry_selected: '',
                    geometry_selected_reason: '',
                    geometry_candidates: geometryCandidatesEncoded,
                    click_method: clickChain ? 'js_chain' : '',
                    click_x: point ? point.x : null,
                    click_y: point ? point.y : null,
                    human_summary: '',
                };
            }

            if (guardState !== 'ready') {
                const payload = basePayload('guard_blocked', 'guard:' + guardState, null, '', null);
                payload.human_summary = '页面状态未就绪: ' + guardState;
                return JSON.stringify(payload);
            }

            for (const frameCtx of frameContexts) {
                const roots = [];
                collectRoots(frameCtx.doc, frameCtx.framePath, '', 0, roots);
                for (const rootCtx of roots) {
                    for (const sel of selectors) {
                        let nodes = [];
                        try {
                            nodes = Array.from(rootCtx.root.querySelectorAll(sel));
                        } catch (_) {
                            pushSelectorHit(rootCtx.context, sel, 'ERR');
                            continue;
                        }

                        pushSelectorHit(rootCtx.context, sel, nodes.length);
                        for (const el of nodes) {
                            if (selectorScannedNodes >= selectorScanLimit) break;
                            selectorScannedNodes += 1;
                            if (!isVisible(el)) continue;
                            const clickable = isClickable(el) ? el : findClickableAncestor(el);
                            if (!clickable || !isVisible(clickable)) continue;

                            const clickPoint = centerPoint(clickable);
                            const clickChain = clickWithFallback(clickable);
                            const payload = basePayload('clicked_selector', 'selector:' + sel, rootCtx, clickChain, clickPoint);
                            return JSON.stringify(payload);
                        }
                    }
                }
            }

            for (const frameCtx of frameContexts) {
                const roots = [];
                collectRoots(frameCtx.doc, frameCtx.framePath, '', 0, roots);
                for (const rootCtx of roots) {
                    let nodes = [];
                    try {
                        nodes = typeof rootCtx.root.querySelectorAll === 'function'
                            ? Array.from(rootCtx.root.querySelectorAll('*'))
                            : [];
                    } catch (_) {
                        nodes = [];
                    }

                    for (const el of nodes) {
                        if (textScannedNodes >= textScanLimit) break;
                        textScannedNodes += 1;
                        if (!isVisible(el)) continue;

                        const text = nodeText(el);
                        if (!text) continue;
                        if (candidateSummary.length < 8) {
                            candidateSummary.push(text.slice(0, 40));
                        }

                        const normalized = text.toLowerCase();
                        if (!markers.some((kw) => normalized.includes(kw))) continue;

                        textHitCount += 1;
                        const clickable = findClickableAncestor(el);
                        if (!clickable || !isVisible(clickable)) continue;

                        const clickPoint = centerPoint(clickable);
                        const clickChain = clickWithFallback(clickable);
                        const clickableText = nodeText(clickable) || text;
                        const payload = basePayload('clicked_text', 'text:' + clickableText.slice(0, 20), rootCtx, clickChain, clickPoint);
                        return JSON.stringify(payload);
                    }
                }
            }

            for (const frameCtx of frameContexts) {
                const roots = [];
                collectRoots(frameCtx.doc, frameCtx.framePath, '', 0, roots);
                for (const rootCtx of roots) {
                    const hotspotSelectors = [
                        "[class*='upload']",
                        "[class*='uploader']",
                        "[class*='drag']",
                        "[class*='drop']",
                        "[class*='post-create']",
                        "label[for*='upload']"
                    ];
                    for (const sel of hotspotSelectors) {
                        let nodes = [];
                        try {
                            nodes = Array.from(rootCtx.root.querySelectorAll(sel));
                        } catch (_) {
                            continue;
                        }
                        pushSelectorHit(rootCtx.context, 'hotspot:' + sel, nodes.length);
                        for (const el of nodes) {
                            if (hotspotScannedNodes >= hotspotScanLimit) break;
                            hotspotScannedNodes += 1;
                            if (!isVisible(el)) continue;
                            const text = nodeText(el);
                            const className = normalize(el.className || '').toLowerCase();
                            let borderStyle = '';
                            try {
                                borderStyle = (window.getComputedStyle(el).borderStyle || '').toLowerCase();
                            } catch (_) {
                                borderStyle = '';
                            }
                            const markerHit = hotspotTextMarkers.some((kw) => text.includes(kw));
                            const uploadLike = markerHit
                                || className.includes('upload')
                                || className.includes('drag')
                                || className.includes('drop')
                                || borderStyle.includes('dashed');
                            if (!uploadLike) continue;

                            const clickable = isClickable(el) ? el : (findClickableAncestor(el) || el);
                            if (!isVisible(clickable)) continue;
                            const clickPoint = centerPoint(clickable);
                            const clickChain = clickWithFallback(clickable);
                            const payload = basePayload('clicked_hotspot', 'hotspot:' + sel, rootCtx, clickChain, clickPoint);
                            return JSON.stringify(payload);
                        }
                    }
                }
            }

            const geometryCandidates = [];
            const geometrySeen = new Set();
            for (const frameCtx of frameContexts) {
                const roots = [];
                collectRoots(frameCtx.doc, frameCtx.framePath, '', 0, roots);
                roots.sort((a, b) => contextPriority(b.context) - contextPriority(a.context));
                for (const rootCtx of roots) {
                    let nodes = [];
                    try {
                        nodes = typeof rootCtx.root.querySelectorAll === 'function'
                            ? Array.from(rootCtx.root.querySelectorAll('*'))
                            : [];
                    } catch (_) {
                        nodes = [];
                    }

                    for (const el of nodes) {
                        if (geometryScannedNodes >= geometryScanLimit) break;
                        geometryScannedNodes += 1;
                        if (!isVisible(el)) continue;

                        const rect = el.getBoundingClientRect();
                        if (!rect || rect.width <= 0 || rect.height <= 0) continue;
                        const text = nodeText(el);
                        const normalized = text.toLowerCase();
                        const attrText = attributeText(el).toLowerCase();

                        let borderStyle = '';
                        try {
                            borderStyle = (window.getComputedStyle(el).borderStyle || '').toLowerCase();
                        } catch (_) {
                            borderStyle = '';
                        }

                        const sizeHit = rect.width >= 160 && rect.height >= 160;
                        const dashedHit = borderStyle.includes('dashed');
                        const textHit = geometryWords.some((kw) => normalized.includes(kw.toLowerCase()));
                        const uploadSemanticHit = geometryWords.some((kw) => attrText.includes(kw.toLowerCase()));
                        const containerHit = containerTextHit(text);
                        const area = rect.width * rect.height;
                        const isOversize = area > (viewportWidth * viewportHeight * 0.58);
                        const strongSignal = textHit || dashedHit || uploadSemanticHit;
                        if (!strongSignal && !sizeHit) continue;
                        if (containerHit && !textHit && !dashedHit) continue;
                        if (isOversize && !textHit && !dashedHit) continue;

                        const className = normalize(el.className || '').toLowerCase();
                        const classHit = className.includes('upload')
                            || className.includes('uploader')
                            || className.includes('drag')
                            || className.includes('drop');
                        const clickable = isClickable(el) ? el : (findClickableAncestor(el) || el);
                        if (!clickable || !isVisible(clickable)) continue;

                        const point = centerPoint(clickable);
                        if (!point) continue;

                        const dedupeKey = rootCtx.context + '|' + Math.round(point.x) + ':' + Math.round(point.y);
                        if (geometrySeen.has(dedupeKey)) continue;
                        geometrySeen.add(dedupeKey);

                        const dx = point.x - viewportCenterX;
                        const dy = point.y - viewportCenterY;
                        const distancePenalty = (Math.sqrt(dx * dx + dy * dy) / viewportDiagonal) * 20;

                        let score = 0;
                        const reasons = [];
                        if (textHit) {
                            score += 45;
                            reasons.push('text');
                        }
                        if (dashedHit) {
                            score += 30;
                            reasons.push('dashed');
                        }
                        if (uploadSemanticHit) {
                            score += 18;
                            reasons.push('semantic');
                        }
                        if (classHit) {
                            score += 12;
                            reasons.push('class');
                        }
                        if (contextPriority(rootCtx.context) > 0) {
                            score += contextPriority(rootCtx.context);
                            reasons.push('wujie');
                        }
                        if (area >= 160 * 160 && area <= 900 * 900) {
                            score += 8;
                            reasons.push('size');
                        }
                        if (containerHit) {
                            score -= 42;
                            reasons.push('container_penalty');
                        }
                        if (isOversize) {
                            score -= 24;
                            reasons.push('oversize_penalty');
                        }
                        score -= distancePenalty;

                        geometryCandidates.push({
                            clickable,
                            score,
                            reasons: reasons.join('+'),
                            context: rootCtx.context,
                            framePath: rootCtx.framePath,
                            x: point.x,
                            y: point.y,
                            width: point.width,
                            height: point.height,
                            text,
                        });
                    }
                }
            }

            geometryCandidates.sort((a, b) => b.score - a.score);
            const topGeometry = geometryCandidates.slice(0, 3);
            geometryCandidateCount = geometryCandidates.length;
            geometryTopSummary.length = 0;
            geometryCandidatesEncoded = JSON.stringify(
                topGeometry.map((item) => ({
                    x: Number(item.x.toFixed(2)),
                    y: Number(item.y.toFixed(2)),
                    score: Number(item.score.toFixed(2)),
                    context: item.context,
                    frame_path: item.framePath,
                    reason: item.reasons,
                }))
            );
            for (const item of topGeometry) {
                geometryTopSummary.push(
                    item.context
                    + '|score=' + item.score.toFixed(1)
                    + '|size=' + Math.round(item.width) + 'x' + Math.round(item.height)
                    + '|reason=' + (item.reasons || 'none')
                    + '|text=' + (item.text || '').slice(0, 18)
                );
            }

            if (topGeometry.length > 0) {
                const selected = topGeometry[0];
                const clickChain = clickWithFallback(selected.clickable);
                const payload = basePayload(
                    'clicked_geometry',
                    'geometry:score=' + Math.round(selected.score),
                    { framePath: selected.framePath, context: selected.context },
                    clickChain,
                    { x: selected.x, y: selected.y }
                );
                payload.geometry_candidate_count = geometryCandidateCount;
                payload.geometry_top_summary = geometryTopSummary.join(' / ');
                payload.geometry_selected =
                    selected.context
                    + '|score=' + selected.score.toFixed(1)
                    + '|size=' + Math.round(selected.width) + 'x' + Math.round(selected.height)
                    + '|reason=' + (selected.reasons || 'none')
                    + '|text=' + (selected.text || '').slice(0, 20);
                payload.geometry_selected_reason = selected.reasons || 'none';
                payload.geometry_candidates = geometryCandidatesEncoded;
                payload.human_summary =
                    '选中候选: ' + payload.geometry_selected
                    + '; 原因=' + (selected.reasons || 'none')
                    + '; 若未触发将自动切换下一个候选';
                return JSON.stringify(payload);
            }

            const payload = basePayload('not_found', 'not_found', null, '', null);
            payload.geometry_candidate_count = geometryCandidateCount;
            payload.geometry_top_summary = geometryTopSummary.join(' / ');
            payload.geometry_selected = '';
            payload.geometry_selected_reason = '';
            payload.geometry_candidates = geometryCandidatesEncoded;
            payload.human_summary = '没有找到像上传框的候选点';
            return JSON.stringify(payload);
        })()
        "#
        .replace("__SELECTORS__", &selector_json)
        .replace("__MARKERS__", &marker_json)
    } else {
        r#"
        (function() {
            const selectors = __SELECTORS__;
            const textMarkers = __MARKERS__;
            const selectorHits = [];
            const candidateSummary = [];
            const normalize = (value) => (value || '').replace(/\s+/g, ' ').trim();
            const markers = textMarkers
                .map((m) => normalize(m).toLowerCase())
                .filter(Boolean);
            const blockedWords = ['暂时无法使用该功能了', '页面加载失败', '请稍后再试', '网络异常'];
            const pageTitle = normalize(document.title || '').slice(0, 80);
            const pageText = document.body ? normalize(document.body.innerText || '') : '';
            const fileInputCount = document.querySelectorAll("input[type='file']").length;
            const blockedTextHit = blockedWords.find((kw) => kw && pageText.includes(kw)) || '';
            const weakReadyProbe = `title=${pageTitle};body_text_len=${pageText.length};file_input_count=${fileInputCount}`;
            let textHitCount = 0;
            let scannedNodes = 0;

            function isVisible(el) {
                if (!el) return false;
                const rect = el.getBoundingClientRect();
                const style = window.getComputedStyle(el);
                return !!rect
                    && rect.width >= 6
                    && rect.height >= 6
                    && style
                    && style.visibility !== 'hidden'
                    && style.display !== 'none';
            }

            function nodeText(el) {
                if (!el) return '';
                return normalize(
                    el.innerText
                    || el.textContent
                    || el.getAttribute('aria-label')
                    || el.getAttribute('title')
                    || ''
                );
            }

            function isClickable(el) {
                if (!el) return false;
                const tag = (el.tagName || '').toLowerCase();
                if (tag === 'button' || tag === 'input' || tag === 'label' || tag === 'a') return true;
                const role = (el.getAttribute('role') || '').toLowerCase();
                if (role === 'button') return true;
                const tabindex = el.getAttribute('tabindex');
                if (tabindex !== null && tabindex !== '-1') return true;
                if (typeof el.onclick === 'function' || el.hasAttribute('onclick')) return true;
                const style = window.getComputedStyle(el);
                return !!style && style.cursor === 'pointer';
            }

            function findClickableAncestor(node) {
                let current = node;
                for (let depth = 0; current && depth < 6; depth += 1) {
                    if (isClickable(current) && isVisible(current)) return current;
                    current = current.parentElement;
                }
                return null;
            }

            for (const sel of selectors) {
                let nodes = [];
                try {
                    nodes = Array.from(document.querySelectorAll(sel));
                } catch (_) {
                    selectorHits.push(sel + ':ERR');
                    continue;
                }
                selectorHits.push(sel + ':' + nodes.length);
                for (const el of nodes) {
                    scannedNodes += 1;
                    if (!isVisible(el)) continue;
                    const clickable = isClickable(el) ? el : findClickableAncestor(el);
                    if (!clickable || !isVisible(clickable)) continue;
                    clickable.click();
                    return JSON.stringify({
                        status: 'clicked_selector',
                        marker: 'selector:' + sel,
                        frame_count: 1,
                        frame_path: 'top',
                        shadow_root_count: 0,
                        clicked_context: 'frame:top',
                        selector_hits: selectorHits.join(','),
                        text_hit_count: textHitCount,
                        scanned_nodes: scannedNodes,
                        candidate_summary: candidateSummary.join(' / '),
                        blocked_text_hit: blockedTextHit,
                        weak_ready_probe: weakReadyProbe
                    });
                }
            }

            const allNodes = Array.from(document.querySelectorAll('body *'));
            const maxScan = 2500;
            for (const el of allNodes) {
                if (scannedNodes >= maxScan) break;
                scannedNodes += 1;
                if (!isVisible(el)) continue;

                const text = nodeText(el);
                if (!text) continue;
                if (candidateSummary.length < 8) {
                    candidateSummary.push(text.slice(0, 40));
                }

                const normalized = text.toLowerCase();
                if (!markers.some((kw) => normalized.includes(kw))) continue;

                textHitCount += 1;
                const clickable = findClickableAncestor(el);
                if (!clickable || !isVisible(clickable)) continue;
                const clickableText = nodeText(clickable) || text;
                clickable.click();
                return JSON.stringify({
                    status: 'clicked_text',
                    marker: 'text:' + clickableText.slice(0, 20),
                    frame_count: 1,
                    frame_path: 'top',
                    shadow_root_count: 0,
                    clicked_context: 'frame:top',
                    selector_hits: selectorHits.join(','),
                    text_hit_count: textHitCount,
                    scanned_nodes: scannedNodes,
                    candidate_summary: candidateSummary.join(' / '),
                    blocked_text_hit: blockedTextHit,
                    weak_ready_probe: weakReadyProbe
                });
            }

            return JSON.stringify({
                status: 'not_found',
                marker: 'not_found',
                frame_count: 1,
                frame_path: '',
                shadow_root_count: 0,
                clicked_context: '',
                selector_hits: selectorHits.join(','),
                text_hit_count: textHitCount,
                scanned_nodes: scannedNodes,
                candidate_summary: candidateSummary.join(' / '),
                blocked_text_hit: blockedTextHit,
                weak_ready_probe: weakReadyProbe
            });
        })()
        "#
        .replace("__SELECTORS__", &selector_json)
        .replace("__MARKERS__", &marker_json)
    };

    let click_probe_raw: String = page
        .evaluate(click_js.as_str())
        .await
        .map(|v| v.into_value().unwrap_or_else(|_| "{}".into()))
        .unwrap_or_else(|_| "{}".into());
    let parsed: serde_json::Value = serde_json::from_str(&click_probe_raw).unwrap_or_else(|_| {
        serde_json::json!({
            "status": "error",
            "marker": "parse_error",
            "selector_hits": "",
            "candidate_summary": "",
            "text_hit_count": 0,
            "scanned_nodes": 0,
            "frame_count": 1,
            "frame_path": "",
            "shadow_root_count": 0,
            "clicked_context": "",
            "blocked_text_hit": "",
            "init_text_hit": "",
            "login_text_hit": "",
            "guard_state": "none",
            "weak_ready_probe": "",
            "click_chain": "",
            "geometry_candidate_count": 0,
            "geometry_top_summary": "",
            "geometry_selected": "",
            "geometry_selected_reason": "",
            "geometry_candidates": "[]",
            "click_method": "js_chain",
            "click_x": null,
            "click_y": null,
            "selector_scanned_nodes": 0,
            "text_scanned_nodes": 0,
            "hotspot_scanned_nodes": 0,
            "geometry_scanned_nodes": 0,
            "human_summary": ""
        })
    });
    let click_status = parsed
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("error");
    let click_marker = parsed
        .get("marker")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let selector_hits = parsed
        .get("selector_hits")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let candidate_summary = parsed
        .get("candidate_summary")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let text_hit_count = parsed
        .get("text_hit_count")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let scanned_nodes = parsed
        .get("scanned_nodes")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let frame_count = parsed
        .get("frame_count")
        .and_then(|v| v.as_i64())
        .unwrap_or(1);
    let frame_path = parsed
        .get("frame_path")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let shadow_root_count = parsed
        .get("shadow_root_count")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let clicked_context = parsed
        .get("clicked_context")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let blocked_text_hit = parsed
        .get("blocked_text_hit")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let init_text_hit = parsed
        .get("init_text_hit")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let login_text_hit = parsed
        .get("login_text_hit")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let guard_state = parsed
        .get("guard_state")
        .and_then(|v| v.as_str())
        .unwrap_or("none");
    let weak_ready_probe = parsed
        .get("weak_ready_probe")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let mut click_chain = parsed
        .get("click_chain")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let geometry_candidate_count = parsed
        .get("geometry_candidate_count")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let geometry_top_summary = parsed
        .get("geometry_top_summary")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let geometry_selected = parsed
        .get("geometry_selected")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let geometry_selected_reason = parsed
        .get("geometry_selected_reason")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let geometry_candidates_raw = parsed
        .get("geometry_candidates")
        .and_then(|v| v.as_str())
        .unwrap_or("[]");
    let mut click_method = parsed
        .get("click_method")
        .and_then(|v| v.as_str())
        .unwrap_or("js_chain")
        .to_string();
    let click_x = parsed.get("click_x").and_then(|v| v.as_f64());
    let click_y = parsed.get("click_y").and_then(|v| v.as_f64());
    let selector_scanned_nodes = parsed
        .get("selector_scanned_nodes")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let text_scanned_nodes = parsed
        .get("text_scanned_nodes")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let hotspot_scanned_nodes = parsed
        .get("hotspot_scanned_nodes")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let geometry_scanned_nodes = parsed
        .get("geometry_scanned_nodes")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let human_summary = parsed
        .get("human_summary")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let geometry_candidates = parse_geometry_click_candidates(geometry_candidates_raw);

    if platform == "wechat" && guard_state != "ready" {
        let current = current_url(page).await;
        let file_inputs = gather_file_inputs_summary(page).await;
        disable_file_chooser_intercept(page).await;
        bail!(
            "[文件选择器-点击触发] 微信页面状态未就绪（guard_state={} blocked_text_hit={} init_text_hit={} login_text_hit={} weak_ready_probe={} current_url={} file_inputs={})",
            guard_state,
            blocked_text_hit,
            init_text_hit,
            login_text_hit,
            weak_ready_probe,
            current,
            file_inputs
        );
    }

    if click_status != "clicked_selector"
        && click_status != "clicked_text"
        && click_status != "clicked_hotspot"
        && click_status != "clicked_geometry"
    {
        let current = current_url(page).await;
        let file_inputs = gather_file_inputs_summary(page).await;
        disable_file_chooser_intercept(page).await;
        bail!(
            "[文件选择器-点击触发] 未找到可点击上传入口（platform={} click_status={} clicked_marker={} frame_count={} frame_path={} shadow_root_count={} clicked_context={} selector_hits={} text_hit_count={} scanned_nodes={} selector_scanned_nodes={} text_scanned_nodes={} hotspot_scanned_nodes={} geometry_scanned_nodes={} blocked_text_hit={} init_text_hit={} login_text_hit={} guard_state={} weak_ready_probe={} click_chain={} candidate_summary={} geometry_candidate_count={} geometry_top_summary={} geometry_selected={} geometry_selected_reason={} click_method={} human_summary={} current_url={} file_inputs={})",
            platform,
            click_status,
            click_marker,
            frame_count,
            frame_path,
            shadow_root_count,
            clicked_context,
            selector_hits,
            text_hit_count,
            scanned_nodes,
            selector_scanned_nodes,
            text_scanned_nodes,
            hotspot_scanned_nodes,
            geometry_scanned_nodes,
            blocked_text_hit,
            init_text_hit,
            login_text_hit,
            guard_state,
            weak_ready_probe,
            click_chain,
            candidate_summary,
            geometry_candidate_count,
            geometry_top_summary,
            geometry_selected,
            geometry_selected_reason,
            click_method,
            human_summary,
            current,
            file_inputs
        );
    }

    info!(
        "[文件选择器-点击触发] 点击结果：platform={} marker={} frame_count={} frame_path={} shadow_root_count={} clicked_context={} selector_hits={} text_hit_count={} scanned_nodes={} selector_scanned_nodes={} text_scanned_nodes={} hotspot_scanned_nodes={} geometry_scanned_nodes={} blocked_text_hit={} init_text_hit={} login_text_hit={} guard_state={} weak_ready_probe={} click_chain={} candidates={} geometry_candidate_count={} geometry_top_summary={} geometry_selected={} geometry_selected_reason={} click_method={} human_summary={}",
        platform,
        click_marker,
        frame_count,
        frame_path,
        shadow_root_count,
        clicked_context,
        selector_hits,
        text_hit_count,
        scanned_nodes,
        selector_scanned_nodes,
        text_scanned_nodes,
        hotspot_scanned_nodes,
        geometry_scanned_nodes,
        blocked_text_hit,
        init_text_hit,
        login_text_hit,
        guard_state,
        weak_ready_probe,
        click_chain,
        candidate_summary,
        geometry_candidate_count,
        geometry_top_summary,
        geometry_selected,
        geometry_selected_reason,
        click_method,
        human_summary
    );

    let mut event_state: String;
    let mut backend_node_id: Option<BackendNodeId> = None;
    let mut click_round: u8 = 1;

    if platform == "wechat" {
        let retry_candidates = build_wechat_retry_candidates(
            click_x,
            click_y,
            clicked_context,
            frame_path,
            &geometry_candidates,
        );
        if retry_candidates.is_empty() {
            event_state = "wechat_no_retry_candidates".to_string();
        } else {
            event_state = "wechat_cdp_retry_started".to_string();
            let deadline = Instant::now() + Duration::from_secs(10);
            for (idx, candidate) in retry_candidates.iter().take(3).enumerate() {
                click_round = (idx + 1) as u8;
                if Instant::now() >= deadline {
                    event_state = "wechat_timeout_total_budget".to_string();
                    break;
                }

                info!(
                    "[文件选择器-点击触发] 微信候选{} 优先使用 CDP 鼠标点击（x={:.1} y={:.1} score={:.1} reason={} context={}）",
                    idx + 1,
                    candidate.x,
                    candidate.y,
                    candidate.score,
                    candidate.reason,
                    candidate.context
                );
                if let Err(e) = cdp_mouse_left_click(page, candidate.x, candidate.y).await {
                    warn!(
                        "[文件选择器-点击触发] 微信 CDP 鼠标点击失败（candidate={} x={:.1} y={:.1}）：{}",
                        idx + 1,
                        candidate.x,
                        candidate.y,
                        e
                    );
                } else {
                    click_method = "cdp_mouse".to_string();
                    let remain_after_cdp = deadline.saturating_duration_since(Instant::now());
                    if remain_after_cdp.is_zero() {
                        event_state = "wechat_timeout_total_budget".to_string();
                        break;
                    }
                    let cdp_wait_ms = (remain_after_cdp.as_millis() as u64).min(1700);
                    let cdp_event =
                        tokio::time::timeout(Duration::from_millis(cdp_wait_ms), event_stream.next()).await;
                    match cdp_event {
                        Ok(Some(evt)) => {
                            info!(
                                "[文件选择器-点击触发] 微信候选{} CDP点击后收到事件 mode={:?} backend_node_id={:?}",
                                idx + 1,
                                evt.mode,
                                evt.backend_node_id
                            );
                            backend_node_id = evt.backend_node_id;
                            event_state = format!("opened_after_cdp_round_{}", idx + 1);
                            break;
                        }
                        Ok(None) => {
                            event_state = "stream_closed_after_cdp".to_string();
                            warn!("[文件选择器-点击触发] 微信 CDP 点击后事件流结束");
                            break;
                        }
                        Err(_) => {
                            event_state = format!("timeout_after_cdp_round_{}", idx + 1);
                        }
                    }
                }

                if backend_node_id.is_some() {
                    break;
                }

                let js_chain_result =
                    js_click_geometry_candidate(page, &candidate.frame_path, candidate.x, candidate.y)
                        .await
                        .unwrap_or_else(|e| format!("js_click_error:{}", e));
                if !js_chain_result.is_empty() {
                    click_chain = if click_chain.is_empty() {
                        js_chain_result.clone()
                    } else {
                        format!("{}|{}", click_chain, js_chain_result)
                    };
                }
                click_method = "js_chain".to_string();

                let remain_after_js = deadline.saturating_duration_since(Instant::now());
                if remain_after_js.is_zero() {
                    event_state = "wechat_timeout_total_budget".to_string();
                    break;
                }
                let js_wait_ms = (remain_after_js.as_millis() as u64).min(1700);
                let js_event =
                    tokio::time::timeout(Duration::from_millis(js_wait_ms), event_stream.next()).await;
                match js_event {
                    Ok(Some(evt)) => {
                        info!(
                            "[文件选择器-点击触发] 微信候选{} JS补充点击后收到事件 mode={:?} backend_node_id={:?}",
                            idx + 1,
                            evt.mode,
                            evt.backend_node_id
                        );
                        backend_node_id = evt.backend_node_id;
                        event_state = format!("opened_after_js_round_{}", idx + 1);
                        break;
                    }
                    Ok(None) => {
                        event_state = "stream_closed_after_js".to_string();
                        warn!("[文件选择器-点击触发] 微信 JS 补充点击后事件流结束");
                        break;
                    }
                    Err(_) => {
                        event_state = format!("timeout_after_js_round_{}", idx + 1);
                    }
                }
            }
        }

        if backend_node_id.is_none() {
            let tail_event = tokio::time::timeout(Duration::from_millis(600), event_stream.next()).await;
            match tail_event {
                Ok(Some(evt)) => {
                    backend_node_id = evt.backend_node_id;
                    event_state = "opened_after_tail_wait".to_string();
                }
                Ok(None) => {
                    event_state = "stream_closed_after_tail_wait".to_string();
                }
                Err(_) => {
                    if !event_state.contains("timeout") && !event_state.contains("stream_closed") {
                        event_state = "timeout_after_wechat_retries".to_string();
                    }
                }
            }
        }
    } else {
        let first_wait_ms = 6000;
        let event =
            tokio::time::timeout(Duration::from_millis(first_wait_ms), event_stream.next()).await;
        backend_node_id = match event {
            Ok(Some(evt)) => {
                info!(
                    "[文件选择器-点击触发] 收到事件 mode={:?} backend_node_id={:?}",
                    evt.mode, evt.backend_node_id
                );
                event_state = "opened".to_string();
                evt.backend_node_id
            }
            Ok(None) => {
                event_state = "stream_closed".to_string();
                warn!("[文件选择器-点击触发] 事件流结束，未收到事件");
                None
            }
            Err(_) => {
                event_state = "timeout".to_string();
                warn!(
                    "[文件选择器-点击触发] 等待文件选择器事件超时（platform={} wait_ms={} click_method={}）",
                    platform, first_wait_ms, click_method
                );
                None
            }
        };
    }

    let mut set_files = SetFileInputFilesParams::new(vec![file_path.to_string()]);
    if let Some(bn_id) = backend_node_id.clone() {
        set_files.backend_node_id = Some(bn_id);
    } else {
        if platform == "wechat" {
            let current = current_url(page).await;
            let file_inputs = gather_file_inputs_summary(page).await;
            disable_file_chooser_intercept(page).await;
            bail!(
                "WECHAT_CHOOSER_NOT_OPENED: [文件选择器-点击触发] 多轮点击后仍未收到文件选择器事件（platform={} event_state={} click_status={} clicked_marker={} frame_count={} frame_path={} shadow_root_count={} clicked_context={} selector_hits={} text_hit_count={} scanned_nodes={} selector_scanned_nodes={} text_scanned_nodes={} hotspot_scanned_nodes={} geometry_scanned_nodes={} blocked_text_hit={} weak_ready_probe={} click_chain={} candidate_summary={} geometry_candidate_count={} geometry_top_summary={} geometry_selected={} geometry_selected_reason={} click_method={} click_round={} human_summary={} current_url={} file_inputs={})",
                platform,
                event_state,
                click_status,
                click_marker,
                frame_count,
                frame_path,
                shadow_root_count,
                clicked_context,
                selector_hits,
                text_hit_count,
                scanned_nodes,
                selector_scanned_nodes,
                text_scanned_nodes,
                hotspot_scanned_nodes,
                geometry_scanned_nodes,
                blocked_text_hit,
                weak_ready_probe,
                click_chain,
                candidate_summary,
                geometry_candidate_count,
                geometry_top_summary,
                geometry_selected,
                geometry_selected_reason,
                click_method,
                click_round,
                human_summary,
                current,
                file_inputs
            );
        }
        let doc = page
            .execute(GetDocumentParams::builder().depth(0).build())
            .await
            .context("[文件选择器-点击触发] 获取文档失败")?;
        let root_node_id = doc.result.root.node_id;
        let query = QuerySelectorParams::new(root_node_id, "input[type='file']");
        let query_result = page
            .execute(query)
            .await
            .context("[文件选择器-点击触发] 查询 input[type='file'] 失败")?;
        if *query_result.result.node_id.inner() <= 0 {
            let current = current_url(page).await;
            let file_inputs = gather_file_inputs_summary(page).await;
            disable_file_chooser_intercept(page).await;
            bail!(
                "[文件选择器-点击触发] 未获取到有效文件输入节点（platform={} event_state={} click_status={} clicked_marker={} frame_count={} frame_path={} shadow_root_count={} clicked_context={} selector_hits={} text_hit_count={} scanned_nodes={} selector_scanned_nodes={} text_scanned_nodes={} hotspot_scanned_nodes={} geometry_scanned_nodes={} blocked_text_hit={} weak_ready_probe={} click_chain={} candidate_summary={} geometry_candidate_count={} geometry_top_summary={} geometry_selected={} geometry_selected_reason={} click_method={} human_summary={} current_url={} file_inputs={})",
                platform,
                event_state,
                click_status,
                click_marker,
                frame_count,
                frame_path,
                shadow_root_count,
                clicked_context,
                selector_hits,
                text_hit_count,
                scanned_nodes,
                selector_scanned_nodes,
                text_scanned_nodes,
                hotspot_scanned_nodes,
                geometry_scanned_nodes,
                blocked_text_hit,
                weak_ready_probe,
                click_chain,
                candidate_summary,
                geometry_candidate_count,
                geometry_top_summary,
                geometry_selected,
                geometry_selected_reason,
                click_method,
                human_summary,
                current,
                file_inputs
            );
        }
        set_files.node_id = Some(query_result.result.node_id);
    }

    let set_result = page
        .execute(set_files)
        .await
        .with_context(|| {
            format!(
                "[文件选择器-点击触发] 通过 CDP 设置文件失败（platform={} event_state={} click_status={} clicked_marker={} frame_count={} frame_path={} shadow_root_count={} clicked_context={} selector_hits={} text_hit_count={} scanned_nodes={} selector_scanned_nodes={} text_scanned_nodes={} hotspot_scanned_nodes={} geometry_scanned_nodes={} blocked_text_hit={} weak_ready_probe={} click_chain={} candidate_summary={} geometry_candidate_count={} geometry_top_summary={} geometry_selected={} geometry_selected_reason={} click_method={} human_summary={})",
                platform, event_state, click_status, click_marker, frame_count, frame_path, shadow_root_count, clicked_context, selector_hits, text_hit_count, scanned_nodes, selector_scanned_nodes, text_scanned_nodes, hotspot_scanned_nodes, geometry_scanned_nodes, blocked_text_hit, weak_ready_probe, click_chain, candidate_summary, geometry_candidate_count, geometry_top_summary, geometry_selected, geometry_selected_reason, click_method, human_summary
            )
        });
    disable_file_chooser_intercept(page).await;
    set_result?;
    let chooser_opened = backend_node_id.is_some();
    info!(
        "[文件选择器-点击触发] 文件设置成功（platform={} event_state={} clicked={} clicked_context={} click_chain={} click_method={} click_round={} chooser_opened={} geometry_selected={} geometry_selected_reason={} human_summary={})",
        platform, event_state, click_marker, clicked_context, click_chain, click_method, click_round, chooser_opened, geometry_selected, geometry_selected_reason, human_summary
    );

    let marker = if clicked_context.is_empty() {
        click_marker.to_string()
    } else {
        format!("{}@{}", click_marker, clicked_context)
    };
    Ok(ClickChooserUploadResult {
        marker,
        chooser_opened,
        chooser_event_state: event_state,
        click_method,
        click_round,
        clicked_context: clicked_context.to_string(),
        signal_source: "chooser:file_set".to_string(),
        file_set: true,
    })
}

/// 通过模拟拖拽事件（CDP Input.dispatchDragEvent）上传文件。
/// 适用于 setFileInputFiles 无法触发前端上传逻辑的自定义上传组件。
pub async fn upload_file_via_drag_drop(
    page: &Page,
    file_path: &str,
    platform: &str,
    drop_zone_selectors: &[&str],
) -> Result<String> {
    // 查找有效的拖放区域元素，获取中心坐标
    let mut center_x: f64 = 0.0;
    let mut center_y: f64 = 0.0;
    let mut found_selector = String::new();
    let mut drop_target_source = "selector".to_string();
    let mut drop_context = "frame:top".to_string();
    let mut drop_score = 100.0;
    let mut geometry_candidate_count = 0_i64;
    let mut geometry_top_summary = String::new();

    for selector in drop_zone_selectors {
        let js = format!(
            r#"
            (function() {{
                const el = document.querySelector('{}');
                if (!el) return null;
                const rect = el.getBoundingClientRect();
                return JSON.stringify({{ x: rect.x + rect.width / 2, y: rect.y + rect.height / 2, w: rect.width, h: rect.height }});
            }})()
            "#,
            escape_js_single(selector)
        );

        let result: Option<String> = page
            .evaluate(js.as_str())
            .await
            .ok()
            .and_then(|v| v.into_value().ok());

        if let Some(json_str) = result {
            if let Ok(coords) = serde_json::from_str::<serde_json::Value>(&json_str) {
                let x = coords["x"].as_f64().unwrap_or(0.0);
                let y = coords["y"].as_f64().unwrap_or(0.0);
                let w = coords["w"].as_f64().unwrap_or(0.0);
                let h = coords["h"].as_f64().unwrap_or(0.0);
                if w > 0.0 && h > 0.0 {
                    center_x = x;
                    center_y = y;
                    found_selector = selector.to_string();
                    info!(
                        "[拖拽上传] 找到拖放区域：选择器={} x={:.0} y={:.0} 宽={:.0} 高={:.0}",
                        selector, x, y, w, h
                    );
                    break;
                }
            }
        }
        info!("[拖拽上传] 选择器未匹配：{}", selector);
    }

    if found_selector.is_empty() && platform == "wechat" {
        let geometry_js = r#"
            (function() {
                const normalize = (value) => (value || '').replace(/\s+/g, ' ').trim();
                const markers = ['上传时长', '20GB', 'MP4', 'H.264', '点击上传', '选择文件', '拖拽', '点击或拖拽上传'];
                const maxFrameDepth = 3;
                const maxShadowDepth = 4;
                const maxScan = 12000;
                const viewportWidth = Math.max(1, (window.visualViewport && window.visualViewport.width) || window.innerWidth || 1280);
                const viewportHeight = Math.max(1, (window.visualViewport && window.visualViewport.height) || window.innerHeight || 720);
                const viewportCenterX = viewportWidth / 2;
                const viewportCenterY = viewportHeight / 2;
                const viewportDiagonal = Math.max(1, Math.sqrt(viewportWidth * viewportWidth + viewportHeight * viewportHeight));
                let scannedNodes = 0;
                let shadowRootCount = 0;

                function isVisible(el) {
                    if (!el) return false;
                    const rect = el.getBoundingClientRect();
                    const style = window.getComputedStyle(el);
                    return !!rect
                        && rect.width >= 6
                        && rect.height >= 6
                        && style
                        && style.visibility !== 'hidden'
                        && style.display !== 'none';
                }

                function nodeText(el) {
                    if (!el) return '';
                    return normalize(
                        el.innerText
                        || el.textContent
                        || el.getAttribute('aria-label')
                        || el.getAttribute('title')
                        || ''
                    );
                }

                function collectFrameContexts(doc, path, depth, frames) {
                    frames.push({ doc, framePath: path, context: 'frame:' + path });
                    if (depth >= maxFrameDepth) return;
                    let iframes = [];
                    try {
                        iframes = Array.from(doc.querySelectorAll('iframe'));
                    } catch (_) {
                        iframes = [];
                    }
                    for (let i = 0; i < iframes.length; i += 1) {
                        let childDoc = null;
                        try {
                            childDoc = iframes[i].contentDocument;
                        } catch (_) {
                            childDoc = null;
                        }
                        if (!childDoc) continue;
                        collectFrameContexts(childDoc, path + '/' + i, depth + 1, frames);
                    }
                }

                function collectRoots(root, framePath, shadowPath, depth, roots) {
                    const context = shadowPath ? ('frame:' + framePath + '|' + shadowPath) : ('frame:' + framePath);
                    roots.push({ root, context, framePath });
                    if (depth >= maxShadowDepth) return;
                    let nodes = [];
                    try {
                        nodes = typeof root.querySelectorAll === 'function'
                            ? Array.from(root.querySelectorAll('*'))
                            : [];
                    } catch (_) {
                        nodes = [];
                    }
                    for (let i = 0; i < nodes.length; i += 1) {
                        const el = nodes[i];
                        if (!el || !el.shadowRoot) continue;
                        shadowRootCount += 1;
                        const tag = (el.tagName || 'shadow').toLowerCase();
                        const nextShadowPath = shadowPath
                            ? (shadowPath + '/shadow:' + tag + '[' + i + ']')
                            : ('shadow:' + tag + '[' + i + ']');
                        collectRoots(el.shadowRoot, framePath, nextShadowPath, depth + 1, roots);
                    }
                }

                const frameContexts = [];
                collectFrameContexts(document, 'top', 0, frameContexts);
                const candidates = [];
                const dedupe = new Set();
                const summary = [];

                for (const frameCtx of frameContexts) {
                    const roots = [];
                    collectRoots(frameCtx.doc, frameCtx.framePath, '', 0, roots);
                    for (const rootCtx of roots) {
                        let nodes = [];
                        try {
                            nodes = typeof rootCtx.root.querySelectorAll === 'function'
                                ? Array.from(rootCtx.root.querySelectorAll('*'))
                                : [];
                        } catch (_) {
                            nodes = [];
                        }
                        for (const el of nodes) {
                            if (scannedNodes >= maxScan) break;
                            scannedNodes += 1;
                            if (!isVisible(el)) continue;

                            const rect = el.getBoundingClientRect();
                            if (!rect || rect.width <= 0 || rect.height <= 0) continue;

                            const text = nodeText(el);
                            const normalized = text.toLowerCase();
                            let borderStyle = '';
                            try {
                                borderStyle = (window.getComputedStyle(el).borderStyle || '').toLowerCase();
                            } catch (_) {
                                borderStyle = '';
                            }

                            const sizeHit = rect.width >= 160 && rect.height >= 160;
                            const dashedHit = borderStyle.includes('dashed');
                            const textHit = markers.some((kw) => normalized.includes(kw.toLowerCase()));
                            if (!sizeHit && !dashedHit && !textHit) continue;

                            const className = normalize(el.className || '').toLowerCase();
                            const classHit = className.includes('upload')
                                || className.includes('drag')
                                || className.includes('drop')
                                || className.includes('post');
                            const area = rect.width * rect.height;
                            const x = rect.left + rect.width / 2;
                            const y = rect.top + rect.height / 2;
                            const key = rootCtx.context + '|' + Math.round(x) + ':' + Math.round(y);
                            if (dedupe.has(key)) continue;
                            dedupe.add(key);

                            const dx = x - viewportCenterX;
                            const dy = y - viewportCenterY;
                            const distancePenalty = (Math.sqrt(dx * dx + dy * dy) / viewportDiagonal) * 20;

                            let score = 0;
                            if (textHit) score += 40;
                            if (dashedHit) score += 25;
                            if (classHit) score += 15;
                            if (area >= 160 * 160 && area <= 900 * 900) score += 10;
                            score -= distancePenalty;

                            candidates.push({
                                x,
                                y,
                                score,
                                context: rootCtx.context,
                                width: rect.width,
                                height: rect.height,
                                text,
                            });
                        }
                    }
                }

                candidates.sort((a, b) => b.score - a.score);
                const top = candidates.slice(0, 3);
                for (const item of top) {
                    summary.push(
                        item.context
                        + '|score=' + item.score.toFixed(1)
                        + '|size=' + Math.round(item.width) + 'x' + Math.round(item.height)
                        + '|text=' + (item.text || '').slice(0, 18)
                    );
                }

                if (top.length === 0) {
                    return JSON.stringify({
                        status: 'not_found',
                        candidate_count: candidates.length,
                        top_summary: summary.join(' / '),
                        frame_count: frameContexts.length,
                        shadow_root_count: shadowRootCount,
                        scanned_nodes: scannedNodes,
                    });
                }

                return JSON.stringify({
                    status: 'found',
                    x: Number(top[0].x.toFixed(2)),
                    y: Number(top[0].y.toFixed(2)),
                    score: Number(top[0].score.toFixed(2)),
                    context: top[0].context,
                    candidate_count: candidates.length,
                    top_summary: summary.join(' / '),
                    frame_count: frameContexts.length,
                    shadow_root_count: shadowRootCount,
                    scanned_nodes: scannedNodes,
                });
            })()
        "#;
        let geometry_raw: String = page
            .evaluate(geometry_js)
            .await
            .map(|v| v.into_value().unwrap_or_else(|_| "{}".to_string()))
            .unwrap_or_else(|_| "{}".to_string());
        let geometry: serde_json::Value =
            serde_json::from_str(&geometry_raw).unwrap_or_else(|_| serde_json::json!({}));
        let geometry_status = geometry
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("not_found");
        geometry_candidate_count = geometry
            .get("candidate_count")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        geometry_top_summary = geometry
            .get("top_summary")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        if geometry_status == "found" {
            center_x = geometry.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0);
            center_y = geometry.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0);
            drop_score = geometry.get("score").and_then(|v| v.as_f64()).unwrap_or(0.0);
            drop_context = geometry
                .get("context")
                .and_then(|v| v.as_str())
                .unwrap_or("frame:top")
                .to_string();
            drop_target_source = "geometry".to_string();
            found_selector = format!("geometry:score={}", drop_score.round() as i64);
            info!(
                "[拖拽上传] 微信几何兜底命中：x={:.1} y={:.1} score={:.1} context={} candidate_count={} top_summary={}",
                center_x,
                center_y,
                drop_score,
                drop_context,
                geometry_candidate_count,
                geometry_top_summary
            );
        }
    }

    if found_selector.is_empty() {
        bail!(
            "[拖拽上传] 未找到有效的拖放区域。platform={} 已尝试={} drop_target_source={} geometry_candidate_count={} geometry_top_summary={}",
            platform,
            drop_zone_selectors.join(", "),
            drop_target_source,
            geometry_candidate_count,
            geometry_top_summary
        );
    }

    // 构建拖拽数据
    let drag_data = DragData {
        items: vec![],
        files: Some(vec![file_path.to_string()]),
        drag_operations_mask: 1, // Copy = 1
    };

    // 第1步：dragEnter
    info!(
        "[拖拽上传] 发送 dragEnter 事件到 ({:.0}, {:.0}) source={} context={} score={:.1}",
        center_x, center_y, drop_target_source, drop_context, drop_score
    );
    let drag_enter = DispatchDragEventParams {
        r#type: DispatchDragEventType::DragEnter,
        x: center_x,
        y: center_y,
        data: drag_data.clone(),
        modifiers: None,
    };
    page.execute(drag_enter)
        .await
        .context("[拖拽上传] dragEnter 失败")?;
    tokio::time::sleep(Duration::from_millis(100)).await;

    // 第2步：dragOver
    info!(
        "[拖拽上传] 发送 dragOver 事件到 ({:.0}, {:.0}) source={} context={} score={:.1}",
        center_x, center_y, drop_target_source, drop_context, drop_score
    );
    let drag_over = DispatchDragEventParams {
        r#type: DispatchDragEventType::DragOver,
        x: center_x,
        y: center_y,
        data: drag_data.clone(),
        modifiers: None,
    };
    page.execute(drag_over)
        .await
        .context("[拖拽上传] dragOver 失败")?;
    tokio::time::sleep(Duration::from_millis(100)).await;

    // 第3步：drop
    info!(
        "[拖拽上传] 发送 drop 事件到 ({:.0}, {:.0}) source={} context={} score={:.1}",
        center_x, center_y, drop_target_source, drop_context, drop_score
    );
    let drop_event = DispatchDragEventParams {
        r#type: DispatchDragEventType::Drop,
        x: center_x,
        y: center_y,
        data: drag_data,
        modifiers: None,
    };
    page.execute(drop_event)
        .await
        .context("[拖拽上传] drop 失败")?;

    info!(
        "[拖拽上传] 拖拽上传完成。选择器={} 文件={} drop_target_source={} drop_context={} drop_score={:.1}",
        found_selector, file_path, drop_target_source, drop_context, drop_score
    );
    Ok(format!(
        "{} source={} context={} score={:.1}",
        found_selector, drop_target_source, drop_context, drop_score
    ))
}
