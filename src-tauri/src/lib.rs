use serde::Serialize;
use serde_json::{json, Value};
mod cursor;
mod cursor_local;
mod status_bar;
use std::{
    io::{BufRead, BufReader, Write},
    process::{Child, Command, Stdio},
    sync::{mpsc, Mutex},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::{
    image::Image,
    menu::{MenuBuilder, MenuItemBuilder},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager, State, WindowEvent,
};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct UsageWindow {
    label: String,
    used_percent: f64,
    left_percent: f64,
    window_duration_mins: Option<u64>,
    resets_at: Option<i64>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Credits {
    has_credits: Option<bool>,
    unlimited: Option<bool>,
    balance: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ResetCredit {
    available_count: u64,
    description: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct UsageSnapshot {
    source: String,
    plan_type: Option<String>,
    fetched_at: u64,
    five_hour: Option<UsageWindow>,
    weekly: Option<UsageWindow>,
    other_windows: Vec<UsageWindow>,
    credits: Option<Credits>,
    ordinary_usage_allowed: Option<bool>,
    reset_credits: Option<ResetCredit>,
    account_id: Option<String>,
}

struct CacheState(Mutex<Option<UsageSnapshot>>);

fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn find_codex_binary() -> std::path::PathBuf {
    if let Some(path) = std::env::var_os("CODEX_PATH") {
        let pb = std::path::PathBuf::from(path);
        if pb.is_file() {
            return pb;
        }
    }

    #[cfg(not(target_os = "windows"))]
    if let Some(home) = std::env::var_os("HOME") {
        let home = std::path::PathBuf::from(home);
        let candidates = [
            home.join(".codex/plugins/.plugin-appserver/codex"),
            home.join(".local/bin/codex"),
            home.join(".cargo/bin/codex"),
            std::path::PathBuf::from("/usr/local/bin/codex"),
            std::path::PathBuf::from("/usr/bin/codex"),
            std::path::PathBuf::from("/snap/bin/codex"),
        ];
        for candidate in candidates {
            if candidate.is_file() {
                return candidate;
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
            let p = std::path::PathBuf::from(local_app_data).join("Programs\\codex\\codex.exe");
            if p.is_file() {
                return p;
            }
        }
        if let Some(app_data) = std::env::var_os("APPDATA") {
            let p = std::path::PathBuf::from(app_data).join("npm\\codex.cmd");
            if p.is_file() {
                return p;
            }
        }
    }

    std::path::PathBuf::from("codex")
}

#[cfg(target_os = "linux")]
fn sync_snap_codex_auth_if_needed() {
    let Some(home) = std::env::var_os("HOME") else { return; };
    let home = std::path::PathBuf::from(home);
    let dot_codex_auth = home.join(".codex/auth.json");
    if !dot_codex_auth.is_file() {
        return;
    }
    let snap_dir = home.join("snap/codex");
    if !snap_dir.is_dir() {
        return;
    }
    if let Ok(entries) = std::fs::read_dir(&snap_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let snap_auth = path.join("auth.json");
                let should_copy = match (dot_codex_auth.metadata(), snap_auth.metadata()) {
                    (Ok(src_meta), Ok(dst_meta)) => {
                        src_meta.modified().unwrap_or(SystemTime::UNIX_EPOCH)
                            > dst_meta.modified().unwrap_or(SystemTime::UNIX_EPOCH)
                    }
                    (Ok(_), Err(_)) => true,
                    _ => false,
                };
                if should_copy {
                    let _ = std::fs::copy(&dot_codex_auth, &snap_auth);
                }
            }
        }
    }
}

fn spawn_codex_app_server() -> Result<(Child, std::sync::Arc<Mutex<String>>), String> {
    #[cfg(target_os = "linux")]
    sync_snap_codex_auth_if_needed();

    let binary = find_codex_binary();
    let binary_str = binary.to_string_lossy().to_string();

    #[cfg(target_os = "windows")]
    let mut cmd = {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;

        let mut cmd = Command::new("cmd.exe");
        cmd.args(["/D", "/S", "/C", &format!("\"{binary_str}\" app-server --listen stdio://")])
            .creation_flags(CREATE_NO_WINDOW);
        cmd
    };

    #[cfg(not(target_os = "windows"))]
    let mut cmd = {
        let mut cmd = Command::new(&binary);
        cmd.args(["app-server", "--listen", "stdio://"]);
        cmd
    };

    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| {
        format!("Could not start Codex ({binary_str}): {e}. Make sure `codex` is installed and on PATH.")
    })?;

    let stderr_buffer = std::sync::Arc::new(Mutex::new(String::new()));
    if let Some(stderr) = child.stderr.take() {
        let buffer_clone = std::sync::Arc::clone(&stderr_buffer);
        thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines().map_while(Result::ok) {
                if let Ok(mut buf) = buffer_clone.lock() {
                    if buf.len() < 4096 {
                        if !buf.is_empty() {
                            buf.push('\n');
                        }
                        buf.push_str(&line);
                    }
                }
            }
        });
    }

    Ok((child, stderr_buffer))
}

