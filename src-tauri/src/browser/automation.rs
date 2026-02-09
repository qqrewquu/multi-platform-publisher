use anyhow::{Context, Result, bail};
use chromiumoxide::browser::Browser;
use chromiumoxide::page::Page;
use chromiumoxide::cdp::browser_protocol::dom::{
    GetDocumentParams, QuerySelectorParams, SetFileInputFilesParams,
};
use futures::StreamExt;
use log::info;
use std::time::Duration;

/// Connect to an already-running Chrome instance via CDP
pub async fn connect_to_chrome(port: u16) -> Result<(Browser, Page)> {
    let debug_url = format!("http://127.0.0.1:{}", port);

    let (browser, mut handler) = Browser::connect(&debug_url)
        .await
        .context(format!("Failed to connect to Chrome on port {}", port))?;

    // Spawn the handler to process CDP events
    tokio::spawn(async move {
        while let Some(_event) = handler.next().await {}
    });

    // Get the first page
    let pages = browser.pages().await.context("Failed to get pages")?;
    let page = pages.into_iter().next()
        .context("No pages found in Chrome")?;

    info!("Connected to Chrome CDP on port {}", port);
    Ok((browser, page))
}

/// Upload a file by finding the file input and using CDP DOM.setFileInputFiles
pub async fn upload_file(page: &Page, file_path: &str) -> Result<()> {
    // Make file inputs visible
    let js_make_visible = r#"
        (function() {
            const inputs = document.querySelectorAll('input[type="file"]');
            inputs.forEach(input => {
                input.style.cssText = 'display:block!important;opacity:1!important;position:fixed!important;top:10px!important;left:10px!important;z-index:99999!important;';
            });
            return inputs.length;
        })()
    "#;

    let count: i64 = page.evaluate(js_make_visible)
        .await
        .map(|v| v.into_value().unwrap_or(0))
        .unwrap_or(0);

    if count == 0 {
        bail!("No file input found on the page");
    }

    info!("Found {} file input(s)", count);
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Get the DOM document root
    let doc = page.execute(GetDocumentParams::builder().depth(0).build())
        .await
        .context("Failed to get document")?;

    let root_node_id = doc.result.root.node_id;

    // Query for the file input
    let query = QuerySelectorParams::new(root_node_id, "input[type=\"file\"]");
    let query_result = page.execute(query)
        .await
        .context("Failed to query selector for file input")?;

    let node_id = query_result.result.node_id;

    // Set the file using builder to include node_id
    let mut set_files = SetFileInputFilesParams::new(vec![file_path.to_string()]);
    set_files.node_id = Some(node_id);
    page.execute(set_files)
        .await
        .context("Failed to set file via CDP")?;

    info!("File set successfully: {}", file_path);

    // Trigger change event
    let trigger_js = r#"
        (function() {
            const input = document.querySelector('input[type="file"]');
            if (input) {
                input.dispatchEvent(new Event('change', { bubbles: true }));
                return true;
            }
            return false;
        })()
    "#;
    page.evaluate(trigger_js).await.ok();

    Ok(())
}

/// Execute JavaScript and return string result
pub async fn execute_js(page: &Page, script: &str) -> Result<String> {
    let result = page.evaluate(script)
        .await
        .context("Failed to execute JavaScript")?;
    Ok(format!("{:?}", result.value()))
}

/// Wait a specified duration
pub async fn wait(secs: f64) {
    tokio::time::sleep(Duration::from_secs_f64(secs)).await;
}
