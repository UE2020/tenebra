use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use std::process::Command;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use log::*;

pub async fn ensure_chrome_running() -> Result<()> {
    if reqwest::get("http://localhost:9222/json").await.is_ok() {
        return Ok(());
    }

    info!("Chrome CDP not reachable, attempting to launch...");

    // Chrome requires a separate data directory to enable remote debugging.
    // Without --user-data-dir, it silently ignores --remote-debugging-port.
    #[cfg(not(target_os = "windows"))]
    let data_dir = dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
        .join("tenebra")
        .join("chrome-profile");
    #[cfg(target_os = "windows")]
    let data_dir = std::path::PathBuf::from("C:\\tenebra\\chrome-profile");

    let _ = std::fs::create_dir_all(&data_dir);
    let data_dir_str = data_dir.to_string_lossy().to_string();

    let args = [
        "--remote-debugging-port=9222",
        "--remote-allow-origins=*",
        "--no-first-run",
        "--no-default-browser-check",
        &format!("--user-data-dir={}", data_dir_str),
    ];

    #[cfg(target_os = "windows")]
    let chrome_paths = [
        "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe",
        "C:\\Program Files (x86)\\Google\\Chrome\\Application\\chrome.exe",
        "C:\\Program Files\\Microsoft\\Edge\\Application\\msedge.exe",
    ];

    #[cfg(target_os = "macos")]
    let chrome_paths = [
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "/Applications/Chromium.app/Contents/MacOS/Chromium",
    ];

    #[cfg(target_os = "linux")]
    let chrome_paths = [
        "google-chrome",
        "google-chrome-stable",
        "chromium",
        "chromium-browser",
    ];

    let mut spawned = false;
    for path in chrome_paths {
        if Command::new(path).args(&args).spawn().is_ok() {
            info!("Spawned Chrome from: {}", path);
            spawned = true;
            break;
        }
    }

    if !spawned {
        anyhow::bail!("Failed to launch Chrome/Chromium executable.");
    }

    // Verify that CDP is actually reachable before returning success.
    // Chrome can take a few seconds to fully start up.
    for attempt in 1..=10 {
        tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
        if reqwest::get("http://localhost:9222/json").await.is_ok() {
            info!("Chrome CDP confirmed reachable on attempt {}", attempt);
            return Ok(());
        }
        info!("CDP not yet reachable, attempt {}/10...", attempt);
    }

    anyhow::bail!("Chrome was launched but CDP on port 9222 never became reachable. \
                    Ensure no other Chrome instance is locking the default profile.")
}

fn is_internal_url(url: &str) -> bool {
    url.is_empty()
        || url.starts_with("chrome://")
        || url.starts_with("chrome-extension://")
        || url.starts_with("devtools://")
        || url.starts_with("about:")
}