fn send_line(stdin: &mut impl Write, value: Value) -> Result<(), String> {
    let line = serde_json::to_string(&value).map_err(|e| e.to_string())?;
    stdin
        .write_all(format!("{line}\n").as_bytes())
        .and_then(|_| stdin.flush())
        .map_err(|e| format!("Failed to talk to Codex: {e}"))
}

fn wait_for_id(rx: &mpsc::Receiver<Value>, id: i64, timeout: Duration) -> Result<Value, String> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            return Err("Timed out waiting for Codex app-server".into());
        }
        let message = match rx.recv_timeout(remaining) {
            Ok(msg) => msg,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                return Err("Timed out waiting for Codex app-server".into());
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err("Codex process terminated unexpectedly".into());
            }
        };
        if message.get("id").and_then(Value::as_i64) == Some(id) {
            if let Some(error) = message.get("error") {
                let err_str = serde_json::to_string(error).unwrap_or_else(|_| error.to_string());
                return Err(err_str);
            }
            return message
                .get("result")
                .cloned()
                .ok_or_else(|| "Codex response did not contain a result".to_string());
        }
    }
}

fn format_codex_error(raw_error: &str, stderr: &str) -> String {
    if raw_error.contains("401 Unauthorized")
        || raw_error.contains("Could not parse your authentication token")
        || raw_error.contains("Please try signing in again")
    {
        return "Codex login expired or invalid. Open your terminal and run `codex` to sign in again.".into();
    }
    if raw_error.contains("429") || raw_error.contains("rate limit") {
        return "Codex rate limit reached. Please wait a bit before refreshing.".into();
    }
    if let Ok(val) = serde_json::from_str::<Value>(raw_error) {
        if let Some(msg) = val.get("message").and_then(Value::as_str) {
            if msg.contains("401 Unauthorized") || msg.contains("Could not parse your authentication token") {
                return "Codex login expired or invalid. Open your terminal and run `codex` to sign in again.".into();
            }
            return msg.to_string();
        }
    }
    if !stderr.is_empty() {
        return format!("{raw_error} (details: {stderr})");
    }
    raw_error.to_string()
}

fn parse_window(value: &Value) -> Option<UsageWindow> {
    let used = value.get("usedPercent")?.as_f64()?;
    let mins = value.get("windowDurationMins").and_then(Value::as_u64);
    let label = match mins {
        Some(300) => "5 hour".to_string(),
        Some(10_080) => "Weekly".to_string(),
        Some(m) if m % 1_440 == 0 => format!("{} day", m / 1_440),
        Some(m) if m % 60 == 0 => format!("{} hour", m / 60),
        Some(m) => format!("{m} min"),
        None => "Usage".to_string(),
    };

    Some(UsageWindow {
        label,
        used_percent: used,
        left_percent: (100.0 - used).clamp(0.0, 100.0),
        window_duration_mins: mins,
        resets_at: value.get("resetsAt").and_then(Value::as_i64),
    })
}

