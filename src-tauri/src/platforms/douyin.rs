use super::traits::PlatformInfo;
use crate::browser::automation;
use chromiumoxide::page::Page;
use anyhow::{Result, Context};
use log::info;

pub fn info() -> PlatformInfo {
    PlatformInfo {
        id: "douyin".into(),
        name: "抖音".into(),
        name_en: "Douyin".into(),
        login_url: "https://creator.douyin.com".into(),
        upload_url: "https://creator.douyin.com/creator-micro/content/upload".into(),
        color: "#fe2c55".into(),
    }
}

/// Automate the full publish flow on Douyin
pub async fn auto_publish(
    page: &Page,
    video_path: &str,
    title: &str,
    description: &str,
    tags: &[String],
) -> Result<()> {
    info!("Starting Douyin auto-publish for: {}", video_path);

    // Step 1: Wait for the upload page to load
    info!("Step 1: Waiting for upload page...");
    automation::wait(3.0).await;

    // Step 2: Upload the video file
    info!("Step 2: Uploading video file...");
    automation::upload_file(page, video_path).await
        .context("Failed to upload video to Douyin")?;

    // Step 3: Wait for upload to complete and edit page to appear
    info!("Step 3: Waiting for upload to complete...");
    wait_for_upload_complete(page).await?;

    // Step 4: Fill in title
    info!("Step 4: Filling title: {}", title);
    automation::wait(1.0).await;
    fill_douyin_title(page, title).await?;

    // Step 5: Fill description
    if !description.is_empty() {
        info!("Step 5: Filling description...");
        fill_douyin_description(page, description).await?;
    }

    // Step 6: Add tags
    if !tags.is_empty() {
        info!("Step 6: Adding tags...");
        for tag in tags {
            add_douyin_tag(page, tag).await.ok();
        }
    }

    info!("Douyin auto-publish complete. Waiting for user confirmation.");
    Ok(())
}

async fn wait_for_upload_complete(page: &Page) -> Result<()> {
    let timeout_secs = 300u64;
    let start = std::time::Instant::now();
    let timeout = std::time::Duration::from_secs(timeout_secs);

    loop {
        if start.elapsed() > timeout {
            info!("Upload timeout, continuing...");
            break;
        }

        let js = r#"
            (function() {
                const titleEl = document.querySelector('input[placeholder*="标题"], [class*="title"] input, [contenteditable="true"]');
                if (titleEl) return 'ready';
                const progress = document.querySelector('[class*="progress"]');
                if (progress) return 'uploading:' + (progress.textContent || '');
                return 'waiting';
            })()
        "#;

        let status: String = page.evaluate(js)
            .await
            .map(|v| v.into_value().unwrap_or_else(|_| "error".into()))
            .unwrap_or_else(|_| "error".into());

        if status == "ready" {
            info!("Upload complete, edit page ready");
            automation::wait(2.0).await;
            return Ok(());
        }

        if status.starts_with("uploading") {
            info!("Uploading: {}", status);
        }

        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
    Ok(())
}

async fn fill_douyin_title(page: &Page, title: &str) -> Result<()> {
    let escaped = title.replace('\\', "\\\\").replace('\'', "\\'").replace('\n', "");
    let js = format!(
        r#"
        (function() {{
            const selectors = [
                'input[placeholder*="标题"]',
                'input[placeholder*="title"]',
                '.title-input input',
                '[class*="title"] input[type="text"]',
            ];
            for (const sel of selectors) {{
                const el = document.querySelector(sel);
                if (el) {{
                    el.focus();
                    el.value = '{}';
                    el.dispatchEvent(new Event('input', {{ bubbles: true }}));
                    el.dispatchEvent(new Event('change', {{ bubbles: true }}));
                    return 'input:' + sel;
                }}
            }}
            const editables = document.querySelectorAll('[contenteditable="true"]');
            for (const el of editables) {{
                const rect = el.getBoundingClientRect();
                if (rect.top < 400 && rect.height < 100) {{
                    el.focus();
                    el.textContent = '{}';
                    el.dispatchEvent(new Event('input', {{ bubbles: true }}));
                    return 'editable';
                }}
            }}
            return 'not_found';
        }})()
        "#,
        escaped, escaped,
    );

    let result: String = page.evaluate(js.as_str())
        .await
        .map(|v| v.into_value().unwrap_or_else(|_| "error".into()))
        .unwrap_or_else(|_| "error".into());

    info!("Title fill result: {}", result);
    Ok(())
}

async fn fill_douyin_description(page: &Page, description: &str) -> Result<()> {
    let escaped = description.replace('\\', "\\\\").replace('\'', "\\'").replace('\n', "\\n");
    let js = format!(
        r#"
        (function() {{
            const selectors = [
                'textarea[placeholder*="描述"]',
                'textarea[placeholder*="简介"]',
                '[class*="desc"] textarea',
            ];
            for (const sel of selectors) {{
                const el = document.querySelector(sel);
                if (el) {{
                    el.focus();
                    el.value = '{}';
                    el.dispatchEvent(new Event('input', {{ bubbles: true }}));
                    return 'textarea:' + sel;
                }}
            }}
            const editables = document.querySelectorAll('[contenteditable="true"]');
            let descEl = null;
            for (const el of editables) {{
                const rect = el.getBoundingClientRect();
                if (rect.height > 80 || rect.top > 300) {{
                    descEl = el;
                    break;
                }}
            }}
            if (descEl) {{
                descEl.focus();
                descEl.textContent = '{}';
                descEl.dispatchEvent(new Event('input', {{ bubbles: true }}));
                return 'editable';
            }}
            return 'not_found';
        }})()
        "#,
        escaped, escaped,
    );

    let result: String = page.evaluate(js.as_str())
        .await
        .map(|v| v.into_value().unwrap_or_else(|_| "error".into()))
        .unwrap_or_else(|_| "error".into());

    info!("Description fill result: {}", result);
    Ok(())
}

async fn add_douyin_tag(page: &Page, tag: &str) -> Result<()> {
    let escaped = tag.replace('\\', "\\\\").replace('\'', "\\'");
    let js = format!(
        r#"
        (function() {{
            const selectors = [
                'input[placeholder*="标签"]',
                'input[placeholder*="话题"]',
                '[class*="tag"] input',
                '[class*="topic"] input',
            ];
            for (const sel of selectors) {{
                const el = document.querySelector(sel);
                if (el) {{
                    el.focus();
                    el.value = '{}';
                    el.dispatchEvent(new Event('input', {{ bubbles: true }}));
                    el.dispatchEvent(new KeyboardEvent('keydown', {{ key: 'Enter', keyCode: 13, bubbles: true }}));
                    return 'added';
                }}
            }}
            return 'not_found';
        }})()
        "#,
        escaped,
    );

    page.evaluate(js.as_str()).await.ok();
    automation::wait(0.5).await;
    Ok(())
}