pub async fn fetch_tree() -> Result<Value> {
    ensure_chrome_running().await.context("check chrome")?;

    // Step 1: Get the browser-level WebSocket URL from /json/version
    let version: Value = reqwest::get("http://localhost:9222/json/version")
        .await?
        .json()
        .await?;
    let browser_ws = version["webSocketDebuggerUrl"]
        .as_str()
        .context("No webSocketDebuggerUrl in /json/version")?;

    let (mut ws, _) = connect_async(browser_ws)
        .await
        .context("Failed to connect to browser WebSocket")?;

    // Step 2: Discover all targets via the Target domain
    let discover_req = serde_json::json!({
        "id": 1,
        "method": "Target.getTargets"
    });
    ws.send(Message::Text(discover_req.to_string().into())).await?;
    let targets_resp = wait_for_id(&mut ws, 1).await?;

    let targets = targets_resp["result"]["targetInfos"]
        .as_array()
        .context("Target.getTargets returned no targetInfos")?;

    // Step 3: Filter for real user-facing pages
    let candidates: Vec<&Value> = targets
        .iter()
        .filter(|t| t["type"] == "page")
        .filter(|t| !is_internal_url(t["url"].as_str().unwrap_or("")))
        .collect();

    if candidates.is_empty() {
        // Chrome just launched with only chrome://newtab — not an error, just nothing to read yet
        info!("No user-facing page in Chrome yet (only internal pages)");
        return Ok(serde_json::json!({ "nodes": [] }));
    }

    // Use an incrementing ID for all CDP requests on this connection
    let mut next_id: u64 = 2;
    let mut alloc_id = || { let id = next_id; next_id += 1; id };

    // Step 4: Find the VISIBLE tab by checking document.visibilityState on each page.
    // Only one tab is "visible" at a time — this correctly handles tab switching.
    let mut chosen_url = String::new();
    let mut chosen_session: Option<String> = None;

    for candidate in &candidates {
        let tid = candidate["targetId"].as_str().unwrap_or("");
        let url = candidate["url"].as_str().unwrap_or("unknown");

        // Attach to this target
        let attach_id = alloc_id();
        let attach_req = serde_json::json!({
            "id": attach_id,
            "method": "Target.attachToTarget",
            "params": { "targetId": tid, "flatten": true }
        });
        ws.send(Message::Text(attach_req.to_string().into())).await?;
        let attach_resp = wait_for_id(&mut ws, attach_id).await?;

        let session_id = match attach_resp["result"]["sessionId"].as_str() {
            Some(s) => s.to_string(),
            None => continue,
        };

        // Check visibility
        let eval_id = alloc_id();
        let eval_req = serde_json::json!({
            "id": eval_id,
            "method": "Runtime.evaluate",
            "sessionId": session_id,
            "params": { "expression": "document.visibilityState" }
        });
        ws.send(Message::Text(eval_req.to_string().into())).await?;
        let eval_resp = wait_for_id(&mut ws, eval_id).await?;

        let visibility = eval_resp["result"]["result"]["value"]
            .as_str()
            .unwrap_or("hidden");

        if visibility == "visible" {
            info!("Active tab detected: {} ({})", url, tid);
            chosen_url = url.to_string();
            chosen_session = Some(session_id);
            break;
        }

        // Not the active tab — detach
        let detach_id = alloc_id();
        let detach_req = serde_json::json!({
            "id": detach_id,
            "method": "Target.detachFromTarget",
            "params": { "sessionId": session_id }
        });
        ws.send(Message::Text(detach_req.to_string().into())).await?;
        // Fire-and-forget detach
    }

    // Fallback: if no tab reported "visible" (e.g. Chrome is minimized),
    // just use the last candidate (most recently created page).
    let session_id = match chosen_session {
        Some(s) => s,
        None => {
            let fallback = candidates.last().unwrap();
            let tid = fallback["targetId"].as_str().unwrap_or("");
            chosen_url = fallback["url"].as_str().unwrap_or("unknown").to_string();
            warn!("No visible tab found, falling back to: {}", chosen_url);

            let attach_id = alloc_id();
            let attach_req = serde_json::json!({
                "id": attach_id,
                "method": "Target.attachToTarget",
                "params": { "targetId": tid, "flatten": true }
            });
            ws.send(Message::Text(attach_req.to_string().into())).await?;
            let attach_resp = wait_for_id(&mut ws, attach_id).await?;
            attach_resp["result"]["sessionId"]
                .as_str()
                .context("Target.attachToTarget returned no sessionId")?
                .to_string()
        }
    };

    // Step 5: Enable Accessibility domain within the session
    let enable_id = alloc_id();
    let enable_req = serde_json::json!({
        "id": enable_id,
        "method": "Accessibility.enable",
        "sessionId": session_id
    });
    ws.send(Message::Text(enable_req.to_string().into())).await?;
    wait_for_id(&mut ws, enable_id).await?;

    // Step 6: Fetch the full accessibility tree
    let tree_id = alloc_id();
    let tree_req = serde_json::json!({
        "id": tree_id,
        "method": "Accessibility.getFullAXTree",
        "sessionId": session_id
    });
    ws.send(Message::Text(tree_req.to_string().into())).await?;
    let tree_resp = wait_for_id(&mut ws, tree_id).await?;

    // Step 7: Detach cleanly (fire-and-forget)
    let detach_id = alloc_id();
    let detach_req = serde_json::json!({
        "id": detach_id,
        "method": "Target.detachFromTarget",
        "params": { "sessionId": session_id }
    });
    ws.send(Message::Text(detach_req.to_string().into())).await?;

    if let Some(nodes) = tree_resp.get("result").and_then(|r| r.get("nodes")).and_then(|n| n.as_array()) {
        // Aggressively strip to keep payload under the data channel limit.
        // The AI only needs: role + name for semantic content. Structural
        // wrappers (generic divs, spans) without names are pure noise.
        let stripped: Vec<Value> = nodes.iter().filter_map(|node| {
            if node.get("ignored").and_then(|v| v.as_bool()).unwrap_or(false) {
                return None;
            }

            let role = node.get("role")
                .and_then(|r| r.get("value"))
                .and_then(|v| v.as_str())
                .unwrap_or("");

            // Skip structural noise
            if matches!(role, "none" | "InlineTextBox" | "LineBreak" | "generic"
                | "Iframe" | "IframePresentational") {
                return None;
            }

            let name = node.get("name")
                .and_then(|n| n.get("value"))
                .and_then(|v| v.as_str())
                .unwrap_or("");

            let value = node.get("value")
                .and_then(|v| v.get("value"))
                .and_then(|v| v.as_str())
                .unwrap_or("");

            // Skip nodes with no name and no value unless they are semantic landmarks
            if name.is_empty() && value.is_empty() && !matches!(role,
                "heading" | "list" | "listitem" | "table" | "row" | "cell"
                | "navigation" | "main" | "banner" | "contentinfo"
                | "complementary" | "article" | "region" | "form" | "search"
                | "menu" | "menubar" | "separator" | "toolbar" | "tablist"
                | "tab" | "tabpanel" | "dialog" | "alert" | "tree" | "treeitem"
            ) {
                return None;
            }

            let mut slim = serde_json::json!({ "role": role });
            if !name.is_empty() {
                slim["name"] = Value::String(name.to_string());
            }
            if !value.is_empty() {
                slim["value"] = Value::String(value.to_string());
            }
            Some(slim)
        }).collect();

        let payload = serde_json::json!({ "nodes": stripped });
        let payload_size = serde_json::to_string(&payload).map(|s| s.len()).unwrap_or(0);
        info!("A11y tree: {} nodes, {}KB payload from {}", stripped.len(), payload_size / 1024, chosen_url);
        return Ok(payload);
    }

    anyhow::bail!("Accessibility.getFullAXTree returned no nodes for {}", chosen_url)
}

/// Wait for a CDP response matching a specific message ID, skipping async events.
/// Times out after 5 seconds to prevent infinite hangs.
async fn wait_for_id(
    ws: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    id: u64,
) -> Result<Value> {
    use tokio::time::{timeout, Duration};

    let result = timeout(Duration::from_secs(5), async {
        while let Some(msg) = ws.next().await {
            let msg = msg?;
            if let Message::Text(text) = msg {
                let json: Value = serde_json::from_str(&text)?;
                if json["id"] == id {
                    if let Some(err) = json.get("error") {
                        anyhow::bail!("CDP error for request {}: {}", id, err);
                    }
                    return Ok(json);
                }
                // Log what we're skipping for debugging
                if let Some(method) = json.get("method").and_then(|m| m.as_str()) {
                    trace!("Skipped CDP event while waiting for id {}: {}", id, method);
                } else if let Some(other_id) = json.get("id") {
                    warn!("Unexpected CDP response id {} while waiting for {}", other_id, id);
                }
            }
        }
        anyhow::bail!("WebSocket closed before receiving response for id {}", id)
    }).await;

    match result {
        Ok(inner) => inner,
        Err(_) => anyhow::bail!("CDP timed out waiting for response to id {} (5s)", id),
    }
}