fn fetch_usage() -> Result<UsageSnapshot, String> {
    let (mut child, stderr_buf) = spawn_codex_app_server()?;
    let mut stdin = child.stdin.take().ok_or("Codex stdin unavailable")?;
    let stdout = child.stdout.take().ok_or("Codex stdout unavailable")?;

    let (tx, rx) = mpsc::channel::<Value>();
    thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if let Ok(value) = serde_json::from_str::<Value>(&line) {
                let _ = tx.send(value);
            }
        }
    });

    let result: Result<UsageSnapshot, String> = (|| {
        send_line(
            &mut stdin,
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "clientInfo": {
                        "name": "codex-usage-widget",
                        "title": "Codex Usage Widget",
                        "version": env!("CARGO_PKG_VERSION")
                    },
                    "capabilities": { "experimentalApi": true }
                }
            }),
        )?;
        wait_for_id(&rx, 1, Duration::from_secs(15))?;

        send_line(
            &mut stdin,
            json!({"jsonrpc": "2.0", "method": "initialized"}),
        )?;

        send_line(
            &mut stdin,
            json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "account/rateLimits/read"
            }),
        )?;

        let response = wait_for_id(&rx, 2, Duration::from_secs(20))?;
        let rate_limits = response
            .get("rateLimits")
            .filter(|v| !v.is_null())
            .or_else(|| response.pointer("/rateLimitsByLimitId/codex"))
            .or_else(|| {
                response
                    .get("rateLimitsByLimitId")
                    .and_then(Value::as_object)
                    .and_then(|map| map.values().next())
            })
            .ok_or_else(|| "Codex did not return rateLimits".to_string())?;

        let primary = rate_limits.get("primary").and_then(parse_window);
        let secondary = rate_limits.get("secondary").and_then(parse_window);

        let mut all = Vec::new();
        if let Some(w) = primary { all.push(w); }
        if let Some(w) = secondary { all.push(w); }

        let five_hour = all.iter().find(|w| w.window_duration_mins == Some(300)).cloned();
        let weekly = all.iter().find(|w| w.window_duration_mins == Some(10_080)).cloned();
        let other_windows = all
            .into_iter()
            .filter(|w| !matches!(w.window_duration_mins, Some(300 | 10_080)))
            .collect();

        let credits = rate_limits.get("credits").and_then(|c| {
            if c.is_null() { return None; }
            Some(Credits {
                has_credits: c.get("hasCredits").and_then(Value::as_bool),
                unlimited: c.get("unlimited").and_then(Value::as_bool),
                balance: c.get("balance").and_then(Value::as_str).map(str::to_owned),
            })
        });

        let reset_credits = response
            .pointer("/rateLimitResetCredits/availableCount")
            .and_then(Value::as_u64)
            .filter(|&c| c > 0)
            .map(|available_count| {
                let description = response
                    .pointer("/rateLimitResetCredits/credits/0/title")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                ResetCredit {
                    available_count,
                    description,
                }
            });

        let plan_type = rate_limits
            .get("planType")
            .and_then(Value::as_str)
            .or_else(|| response.get("planType").and_then(Value::as_str))
            .map(str::to_owned);

        let account_id = response
            .get("accountId")
            .and_then(Value::as_str)
            .map(str::to_owned);

        Ok(UsageSnapshot {
            source: "codex app-server account/rateLimits/read".into(),
            plan_type,
            fetched_at: now_epoch(),
            five_hour,
            weekly,
            other_windows,
            credits,
            ordinary_usage_allowed: response.get("ordinaryUsageAllowed").and_then(Value::as_bool),
            reset_credits,
            account_id,
        })
    })();

    let _ = child.kill();
    let _ = child.wait();

    result.map_err(|err| {
        let stderr = stderr_buf.lock().map(|b| b.clone()).unwrap_or_default();
        format_codex_error(&err, &stderr)
    })
}

