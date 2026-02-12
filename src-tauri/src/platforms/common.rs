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
pub const PRE_CLICK_WAIT_MS: u64 = 300;
pub const WEAK_READY_SELF_HEAL_TIMEOUT_SECS: u64 = 8;
pub const WEAK_READY_RELOAD_WAIT_MS: u64 = 400;
pub const WECHAT_GUARD_TIMEOUT_SECS: u64 = 20;
pub const WECHAT_CLICK_RETRY_ROUNDS: usize = 3;
pub const WECHAT_CLICK_RETRY_WAIT_MS: u64 = 2300;
pub const WECHAT_INTERACTIVE_RECHECK_TIMEOUT_SECS: u64 = 3;

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
    pub pre_click_selectors: &'static [&'static str],
    pub click_selectors: &'static [&'static str],
    pub click_text_markers: &'static [&'static str],
    pub require_surface_ready: bool,
    pub fill_failure_is_error: bool,
    pub weak_ready_self_heal: bool,
    pub weak_ready_min_body_text_len: usize,
    pub blocked_text_markers: &'static [&'static str],
    pub init_text_markers: &'static [&'static str],
    pub login_text_markers: &'static [&'static str],
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

struct UploadPageProbe {
    title: String,
    body_text_len: usize,
    body_excerpt: String,
    file_input_count: usize,
    blocked_text_hit: String,
    init_text_hit: String,
    login_text_hit: String,
    surface_text_hit: String,
    anchor_hit: bool,
    surface_selector_hit_count: usize,
    surface_context_hit: String,
    frame_count: usize,
    shadow_root_count: usize,
    scanned_nodes: usize,
    interactive_candidate_count: usize,
    interactive_context: String,
    guard_state: String,
    ready_kind: String,
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
    let mut wechat_file_set_success = false;
    let mut wechat_chooser_event_state = "none".to_string();
    let mut wechat_click_round: u8 = 0;
    let mut wechat_click_method = "none".to_string();
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

    if upload_signal.is_none() && cfg.id == "wechat" {
        info!("[{}上传] 策略B失败，微信优先尝试策略D：点击上传入口...", cfg.name);
        let click_retry_start = std::time::Instant::now();
        for round in 1..=WECHAT_CLICK_RETRY_ROUNDS {
            upload_diagnostics.push(format!(
                "D:round={} start_ms={}",
                round,
                click_retry_start.elapsed().as_millis()
            ));

            if round > 1 {
                let interactive_probe = wait_for_wechat_interactive_ready(
                    page,
                    cfg,
                    WECHAT_INTERACTIVE_RECHECK_TIMEOUT_SECS,
                )
                .await;
                if let Some(probe) = interactive_probe {
                    upload_diagnostics.push(format!(
                        "D:round={} interactive_ready candidates={} context={}",
                        round,
                        probe.interactive_candidate_count,
                        if probe.interactive_context.is_empty() {
                            "none"
                        } else {
                            &probe.interactive_context
                        }
                    ));
                } else {
                    upload_diagnostics.push(format!(
                        "D:round={} interactive_pending(timeout={}s)",
                        round, WECHAT_INTERACTIVE_RECHECK_TIMEOUT_SECS
                    ));
                }
                tokio::time::sleep(Duration::from_millis(WECHAT_CLICK_RETRY_WAIT_MS)).await;
            }

            if !cfg.pre_click_selectors.is_empty() {
                match automation::click_first_visible(page, cfg.pre_click_selectors).await {
                    Ok(marker) => {
                        upload_diagnostics.push(format!("D:round={} pre_click={}", round, marker));
                        tokio::time::sleep(Duration::from_millis(PRE_CLICK_WAIT_MS)).await;
                    }
                    Err(e) => {
                        upload_diagnostics.push(format!("D:round={} pre_click_failed={}", round, e));
                    }
                }
            }

            match automation::upload_file_via_click_to_open_file_chooser(
                page,
                video_path,
                cfg.id,
                cfg.click_selectors,
                cfg.click_text_markers,
            )
            .await
            {
                Ok(click_result) => {
                    upload_action_performed = true;
                    wechat_file_set_success = wechat_file_set_success || click_result.file_set;
                    wechat_chooser_event_state = click_result.chooser_event_state.clone();
                    wechat_click_round = wechat_click_round.max(click_result.click_round);
                    wechat_click_method = click_result.click_method.clone();
                    upload_diagnostics.push(format!(
                        "D:round={} clicked={} chooser_opened={} chooser_event_state={} click_method={} click_round={} clicked_context={} signal_source={}",
                        round,
                        click_result.marker,
                        click_result.chooser_opened,
                        click_result.chooser_event_state,
                        click_result.click_method,
                        click_result.click_round,
                        if click_result.clicked_context.is_empty() {
                            "none"
                        } else {
                            &click_result.clicked_context
                        },
                        click_result.signal_source
                    ));
                    if let Some(signal) = wait_for_upload_signal(page, cfg, FAST_SIGNAL_TIMEOUT_SECS).await {
                        upload_signal = Some(signal.clone());
                        upload_diagnostics.push(format!("D:round={} signal={}", round, signal));
                        break;
                    }
                    upload_diagnostics.push(format!(
                        "D:round={} no_signal_fast({}s)",
                        round, FAST_SIGNAL_TIMEOUT_SECS
                    ));
                }
                Err(e) => {
                    upload_diagnostics.push(format!("D:round={} failed={}", round, e));
                }
            }
        }

        upload_diagnostics.push(format!(
            "D:summary chooser_event_state={} click_round={} click_method={} file_set_success={}",
            wechat_chooser_event_state, wechat_click_round, wechat_click_method, wechat_file_set_success
        ));
    }

