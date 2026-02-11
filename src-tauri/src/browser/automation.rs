use anyhow::{bail, Context, Result};
use chromiumoxide::browser::Browser;
use chromiumoxide::cdp::browser_protocol::dom::{
    BackendNodeId, GetDocumentParams, QuerySelectorParams, SetFileInputFilesParams,
};
use chromiumoxide::cdp::browser_protocol::input::{
    DispatchDragEventParams, DispatchDragEventType, DragData,
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

    loop {
        if pages.is_empty() {
            if created_target_page {
                bail!(
                    "CDP_NO_PAGE: Chrome 调试端口 {} 可用，但没有可操作页面。请先关闭该账号已打开的 Chrome 窗口后重试。",
                    port
                );
            }
            info!(
                "[CDP] 端口 {} 暂无页面，尝试主动创建新页面：{}",
                port, create_url
            );
            match browser.new_page(create_url.to_string()).await {
                Ok(_) => {
                    created_target_page = true;
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
                        "[CDP] 主动创建页面失败（port={} url={}）：{}",
                        port, create_url, e
                    );
                    bail!(
                        "TARGET_PAGE_NOT_FOUND: 无法创建目标页面。port={} expected_url={} error={}",
                        port,
                        expected_url,
                        e
                    );
                }
            }
        }

        let selection = select_best_page(&pages, expected_url, &expected_host).await;
        for (idx, url, score) in &selection.observed {
            info!("[CDP] page[{}] url={} score={}", idx, url, score);
        }

        if !strict_target || selection.score >= STRICT_TARGET_SCORE {
            let page = pages
                .into_iter()
                .nth(selection.idx)
                .context("Chrome 页面选择失败")?;
            info!(
                "已连接到 Chrome CDP，端口 {}，选中页面 idx={} url={} expected={}",
                port, selection.idx, selection.url, expected_url
            );
            return Ok((browser, page));
        }

        if created_target_page {
            let observed_urls = selection
                .observed
                .iter()
                .map(|(_, url, _)| url.clone())
                .collect::<Vec<_>>()
                .join(" | ");
            bail!(
                "TARGET_PAGE_NOT_FOUND: 未定位到目标页面。port={} expected_url={} expected_host={} observed_pages={}",
                port,
                expected_url,
                expected_host,
                observed_urls
            );
        }

        info!(
            "[CDP] 端口 {} 未命中目标 host={}，尝试创建目标页：{}",
            port, expected_host, create_url
        );
        match browser.new_page(create_url.to_string()).await {
            Ok(_) => {
                created_target_page = true;
                pages = wait_for_pages_or_target(
                    &browser,
                    &expected_host,
                    strict_target,
                    CDP_TARGET_RETRY_WAIT_SECS,
                )
                .await?;
            }
            Err(e) => {
                warn!(
                    "[CDP] 创建目标页失败（port={} host={} url={}）：{}",
                    port, expected_host, create_url, e
                );
                bail!(
                    "TARGET_PAGE_NOT_FOUND: 创建目标页失败。port={} expected_url={} expected_host={} error={}",
                    port,
                    expected_url,
                    expected_host,
                    e
                );
            }
        }
    }
}

struct PageSelection {
    idx: usize,
    score: i32,
    url: String,
    observed: Vec<(usize, String, i32)>,
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
    let mut selected_idx = 0usize;
    let mut selected_score = i32::MIN;
    let mut selected_url = String::new();
    let mut observed = Vec::with_capacity(pages.len());

    for (idx, page) in pages.iter().enumerate() {
        let url: String = page
            .evaluate("window.location.href")
            .await
            .map(|v| v.into_value().unwrap_or_else(|_| String::new()))
            .unwrap_or_default();
        let score = score_url_match(&url, expected_url, expected_host);
        observed.push((idx, url.clone(), score));

        if score > selected_score {
            selected_score = score;
            selected_idx = idx;
            selected_url = url;
        }
    }