#[tauri::command]
async fn get_codex_usage(force: bool, state: State<'_, CacheState>) -> Result<UsageSnapshot, String> {
    if !force {
        if let Ok(cache) = state.0.lock() {
            if let Some(snapshot) = cache.as_ref() {
                if now_epoch().saturating_sub(snapshot.fetched_at) < 60 {
                    return Ok(snapshot.clone());
                }
            }
        }
    }

    let snapshot = tauri::async_runtime::spawn_blocking(fetch_usage)
        .await
        .map_err(|_| "The Codex usage reader stopped unexpectedly.".to_string())??;
    if let Ok(mut cache) = state.0.lock() {
        *cache = Some(snapshot.clone());
    }
    Ok(snapshot)
}

pub fn run() {
    tauri::Builder::default()
        .manage(CacheState(Mutex::new(None)))
        .manage(cursor::CursorCache::default())
        .manage(status_bar::StatusState::default())
        .manage(status_bar::CompactState::default())
        .invoke_handler(tauri::generate_handler![
            get_codex_usage,
            cursor::get_cursor_usage,
            cursor::connect_cursor,
            cursor::disconnect_cursor,
            status_bar::publish_usage_summary,
            status_bar::get_usage_summary,
            status_bar::restore_usage_window,
            status_bar::enter_compact_mode,
            status_bar::set_status_bar_click_through,
            status_bar::start_status_bar_dragging
        ])
        .setup(|app| {
            let show = MenuItemBuilder::with_id("show", "Show Code Usage").build(app)?;
            let refresh = MenuItemBuilder::with_id("refresh", "Refresh").build(app)?;
            let quit = MenuItemBuilder::with_id("quit", "Quit").build(app)?;
            let menu = MenuBuilder::new(app)
                .items(&[&show, &refresh, &quit])
                .build()?;

            let icon = Image::from_bytes(include_bytes!("../icons/icon.png"))?;
            TrayIconBuilder::with_id("usage-tray")
                .icon(icon)
                .tooltip("Code Usage — click to open")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "show" => {
                        let _ = status_bar::restore(app);
                    }
                    "refresh" => {
                        let _ = app.emit("request-refresh", ());
                        let _ = status_bar::restore(app);
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let _ = status_bar::restore(tray.app_handle());
                    }
                })
                .build(app)?;

            if let Some(window) = app.get_webview_window("main") {
                let window_for_close = window.clone();
                let handle = app.handle().clone();
                window.on_window_event(move |event| {
                    if let WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = status_bar::hide_to_tray(&handle, &window_for_close);
                    }
                });
            }
            status_bar::hook_native_minimize(app.handle());
            status_bar::watch(app.handle().clone());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Codex Usage Widget");
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_5_hour_and_weekly_windows() {
        let five_hour = parse_window(&json!({
            "usedPercent": 25.0,
            "windowDurationMins": 300,
            "resetsAt": 1793605463
        })).unwrap();
        assert_eq!(five_hour.label, "5 hour");
        assert_eq!(five_hour.used_percent, 25.0);
        assert_eq!(five_hour.left_percent, 75.0);

        let weekly = parse_window(&json!({
            "usedPercent": 80.5,
            "windowDurationMins": 10080,
            "resetsAt": 1793866617
        })).unwrap();
        assert_eq!(weekly.label, "Weekly");
        assert_eq!(weekly.used_percent, 80.5);
        assert_eq!(weekly.left_percent, 19.5);
    }

    #[test]
    fn formats_friendly_auth_and_rate_limit_errors() {
        let auth_err = "failed to fetch codex rate limits: GET https://chatgpt.com/backend-api/wham/usage failed: 401 Unauthorized; body={\"error\":{\"message\":\"Could not parse your authentication token. Please try signing in again.\"}}";
        let formatted = format_codex_error(auth_err, "");
        assert!(formatted.contains("Codex login expired or invalid"));

        let rate_err = "429 Too Many Requests: rate limit exceeded";
        let formatted_rate = format_codex_error(rate_err, "");
        assert!(formatted_rate.contains("Codex rate limit reached"));
    }

    #[test]
    fn codex_binary_resolution_finds_available_path() {
        let bin = find_codex_binary();
        assert!(!bin.as_os_str().is_empty());
    }
}