    if upload_signal.is_none() && cfg.id != "wechat" {
        info!("[{}上传] 策略B失败，尝试策略C：拖拽上传...", cfg.name);
        match automation::upload_file_via_drag_drop(page, video_path, cfg.id, cfg.drop_zone_selectors)
            .await
        {
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

    if upload_signal.is_none() && cfg.id != "wechat" {
        info!("[{}上传] 策略C后仍未触发，尝试策略D：点击上传入口...", cfg.name);
        if !cfg.pre_click_selectors.is_empty() {
            match automation::click_first_visible(page, cfg.pre_click_selectors).await {
                Ok(marker) => {
                    upload_diagnostics.push(format!("D:pre_click={}", marker));
                    tokio::time::sleep(Duration::from_millis(PRE_CLICK_WAIT_MS)).await;
                }
                Err(e) => {
                    upload_diagnostics.push(format!("D:pre_click_failed={}", e));
                }
            }
        }

        match automation::upload_file_via_click_to_open_file_chooser(
            page,
            video_path,
            cfg.id,
            cfg.click_selectors,
            cfg.click_text_markers,
        )
        .await {
            Ok(click_result) => {
                upload_action_performed = true;
                upload_diagnostics.push(format!(
                    "D:clicked={} chooser_opened={} chooser_event_state={} click_method={} click_round={} clicked_context={} signal_source={}",
                    click_result.marker,
                    click_result.chooser_opened,
                    click_result.chooser_event_state,
                    click_result.click_method,
                    click_result.click_round,
                    if click_result.clicked_context.is_empty() {
                        "none"
                    } else {
                        &click_result.clicked_context
                    },
                    click_result.signal_source
                ));
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

    if upload_signal.is_none() && cfg.id == "wechat" && !wechat_file_set_success {
        info!("[{}上传] 微信策略D后仍未触发，回退尝试策略C：拖拽上传...", cfg.name);
        match automation::upload_file_via_drag_drop(page, video_path, cfg.id, cfg.drop_zone_selectors)
            .await
        {
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
    } else if upload_signal.is_none() && cfg.id == "wechat" && wechat_file_set_success {
        upload_diagnostics.push(
            "D:file_set_success skip_drag_drop_waiting_for_signal_confirmation".to_string(),
        );
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

    if upload_signal.is_none() && cfg.id == "wechat" && wechat_file_set_success {
        upload_diagnostics.push("fallback:signal=chooser:file_set".to_string());
        upload_signal = Some("chooser:file_set".to_string());
    }

    let started_signal = match upload_signal {
        Some(signal) => signal,
        None => {
            if cfg.id == "wechat" {
                bail!(
                    "WECHAT_UPLOAD_SIGNAL_TIMEOUT: [{}上传] 已执行上传动作，但在快速检测与兜底检测中都未检测到上传信号。diagnosis: chooser_event_state={} click_round={} click_method={} 诊断：{}",
                    cfg.name,
                    wechat_chooser_event_state,
                    wechat_click_round,
                    wechat_click_method,
                    upload_diagnostics.join(" | ")
                );
            }
            bail!(
                "[{}上传] 已执行上传动作，但在快速检测与兜底检测中都未检测到上传信号。诊断：{}",
                cfg.name,
                upload_diagnostics.join(" | ")
            );
        }
    };
    let signal_source = upload_signal_source(&started_signal);
    info!(
        "[{}上传] 上传信号确认：signal={} signal_source={} chooser_event_state={} click_round={} click_method={}",
        cfg.name,
        started_signal,
        signal_source,
        wechat_chooser_event_state,
        wechat_click_round,
        wechat_click_method
    );

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
        if cfg.fill_failure_is_error {
            bail!(
                "[{}填表] 上传已触发（signal={}），但标题和描述均未命中可编辑字段。诊断：{}",
                cfg.name,
                started_signal,
                upload_diagnostics.join(" | ")
            );
        }
        warn!(
            "[{}填表] 上传已触发（signal={}），但标题和描述均未命中可编辑字段。已按平台策略降级为非阻断。诊断：{}",
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
    let is_wechat = cfg.id == "wechat";

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

    let timeout = Duration::from_secs(if is_wechat {
        WECHAT_GUARD_TIMEOUT_SECS
    } else {
        15
    });
    let start = std::time::Instant::now();
    let mut last_url = before_url;
    let mut weak_ready_self_heal_attempted = false;
    loop {
        let host_ok = last_url.contains(cfg.target_host);
        let path_ok = path_allowed(&last_url, cfg.allowed_paths);
        let probe = probe_upload_page(page, cfg).await;
        let surface_ok = if is_wechat {
            wechat_upload_ready(&probe)
        } else {
            has_upload_surface(page, cfg).await
        };
        let (weak_ready, weak_ready_reason) = compute_weak_ready(surface_ok, &probe, cfg);
        let fingerprint = format_probe_fingerprint(&probe);
        let login_url_hit = is_wechat && is_wechat_login_url(&last_url);

        if is_wechat && (login_url_hit || !probe.login_text_hit.is_empty()) {
            bail!(
                "LOGIN_REQUIRED: {} 上传页需要登录。当前URL={} login_url_hit={} login_text_hit={} ready_kind={} weak_ready_reason={} self_heal_attempted={} fingerprint={}",
                cfg.name,
                last_url,
                login_url_hit,
                if probe.login_text_hit.is_empty() {
                    "none"
                } else {
                    &probe.login_text_hit
                },
                probe.ready_kind,
                weak_ready_reason,
                weak_ready_self_heal_attempted,
                fingerprint
            );
        }

        if host_ok && path_ok && !probe.blocked_text_hit.is_empty() {
            bail!(
                "TARGET_PAGE_NOT_READY: {} 上传页命中拦截文案。当前URL={}（期望 host={} path={:?}） triad(host_ok={} path_ok={} surface_ok={}) ready_kind={} weak_ready_reason={} self_heal_attempted={} fingerprint={}",
                cfg.name,
                last_url,
                cfg.target_host,
                cfg.allowed_paths,
                host_ok,
                path_ok,
                surface_ok,
                probe.ready_kind,
                weak_ready_reason,
                weak_ready_self_heal_attempted,
                fingerprint
            );
        }

        if host_ok && path_ok && surface_ok {
            let interactive_ready_ms = start.elapsed().as_millis();
            info!(
                "[{}上传] 页面守卫通过：{} ready_kind={} interactive_candidate_count={} interactive_ready_ms={} interactive_context={} fingerprint={}",
                cfg.name,
                last_url,
                probe.ready_kind,
                probe.interactive_candidate_count,
                interactive_ready_ms,
                if probe.interactive_context.is_empty() {
                    "none"
                } else {
                    &probe.interactive_context
                },
                fingerprint
            );
            return Ok(());
        }

        if is_wechat && host_ok && path_ok {
            if !probe.init_text_hit.is_empty() {
                if start.elapsed() > timeout {
                    bail!(
                        "TARGET_PAGE_NOT_READY: {} 上传页仍在初始化。当前URL={}（期望 host={} path={:?}） triad(host_ok={} path_ok={} surface_ok={}) ready_kind={} weak_ready_reason={} self_heal_attempted={} fingerprint={}",
                        cfg.name,
                        last_url,
                        cfg.target_host,
                        cfg.allowed_paths,
                        host_ok,
                        path_ok,
                        surface_ok,
                        probe.ready_kind,
                        weak_ready_reason,
                        weak_ready_self_heal_attempted,
                        fingerprint
                    );
                }
                tokio::time::sleep(Duration::from_millis(FAST_POLL_INTERVAL_MS)).await;
                last_url = current_url(page).await;
                continue;
            }

            if weak_ready_reason == "wechat_empty_dom"
                && cfg.weak_ready_self_heal
                && !weak_ready_self_heal_attempted
            {
                weak_ready_self_heal_attempted = true;
                warn!(
                    "[{}上传] 微信页面空壳，触发自动自愈（reason={} url={} ready_kind={} triad(host_ok={} path_ok={} surface_ok={}) fingerprint={}）",
                    cfg.name,
                    weak_ready_reason,
                    last_url,
                    probe.ready_kind,
                    host_ok,
                    path_ok,
                    surface_ok,
                    fingerprint
                );
                let healed = self_heal_weak_ready_page(page, cfg).await;
                let after_heal_probe = probe_upload_page(page, cfg).await;
                let after_heal_fingerprint = format_probe_fingerprint(&after_heal_probe);
                warn!(
                    "[{}上传] 微信页面自愈结果：healed={} ready_kind={} fingerprint={}",
                    cfg.name, healed, after_heal_probe.ready_kind, after_heal_fingerprint
                );
                last_url = current_url(page).await;
                continue;
            }

            if start.elapsed() > timeout {
                bail!(
                    "TARGET_PAGE_NOT_READY: {} 上传页超时仍未达可上传状态。当前URL={}（期望 host={} path={:?}） triad(host_ok={} path_ok={} surface_ok={}) ready_kind={} weak_ready_reason={} self_heal_attempted={} fingerprint={}",
                    cfg.name,
                    last_url,
                    cfg.target_host,
                    cfg.allowed_paths,
                    host_ok,
                    path_ok,
                    surface_ok,
                    probe.ready_kind,
                    weak_ready_reason,
                    weak_ready_self_heal_attempted,
                    fingerprint
                );
            }

            tokio::time::sleep(Duration::from_millis(FAST_POLL_INTERVAL_MS)).await;
            last_url = current_url(page).await;
            continue;
        }

        if host_ok && path_ok && !surface_ok && !cfg.require_surface_ready {
            if weak_ready {
                if cfg.weak_ready_self_heal && !weak_ready_self_heal_attempted {
                    weak_ready_self_heal_attempted = true;
                    warn!(
                        "[{}上传] 页面弱就绪，触发自动自愈（reason={} url={} triad(host_ok={} path_ok={} surface_ok={}) fingerprint={}）",
                        cfg.name,
                        weak_ready_reason,
                        last_url,
                        host_ok,
                        path_ok,
                        surface_ok,
                        fingerprint
                    );
                    let healed = self_heal_weak_ready_page(page, cfg).await;
                    let after_heal_probe = probe_upload_page(page, cfg).await;
                    let after_heal_fingerprint = format_probe_fingerprint(&after_heal_probe);
                    warn!(
                        "[{}上传] 页面弱就绪自动自愈结果：healed={} fingerprint={}",
                        cfg.name, healed, after_heal_fingerprint
                    );
                    last_url = current_url(page).await;
                    continue;
                }
                bail!(
                    "TARGET_PAGE_NOT_READY: {} 上传页弱就绪。当前URL={}（期望 host={} path={:?}） triad(host_ok={} path_ok={} surface_ok={}) ready_kind={} weak_ready_reason={} self_heal_attempted={} fingerprint={}",
                    cfg.name,
                    last_url,
                    cfg.target_host,
                    cfg.allowed_paths,
                    host_ok,
                    path_ok,
                    surface_ok,
                    probe.ready_kind,
                    weak_ready_reason,
                    weak_ready_self_heal_attempted,
                    fingerprint
                );
            }
            warn!(
                "[{}上传] 页面守卫降级放行：surface_ok=false but continue（host_ok={} path_ok={} surface_ok={} require_surface_ready={} url={} fingerprint={}）",
                cfg.name,
                host_ok,
                path_ok,
                surface_ok,
                cfg.require_surface_ready,
                last_url,
                fingerprint
            );
            return Ok(());
        }

        if start.elapsed() > timeout {
            bail!(
                "TARGET_PAGE_NOT_READY: {} 上传页未就绪。当前URL={}（期望 host={} path={:?}） triad(host_ok={} path_ok={} surface_ok={}) ready_kind={} weak_ready_reason={} self_heal_attempted={} fingerprint={}",
                cfg.name,
                last_url,
                cfg.target_host,
                cfg.allowed_paths,
                host_ok,
                path_ok,
                surface_ok,
                probe.ready_kind,
                weak_ready_reason,
                weak_ready_self_heal_attempted,
                fingerprint
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

async fn wait_for_wechat_interactive_ready(
    page: &Page,
    cfg: &PlatformPublishConfig,
    timeout_secs: u64,
) -> Option<UploadPageProbe> {
    if cfg.id != "wechat" {
        return None;
    }
    let timeout = Duration::from_secs(timeout_secs);
    let start = std::time::Instant::now();
    loop {
        let probe = probe_upload_page(page, cfg).await;
        if wechat_upload_ready(&probe) {
            return Some(probe);
        }
        if start.elapsed() > timeout {
            return None;
        }
        tokio::time::sleep(Duration::from_millis(FAST_POLL_INTERVAL_MS)).await;
    }
}

fn wechat_upload_ready(probe: &UploadPageProbe) -> bool {
    probe.guard_state == "ready" && probe.interactive_candidate_count > 0
}

fn upload_signal_source(signal: &str) -> &'static str {
    if signal.starts_with("url:") {
        "url"
    } else if signal.starts_with("file:selected") {
        "file_input"
    } else if signal.starts_with("progress:") {
        "progress"
    } else if signal.starts_with("text:") {
        "text"
    } else if signal.starts_with("chooser:") {
        "chooser_file_set"
    } else {
        "unknown"
    }
}

async fn has_upload_surface(page: &Page, cfg: &PlatformPublishConfig) -> bool {
    if cfg.id == "wechat" {
        return wechat_upload_ready(&probe_upload_page(page, cfg).await);
    }

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

async fn probe_upload_page(
    page: &Page,
    cfg: &PlatformPublishConfig,
) -> UploadPageProbe {
    let blocked_markers = js_array(cfg.blocked_text_markers);
    let init_markers = js_array(cfg.init_text_markers);
    let login_markers = js_array(cfg.login_text_markers);
    let surface_markers = js_array(cfg.surface_text_markers);
    let surface_selectors = js_array(cfg.surface_selectors);
    let js = if cfg.id == "wechat" {
        r#"
        (function(surfaceSelectors, surfaceMarkers, blockedMarkers, initMarkers, loginMarkers) {
            const normalize = (value) => (value || '').replace(/\s+/g, ' ').trim();
            const maxFrameDepth = 3;
            const maxShadowDepth = 4;

            let frameCount = 0;
            let shadowRootCount = 0;
            let scannedNodes = 0;
            let fileInputCount = 0;
            let surfaceSelectorHitCount = 0;
            let surfaceTextHit = '';
            let blockedTextHit = '';
            let initTextHit = '';
            let loginTextHit = '';
            let surfaceContextHit = '';
            let bodyTextLen = 0;
            let bodyExcerpt = '';
            let interactiveCandidateCount = 0;
            let interactiveContext = '';

            const title = normalize(document.title || '').slice(0, 80);

            function markSurfaceContext(kind, context) {
                if (!surfaceContextHit) {
                    surfaceContextHit = kind + '@' + context;
                }
            }

            function markInteractive(context, reason) {
                interactiveCandidateCount += 1;
                if (!interactiveContext) {
                    interactiveContext = context + '|' + reason;
                }
            }

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

            function scanText(text, context) {
                const normalized = normalize(text);
                if (!normalized) return;

                if (normalized.length > bodyTextLen) {
                    bodyTextLen = normalized.length;
                    if (!bodyExcerpt) {
                        bodyExcerpt = normalized.slice(0, 120);
                    }
                }

                if (!blockedTextHit) {
                    for (const marker of blockedMarkers || []) {
                        if (marker && normalized.includes(marker)) {
                            blockedTextHit = marker;
                            break;
                        }
                    }
                }

                if (!loginTextHit) {
                    for (const marker of loginMarkers || []) {
                        if (marker && normalized.includes(marker)) {
                            loginTextHit = marker;
                            break;
                        }
                    }
                }

                if (!initTextHit) {
                    for (const marker of initMarkers || []) {
                        if (marker && normalized.includes(marker)) {
                            initTextHit = marker;
                            break;
                        }
                    }
                }

                if (!surfaceTextHit) {
                    for (const marker of surfaceMarkers || []) {
                        if (marker && normalized.includes(marker)) {
                            surfaceTextHit = marker;
                            markSurfaceContext('text', context);
                            break;
                        }
                    }
                }
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
                scannedNodes += nodes.length;

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
            frameCount = frameContexts.length;

            for (const frameCtx of frameContexts) {
                const roots = [];
                collectRoots(frameCtx.doc, frameCtx.framePath, '', 0, roots);
                for (const rootCtx of roots) {
                    let text = '';
                    try {
                        text = rootCtx.root.body
                            ? (rootCtx.root.body.innerText || '')
                            : (rootCtx.root.textContent || '');
                    } catch (_) {
                        text = '';
                    }
                    scanText(text, rootCtx.context);

                    let fileInputs = [];
                    try {
                        fileInputs = Array.from(rootCtx.root.querySelectorAll("input[type='file']"));
                    } catch (_) {
                        fileInputs = [];
                    }
                    if (fileInputs.length > 0) {
                        fileInputCount += fileInputs.length;
                        markSurfaceContext('file_input', rootCtx.context);
                        for (const input of fileInputs) {
                            if (!isVisible(input)) continue;
                            markInteractive(rootCtx.context, 'file_input');
                        }
                    }

                    for (const sel of surfaceSelectors || []) {
                        let nodes = [];
                        try {
                            nodes = Array.from(rootCtx.root.querySelectorAll(sel));
                        } catch (_) {
                            nodes = [];
                        }
                        if (nodes.length > 0) {
                            surfaceSelectorHitCount += nodes.length;
                            markSurfaceContext('selector:' + sel, rootCtx.context);
                            for (const node of nodes) {
                                if (!isVisible(node)) continue;
                                const clickable = isClickable(node) ? node : findClickableAncestor(node);
                                if (!clickable || !isVisible(clickable)) continue;
                                markInteractive(rootCtx.context, 'selector:' + sel);
                                break;
                            }
                        }
                    }
                }
            }

            const anchorHit = fileInputCount > 0 || surfaceSelectorHitCount > 0 || !!surfaceTextHit;
            let readyKind = 'wechat_no_anchor_but_dom_present';
            if (blockedTextHit) {
                readyKind = 'blocked';
            } else if (loginTextHit) {
                readyKind = 'wechat_login_required';
            } else if (initTextHit) {
                readyKind = 'wechat_init_pending';
            } else if (anchorHit && interactiveCandidateCount > 0) {
                readyKind = 'wechat_interactive_ready';
            } else if (anchorHit) {
                readyKind = 'wechat_anchor_no_interactive';
            } else if (scannedNodes === 0 && bodyTextLen === 0) {
                readyKind = 'wechat_empty_dom';
            }
            const guardState = blockedTextHit
                ? 'blocked'
                : (loginTextHit
                    ? 'login_required'
                    : (initTextHit
                        ? 'init_pending'
                        : (anchorHit ? 'ready' : 'pending')));

            return JSON.stringify({
                title,
                body_text_len: bodyTextLen,
                body_excerpt: bodyExcerpt,
                file_input_count: fileInputCount,
                blocked_text_hit: blockedTextHit,
                init_text_hit: initTextHit,
                login_text_hit: loginTextHit,
                surface_text_hit: surfaceTextHit,
                anchor_hit: anchorHit,
                surface_selector_hit_count: surfaceSelectorHitCount,
                surface_context_hit: surfaceContextHit,
                frame_count: frameCount,
                shadow_root_count: shadowRootCount,
                scanned_nodes: scannedNodes,
                interactive_candidate_count: interactiveCandidateCount,
                interactive_context: interactiveContext,
                guard_state: guardState,
                ready_kind: readyKind
            });
        })(__SURFACE_SELECTORS__, __SURFACE_MARKERS__, __BLOCKED_MARKERS__, __INIT_MARKERS__, __LOGIN_MARKERS__)
        "#
        .replace("__SURFACE_SELECTORS__", format!("[{}]", surface_selectors).as_str())
        .replace("__SURFACE_MARKERS__", format!("[{}]", surface_markers).as_str())
        .replace("__BLOCKED_MARKERS__", format!("[{}]", blocked_markers).as_str())
        .replace("__INIT_MARKERS__", format!("[{}]", init_markers).as_str())
        .replace("__LOGIN_MARKERS__", format!("[{}]", login_markers).as_str())
    } else {
        r#"
        (function(markers, initMarkers, loginMarkers, surfaceMarkers, surfaceSelectors) {
            const normalize = (value) => (value || '').replace(/\s+/g, ' ').trim();
            const title = normalize(document.title || '').slice(0, 80);
            const bodyText = document.body ? normalize(document.body.innerText || '') : '';
            const fileInputCount = document.querySelectorAll("input[type='file']").length;
            let blockedTextHit = '';
            let initTextHit = '';
            let loginTextHit = '';
            let surfaceTextHit = '';
            let surfaceSelectorHitCount = 0;

            for (const marker of markers || []) {
                if (marker && bodyText.includes(marker)) {
                    blockedTextHit = marker;
                    break;
                }
            }
            for (const marker of initMarkers || []) {
                if (marker && bodyText.includes(marker)) {
                    initTextHit = marker;
                    break;
                }
            }
            for (const marker of loginMarkers || []) {
                if (marker && bodyText.includes(marker)) {
                    loginTextHit = marker;
                    break;
                }
            }
            for (const marker of surfaceMarkers || []) {
                if (marker && bodyText.includes(marker)) {
                    surfaceTextHit = marker;
                    break;
                }
            }
            for (const sel of surfaceSelectors || []) {
                try {
                    surfaceSelectorHitCount += document.querySelectorAll(sel).length;
                } catch (_) {}
            }

            const anchorHit = fileInputCount > 0 || !!surfaceTextHit || surfaceSelectorHitCount > 0;
            let readyKind = 'none';
            if (blockedTextHit) {
                readyKind = 'blocked';
            } else if (loginTextHit) {
                readyKind = 'login_required';
            } else if (initTextHit) {
                readyKind = 'init_pending';
            } else if (anchorHit) {
                readyKind = 'anchor_ready';
            } else if (bodyText.length === 0) {
                readyKind = 'empty_dom';
            } else {
                readyKind = 'anchor_miss';
            }
            const guardState = blockedTextHit
                ? 'blocked'
                : (loginTextHit
                    ? 'login_required'
                    : (initTextHit ? 'init_pending' : 'none'));

            return JSON.stringify({
                title,
                body_text_len: bodyText.length,
                body_excerpt: bodyText.slice(0, 120),
                file_input_count: fileInputCount,
                blocked_text_hit: blockedTextHit,
                init_text_hit: initTextHit,
                login_text_hit: loginTextHit,
                surface_text_hit: surfaceTextHit,
                anchor_hit: anchorHit,
                surface_selector_hit_count: surfaceSelectorHitCount,
                surface_context_hit: anchorHit ? 'frame:top' : '',
                frame_count: 1,
                shadow_root_count: 0,
                scanned_nodes: 0,
                interactive_candidate_count: 0,
                interactive_context: '',
                guard_state: guardState,
                ready_kind: readyKind
            });
        })(__BLOCKED_MARKERS__, __INIT_MARKERS__, __LOGIN_MARKERS__, __SURFACE_MARKERS__, __SURFACE_SELECTORS__)
        "#
        .replace("__BLOCKED_MARKERS__", format!("[{}]", blocked_markers).as_str())
        .replace("__INIT_MARKERS__", format!("[{}]", init_markers).as_str())
        .replace("__LOGIN_MARKERS__", format!("[{}]", login_markers).as_str())
        .replace("__SURFACE_MARKERS__", format!("[{}]", surface_markers).as_str())
        .replace("__SURFACE_SELECTORS__", format!("[{}]", surface_selectors).as_str())
    };

    let raw: String = page
        .evaluate(js.as_str())
        .await
        .map(|v| v.into_value().unwrap_or_else(|_| "{}".to_string()))
        .unwrap_or_else(|_| "{}".to_string());
    let parsed: serde_json::Value = serde_json::from_str(&raw).unwrap_or_else(|_| serde_json::json!({}));

    UploadPageProbe {
        title: parsed
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        body_text_len: parsed
            .get("body_text_len")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize,
        body_excerpt: parsed
            .get("body_excerpt")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        file_input_count: parsed
            .get("file_input_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize,
        blocked_text_hit: parsed
            .get("blocked_text_hit")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        init_text_hit: parsed
            .get("init_text_hit")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        login_text_hit: parsed
            .get("login_text_hit")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        surface_text_hit: parsed
            .get("surface_text_hit")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        anchor_hit: parsed
            .get("anchor_hit")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        surface_selector_hit_count: parsed
            .get("surface_selector_hit_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize,
        surface_context_hit: parsed
            .get("surface_context_hit")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        frame_count: parsed
            .get("frame_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(1) as usize,
        shadow_root_count: parsed
            .get("shadow_root_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize,
        scanned_nodes: parsed
            .get("scanned_nodes")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize,
        interactive_candidate_count: parsed
            .get("interactive_candidate_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize,
        interactive_context: parsed
            .get("interactive_context")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        guard_state: parsed
            .get("guard_state")
            .and_then(|v| v.as_str())
            .unwrap_or("none")
            .to_string(),
        ready_kind: parsed
            .get("ready_kind")
            .and_then(|v| v.as_str())
            .unwrap_or("none")
            .to_string(),
    }
}

fn format_probe_fingerprint(probe: &UploadPageProbe) -> String {
    format!(
        "title={};body_text_len={};body_excerpt={};file_input_count={};blocked_text_hit={};init_text_hit={};login_text_hit={};surface_text_hit={};anchor_hit={};surface_selector_hit_count={};surface_context_hit={};frame_count={};shadow_root_count={};scanned_nodes={};interactive_candidate_count={};interactive_context={};guard_state={};ready_kind={}",
        probe.title,
        probe.body_text_len,
        probe.body_excerpt,
        probe.file_input_count,
        if probe.blocked_text_hit.is_empty() {
            "none"
        } else {
            &probe.blocked_text_hit
        },
        if probe.init_text_hit.is_empty() {
            "none"
        } else {
            &probe.init_text_hit
        },
        if probe.login_text_hit.is_empty() {
            "none"
        } else {
            &probe.login_text_hit
        },
        if probe.surface_text_hit.is_empty() {
            "none"
        } else {
            &probe.surface_text_hit
        },
        probe.anchor_hit,
        probe.surface_selector_hit_count,
        if probe.surface_context_hit.is_empty() {
            "none"
        } else {
            &probe.surface_context_hit
        },
        probe.frame_count,
        probe.shadow_root_count,
        probe.scanned_nodes,
        probe.interactive_candidate_count,
        if probe.interactive_context.is_empty() {
            "none"
        } else {
            &probe.interactive_context
        },
        probe.guard_state,
        probe.ready_kind
    )
}

fn compute_weak_ready(
    surface_ok: bool,
    probe: &UploadPageProbe,
    cfg: &PlatformPublishConfig,
) -> (bool, String) {
    if !probe.blocked_text_hit.is_empty() {
        return (
            true,
            format!("blocked_text_hit:{}", probe.blocked_text_hit),
        );
    }

    if cfg.id == "wechat" {
        if !probe.login_text_hit.is_empty() {
            return (
                true,
                format!("wechat_login_required:{}", probe.login_text_hit),
            );
        }
        if !probe.init_text_hit.is_empty() {
            return (
                true,
                format!("wechat_init_pending:{}", probe.init_text_hit),
            );
        }
        if wechat_upload_ready(probe) {
            return (false, "wechat_interactive_ready".to_string());
        }
        if probe.anchor_hit {
            return (true, "wechat_anchor_no_interactive".to_string());
        }
        if probe.scanned_nodes == 0 && probe.body_text_len == 0 {
            return (true, "wechat_empty_dom".to_string());
        }
        return (false, "wechat_no_anchor_but_dom_present".to_string());
    }

    if cfg.weak_ready_min_body_text_len > 0 && probe.body_text_len < cfg.weak_ready_min_body_text_len {
        return (
            true,
            format!(
                "body_text_len_too_short:{}<{}",
                probe.body_text_len, cfg.weak_ready_min_body_text_len
            ),
        );
    }

    if !surface_ok && probe.file_input_count == 0 && probe.body_text_len == 0 {
        return (true, "surface_missing_and_empty_dom".to_string());
    }

    (false, "none".to_string())
}

async fn self_heal_weak_ready_page(page: &Page, cfg: &PlatformPublishConfig) -> bool {
    let replace_js = format!(
        "(function() {{ try {{ window.location.replace('{}'); return 'ok'; }} catch (_) {{ return 'error'; }} }})()",
        escape_js_single(cfg.upload_url)
    );
    let _replace_result: String = page
        .evaluate(replace_js.as_str())
        .await
        .map(|v| v.into_value().unwrap_or_else(|_| "error".to_string()))
        .unwrap_or_else(|_| "error".to_string());

    tokio::time::sleep(Duration::from_millis(WEAK_READY_RELOAD_WAIT_MS)).await;

    let _reload_result: String = page
        .evaluate(
            "(function() { try { window.location.reload(); return 'ok'; } catch (_) { return 'error'; } })()",
        )
        .await
        .map(|v| v.into_value().unwrap_or_else(|_| "error".to_string()))
        .unwrap_or_else(|_| "error".to_string());

    let timeout = Duration::from_secs(WEAK_READY_SELF_HEAL_TIMEOUT_SECS);
    let start = std::time::Instant::now();
    while start.elapsed() <= timeout {
        tokio::time::sleep(Duration::from_millis(FAST_POLL_INTERVAL_MS)).await;
        let probe = probe_upload_page(page, cfg).await;
        if !probe.blocked_text_hit.is_empty() {
            return false;
        }
        if cfg.id == "wechat" {
            if wechat_upload_ready(&probe) {
                return true;
            }
            continue;
        }

        let surface_ok = has_upload_surface(page, cfg).await;
        if surface_ok
            || (cfg.weak_ready_min_body_text_len > 0
                && probe.body_text_len >= cfg.weak_ready_min_body_text_len)
        {
            return true;
        }
    }
    false
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

fn is_wechat_login_url(url: &str) -> bool {
    if !url.contains("channels.weixin.qq.com") {
        return false;
    }
    url.contains("/login") || url.contains("login.weixin.qq.com") || url.contains("scanlogin")
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
