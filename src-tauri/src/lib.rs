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
struct UsageSnapshot {
    source: String,
    plan_type: Option<String>,
    fetched_at: u64,
    five_hour: Option<UsageWindow>,
    weekly: Option<UsageWindow>,
    other_windows: Vec<UsageWindow>,
    credits: Option<Credits>,
    ordinary_usage_allowed: Option<bool>,
}

struct CacheState(Mutex<Option<UsageSnapshot>>);

fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn spawn_codex_app_server() -> Result<Child, String> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;

        // npm installs often expose codex.cmd. Let cmd.exe resolve either that or a native codex.exe.
        let mut cmd = Command::new("cmd.exe");
        cmd.args(["/D", "/S", "/C", "codex app-server --listen stdio://"])
            .creation_flags(CREATE_NO_WINDOW)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        return cmd.spawn().map_err(|e| format!("Could not start Codex: {e}"));
    }

    #[cfg(not(target_os = "windows"))]
    {
        let mut cmd = Command::new("codex");
        cmd.args(["app-server", "--listen", "stdio://"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        cmd.spawn().map_err(|e| format!("Could not start Codex: {e}"))
    }
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
        let message = rx
            .recv_timeout(remaining)
            .map_err(|_| "Timed out waiting for Codex app-server".to_string())?;
        if message.get("id").and_then(Value::as_i64) == Some(id) {
            if let Some(error) = message.get("error") {
                return Err(format!("Codex returned an error: {error}"));
            }
            return message
                .get("result")
                .cloned()
                .ok_or_else(|| "Codex response did not contain a result".to_string());
        }
    }
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
    let mut child = spawn_codex_app_server()?;
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

    let result = (|| {
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
            .ok_or("Codex did not return rateLimits")?;

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

        Ok(UsageSnapshot {
            source: "codex app-server account/rateLimits/read".into(),
            plan_type: rate_limits.get("planType").and_then(Value::as_str).map(str::to_owned),
            fetched_at: now_epoch(),
            five_hour,
            weekly,
            other_windows,
            credits,
            ordinary_usage_allowed: response.get("ordinaryUsageAllowed").and_then(Value::as_bool),
        })
    })();

    let _ = child.kill();
    let _ = child.wait();
    result
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
        .invoke_handler(tauri::generate_handler![get_codex_usage, cursor::get_cursor_usage, cursor::connect_cursor, cursor::disconnect_cursor, status_bar::publish_usage_summary, status_bar::get_usage_summary, status_bar::restore_usage_window])
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