    PageSelection {
        idx: selected_idx,
        score: selected_score,
        url: selected_url,
        observed,
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
                const href = window.location.href || '';
                if (href.includes('/platform/post/edit') || href.includes('/platform/post/list') || href.includes('/platform/post/result')) {
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
                if (pageText.includes('上传中') || pageText.includes('处理中') || pageText.includes('发布中') || pageText.includes('重新上传')) {
                    return 'text:uploading';
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

                const progress = document.querySelector('ytcp-video-upload-progress, [id*="progress"], [class*="progress"], [class*="upload-progress"]');
                if (progress) {
                    const text = ((progress.textContent || '').trim().replace(/\s+/g, ' ')).slice(0, 80);
                    return text ? ('progress:' + text) : 'progress:visible';
                }

                const pageText = (document.body && document.body.innerText) ? document.body.innerText : '';
                if (pageText.includes('Uploading') || pageText.includes('Processing') || pageText.includes('Checking') || pageText.includes('上传中') || pageText.includes('处理中')) {
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
pub async fn upload_file_via_click_to_open_file_chooser(
    page: &Page,
    file_path: &str,
    click_selectors: &[&str],
) -> Result<String> {
    info!(
        "[文件选择器-点击触发] 开始：selectors={} file={}",
        click_selectors.join(", "),
        file_path
    );

    page.execute(SetInterceptFileChooserDialogParams { enabled: true })
        .await
        .context("[文件选择器-点击触发] 启用文件选择器拦截失败")?;

    let mut event_stream = page
        .event_listener::<EventFileChooserOpened>()
        .await
        .context("[文件选择器-点击触发] 创建事件监听器失败")?;

    let selector_array = click_selectors
        .iter()
        .map(|selector| format!("'{}'", escape_js_single(selector)))
        .collect::<Vec<_>>()
        .join(",");

    let click_js = format!(
        r#"
        (function() {{
            const selectors = [{}];
            for (const sel of selectors) {{
                const el = document.querySelector(sel);
                if (el) {{
                    el.click();
                    return 'selector:' + sel;
                }}
            }}

            const textKeywords = ['上传视频', '点击上传', '上传文件', '选择文件', '拖入', '拖拽', 'Upload', 'Select files', 'Upload videos', 'Create'];
            const candidates = Array.from(document.querySelectorAll('button, [role="button"], [class*="upload"], [data-e2e*="upload"]'));
            for (const el of candidates) {{
                const rect = el.getBoundingClientRect();
                if (!rect || rect.width < 10 || rect.height < 10) continue;
                const text = ((el.innerText || el.textContent || '').trim()).replace(/\s+/g, '');
                if (!text) continue;
                if (textKeywords.some((kw) => text.includes(kw))) {{
                    el.click();
                    return 'text:' + text.slice(0, 20);
                }}
            }}

            return 'not_found';
        }})()
        "#,
        selector_array
    );

    let click_result: String = page
        .evaluate(click_js.as_str())
        .await
        .map(|v| v.into_value().unwrap_or_else(|_| "error".into()))
        .unwrap_or_else(|_| "error".into());

    if click_result == "not_found" || click_result == "error" {
        disable_file_chooser_intercept(page).await;
        bail!(
            "[文件选择器-点击触发] 未找到可点击上传入口（result={}）",
            click_result
        );
    }

    info!("[文件选择器-点击触发] 点击结果：{}", click_result);

    let event = tokio::time::timeout(Duration::from_secs(6), event_stream.next()).await;
    let backend_node_id: Option<BackendNodeId> = match event {
        Ok(Some(evt)) => {
            info!(
                "[文件选择器-点击触发] 收到事件 mode={:?} backend_node_id={:?}",
                evt.mode, evt.backend_node_id
            );
            evt.backend_node_id
        }
        Ok(None) => {
            warn!("[文件选择器-点击触发] 事件流结束，未收到事件");
            None
        }
        Err(_) => {
            warn!("[文件选择器-点击触发] 等待文件选择器事件超时");
            None
        }
    };

    let mut set_files = SetFileInputFilesParams::new(vec![file_path.to_string()]);
    if let Some(bn_id) = backend_node_id {
        set_files.backend_node_id = Some(bn_id);
    } else {
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
            disable_file_chooser_intercept(page).await;
            bail!("[文件选择器-点击触发] 未获取到有效文件输入节点");
        }
        set_files.node_id = Some(query_result.result.node_id);
    }

    let set_result = page
        .execute(set_files)
        .await
        .context("[文件选择器-点击触发] 通过 CDP 设置文件失败");
    disable_file_chooser_intercept(page).await;
    set_result?;
    info!("[文件选择器-点击触发] 文件设置成功");

    Ok(click_result)
}

/// 通过模拟拖拽事件（CDP Input.dispatchDragEvent）上传文件。
/// 适用于 setFileInputFiles 无法触发前端上传逻辑的自定义上传组件。
pub async fn upload_file_via_drag_drop(
    page: &Page,
    file_path: &str,
    drop_zone_selectors: &[&str],
) -> Result<String> {
    // 查找有效的拖放区域元素，获取中心坐标
    let mut center_x: f64 = 0.0;
    let mut center_y: f64 = 0.0;
    let mut found_selector = String::new();

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

    if found_selector.is_empty() {
        bail!(
            "[拖拽上传] 未找到有效的拖放区域。已尝试：{}",
            drop_zone_selectors.join(", ")
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
        "[拖拽上传] 发送 dragEnter 事件到 ({:.0}, {:.0})",
        center_x, center_y
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
        "[拖拽上传] 发送 dragOver 事件到 ({:.0}, {:.0})",
        center_x, center_y
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
        "[拖拽上传] 发送 drop 事件到 ({:.0}, {:.0})",
        center_x, center_y
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
        "[拖拽上传] 拖拽上传完成。选择器={} 文件={}",
        found_selector, file_path
    );
    Ok(found_selector)
}
