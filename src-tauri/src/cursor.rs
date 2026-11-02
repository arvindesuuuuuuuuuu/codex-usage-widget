use serde::Serialize;
use serde_json::Value;
use std::{sync::Mutex, time::Duration};
use tauri::{AppHandle, Manager, State, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

const DASHBOARD: &str = "https://cursor.com/dashboard";
const SUMMARY: &str = "https://cursor.com/api/usage-summary";

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CursorUsage {
    auth_source: String,
    fetched_at: u64,
    plan_type: Option<String>,
    resets_at: Option<i64>,
    billing_cycle_start: Option<i64>,
    windows: Vec<super::UsageWindow>,
    on_demand_used_usd: Option<f64>,
    on_demand_limit_usd: Option<f64>,
    total_requests_used: Option<u64>,
    total_requests_limit: Option<u64>,
    total_requests_remaining: Option<u64>,
    bonus_requests: Option<u64>,
    auto_model_display_message: Option<String>,
    named_model_display_message: Option<String>,
}

#[derive(Default)]
pub struct CursorCache(Mutex<Option<(String, CursorUsage)>>);

fn main_only(window: &WebviewWindow) -> Result<(), String> {
    if window.label() != "main" {
        return Err("This action is only available in the usage widget.".into());
    }
    Ok(())
}

// A separate WebView profile keeps this login independent of browser and Codex sessions.
fn login_window(app: &AppHandle) -> Result<WebviewWindow, String> {
    if let Some(window) = app.get_webview_window("cursor-login") {
        return Ok(window);
    }
    let directory = app
        .path()
        .app_local_data_dir()
        .map_err(|_| "Could not locate the Cursor sign-in storage.".to_string())?
        .join("cursor-session");
    let window = WebviewWindowBuilder::new(
        app,
        "cursor-login",
        WebviewUrl::External("about:blank".parse().unwrap()),
    )
    .title("Sign in to Cursor — close this window, then refresh the widget")
    .inner_size(900.0, 720.0)
    .data_directory(directory)
    .visible(false)
    .build()
    .map_err(|_| "Could not open the Cursor sign-in window.".to_string())?;
    let close_window = window.clone();
    window.on_window_event(move |event| {
        if let tauri::WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            let _ = close_window.hide();
        }
    });
    Ok(window)
}

#[tauri::command]
pub async fn connect_cursor(
    app: AppHandle,
    window: WebviewWindow,
    cache: State<'_, CursorCache>,
) -> Result<(), String> {
    main_only(&window)?;
    *cache.0.lock().map_err(|_| "Cursor cache unavailable")? = None;
    let login = login_window(&app)?;
    login
        .navigate(DASHBOARD.parse().unwrap())
        .map_err(|_| "Could not load Cursor sign-in.")?;
    login.show().map_err(|_| "Could not show Cursor sign-in.")?;
    login
        .set_focus()
        .map_err(|_| "Could not focus Cursor sign-in.")?;
    Ok(())
}

#[tauri::command]
pub async fn disconnect_cursor(
    app: AppHandle,
    window: WebviewWindow,
    cache: State<'_, CursorCache>,
) -> Result<(), String> {
    main_only(&window)?;
    *cache.0.lock().map_err(|_| "Cursor cache unavailable")? = None;
    let login = login_window(&app)?;
    login
        .navigate("about:blank".parse().unwrap())
        .map_err(|_| "Could not close Cursor page.")?;
    login
        .clear_all_browsing_data()
        .map_err(|_| "Could not clear the Cursor session.")?;
    let _ = login.hide();
    Ok(())
}

fn number(value: Option<&Value>) -> Option<f64> {
    value
        .and_then(Value::as_f64)
        .filter(|v| v.is_finite() && *v >= 0.0)
}

fn normalize_cursor_percent(raw: f64) -> f64 {
    // Cursor API returns decimal ratios where 1.0 represents 100% (e.g. 0.10666 = 10.67%, 0.36 = 36%).
    // If raw <= 2.0, scale by 100. If already > 2.0 (e.g. 50.0, 110.0), treat as already on percentage scale.
    if raw <= 2.0 {
        raw * 100.0
    } else {
        raw
    }
}

fn parse_summary(value: &Value) -> Result<CursorUsage, String> {
    let plan = value.pointer("/individualUsage/plan");
    let resets_at = value
        .get("billingCycleEnd")
        .and_then(Value::as_str)
        .and_then(|date| OffsetDateTime::parse(date, &Rfc3339).ok())
        .map(|date| date.unix_timestamp());
    let billing_cycle_start = value
        .get("billingCycleStart")
        .and_then(Value::as_str)
        .and_then(|date| OffsetDateTime::parse(date, &Rfc3339).ok())
        .map(|date| date.unix_timestamp());

    let mut windows = Vec::new();
    if let Some(plan) = plan {
        for (field, label) in [
            ("autoPercentUsed", "Cursor models"),
            ("apiPercentUsed", "Other models"),
        ] {
            if let Some(raw_used) = number(plan.get(field)) {
                let used_percent = normalize_cursor_percent(raw_used);
                windows.push(super::UsageWindow {
                    label: label.into(),
                    used_percent,
                    left_percent: (100.0 - used_percent).clamp(0.0, 100.0),
                    window_duration_mins: None,
                    resets_at,
                });
            }
        }
        // Prefer the explicit pools. Never invent an average or assume missing usage is zero.
        if windows.is_empty() {
            let percent = number(plan.get("totalPercentUsed")).map(normalize_cursor_percent).or_else(|| {
                let used = number(plan.get("used"))?;
                let limit = number(plan.get("limit"))?;
                (limit > 0.0).then_some(used / limit * 100.0)
            });
            if let Some(used) = percent.filter(|v| v.is_finite()) {
                windows.push(super::UsageWindow {
                    label: "Included plan".into(),
                    used_percent: used,
                    left_percent: (100.0 - used).clamp(0.0, 100.0),
                    window_duration_mins: None,
                    resets_at,
                });
            }
        }
    }
    let on_demand_used_usd =
        number(value.pointer("/individualUsage/onDemand/used")).map(|v| v / 100.0);
    let on_demand_limit_usd =
        number(value.pointer("/individualUsage/onDemand/limit")).map(|v| v / 100.0);
    if windows.is_empty() && on_demand_used_usd.is_none() {
        return Err("Cursor returned no supported personal usage data. Check your Cursor dashboard; its response format may have changed.".into());
    }

    let total_requests_used = plan.and_then(|p| p.get("used")).and_then(Value::as_u64);
    let total_requests_limit = plan.and_then(|p| p.get("limit")).and_then(Value::as_u64);
    let total_requests_remaining = plan.and_then(|p| p.get("remaining")).and_then(Value::as_u64);
    let bonus_requests = plan.and_then(|p| p.pointer("/breakdown/bonus")).and_then(Value::as_u64);

    let auto_model_display_message = value
        .get("autoModelSelectedDisplayMessage")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let named_model_display_message = value
        .get("namedModelSelectedDisplayMessage")
        .and_then(Value::as_str)
        .map(str::to_owned);

    Ok(CursorUsage {
        auth_source: "web".into(),
        fetched_at: super::now_epoch(),
        plan_type: value
            .get("membershipType")
            .and_then(Value::as_str)
            .map(str::to_owned),
        resets_at,
        billing_cycle_start,
        windows,
        on_demand_used_usd,
        on_demand_limit_usd,
        total_requests_used,
        total_requests_limit,
        total_requests_remaining,
        bonus_requests,
        auto_model_display_message,
        named_model_display_message,
    })
}

fn find_cursor_state_vscdb(app: &AppHandle) -> Result<Option<std::path::PathBuf>, String> {
    if let Some(path) = std::env::var_os("CURSOR_USER_DATA_DIR") {
        let base = std::path::PathBuf::from(path);
        if !base.is_absolute() {
            return Err("CURSOR_USER_DATA_DIR must be an absolute path to Cursor's user-data folder.".into());
        }
        return Ok(Some(base.join("User/globalStorage/state.vscdb")));
    }

    let default_base = app
        .path()
        .config_dir()
        .map_err(|_| "Could not locate Cursor's user data.")?
        .join("Cursor");
    let default_db = default_base.join("User/globalStorage/state.vscdb");
    if default_db.exists() {
        return Ok(Some(default_db));
    }

    #[cfg(target_os = "linux")]
    if let Some(home) = std::env::var_os("HOME") {
        let home = std::path::PathBuf::from(home);
        let candidates = [
            home.join(".config/Cursor/User/globalStorage/state.vscdb"),
            home.join(".var/app/com.cursor.Cursor/config/Cursor/User/globalStorage/state.vscdb"),
            home.join("snap/cursor/current/.config/Cursor/User/globalStorage/state.vscdb"),
        ];
        for candidate in candidates {
            if candidate.exists() {
                return Ok(Some(candidate));
            }
        }
    }

    Ok(Some(default_db))
}

#[tauri::command]
pub async fn get_cursor_usage(
    app: AppHandle,
    window: WebviewWindow,
    force: bool,
    source: Option<String>,
    cache: State<'_, CursorCache>,
) -> Result<CursorUsage, String> {
    main_only(&window)?;
    let source = source.as_deref().unwrap_or("auto");
    if !matches!(source, "auto" | "app" | "web") {
        return Err("Unknown Cursor sign-in source.".into());
    }
    let local_cookie = if source != "web" {
        if let Some(path) = find_cursor_state_vscdb(&app)? {
            super::cursor_local::read_cookie(&path, super::now_epoch())?
        } else {
            None
        }
    } else {
        None
    };
    if source == "app" && local_cookie.is_none() {
        return Err("No signed-in Cursor app was found. Open Cursor and sign in there, then refresh this widget.".into());
    }
    let (cookie, auth_source) = if let Some(cookie) = local_cookie {
        (cookie, "app")
    } else {
        let login = login_window(&app)?;
        let cookie = login
            .cookies_for_url(SUMMARY.parse().unwrap())
            .map_err(|_| {
                "Could not read the widget's Cursor session. Try signing in again.".to_string()
            })?
            .into_iter()
            .filter(|cookie| {
                let name = cookie.name();
                name == "WorkosCursorSessionToken"
                    || name.starts_with("__Secure-next-auth.session-token")
                    || name.starts_with("next-auth.session-token")
                    || name.starts_with("cursor")
                    || name.contains("session")
                    || name.contains("token")
            })
            .map(|cookie| format!("{}={}", cookie.name(), cookie.value()))
            .collect::<Vec<_>>()
            .join("; ");
        if cookie.is_empty() {
            return Err("No Cursor sign-in was found. Sign in to the Cursor app and refresh, or use Web sign-in.".into());
        }
        (cookie, "web")
    };
    if !force {
        if let Some((identity, snapshot)) = cache
            .0
            .lock()
            .map_err(|_| "Cursor cache unavailable")?
            .as_ref()
        {
            if identity == &cookie
                && snapshot.auth_source == auth_source
                && super::now_epoch().saturating_sub(snapshot.fetched_at) < 60
            {
                return Ok(snapshot.clone());
            }
        }
    }
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::none())
        .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Cursor/0.45.0 Chrome/130.0.0.0 Safari/537.36")
        .build()
        .map_err(|_| "Could not initialize the Cursor connection.")?;
    let response = client
        .get(SUMMARY)
        .header(reqwest::header::COOKIE, &cookie)
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(|_| "Could not reach Cursor. Check your connection and try again.")?;
    let status = response.status();
    if status.is_redirection() || matches!(status.as_u16(), 401 | 403) {
        *cache.0.lock().map_err(|_| "Cursor cache unavailable")? = None;
        return Err(if auth_source == "app" {
            "Cursor rejected the app's saved sign-in. Open Cursor to renew it and refresh, or use Web sign-in."
        } else {
            "Cursor web sign-in expired or was rejected. Use Web sign-in again, then refresh."
        }.into());
    }
    if status.as_u16() == 429 {
        return Err("Cursor is limiting requests. Wait a minute before refreshing.".into());
    }
    if !status.is_success() {
        return Err(format!(
            "Cursor usage is temporarily unavailable (HTTP {}).",
            status.as_u16()
        ));
    }
    let value: Value = response
        .json()
        .await
        .map_err(|_| "Cursor returned an unexpected response. Try signing in again.")?;
    let mut snapshot = parse_summary(&value)?;
    snapshot.auth_source = auth_source.into();
    *cache.0.lock().map_err(|_| "Cursor cache unavailable")? = Some((cookie, snapshot.clone()));
    Ok(snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn pools_keep_fractional_percentages_and_billing_reset() {
        let data = parse_summary(&json!({
            "billingCycleEnd": "2026-10-01T00:00:00.000Z", "membershipType": "pro",
            "individualUsage": {"plan": {"autoPercentUsed": 0.36, "apiPercentUsed": 110},
                "onDemand": {"used": 1234, "limit": 5000}}
        }))
        .unwrap();
        assert_eq!(data.windows.len(), 2);
        // 0.36 in Cursor decimal ratio represents 36% used, so 64% left
        assert!((data.windows[0].left_percent - 64.0).abs() < 0.0001);
        assert_eq!(data.windows[1].left_percent, 0.0);
        assert_eq!(data.resets_at, Some(1790812800));
        assert_eq!(data.on_demand_used_usd, Some(12.34));
        assert_eq!(data.on_demand_limit_usd, Some(50.0));
    }

    #[test]
    fn decimal_ratio_is_correctly_scaled() {
        let data = parse_summary(&json!({
            "billingCycleStart": "2026-10-31T11:48:50.000Z",
            "billingCycleEnd": "2026-11-30T11:48:50.000Z",
            "membershipType": "pro",
            "individualUsage": {
                "plan": {
                    "used": 48,
                    "limit": 2000,
                    "remaining": 1952,
                    "autoPercentUsed": 0.10666666666666667,
                    "apiPercentUsed": 0.0
                }
            }
        })).unwrap();
        assert_eq!(data.windows.len(), 2);
        // 0.106666... is ~10.67% used, so ~89.33% left
        assert!((data.windows[0].left_percent - 89.3333).abs() < 0.01);
        assert_eq!(data.windows[1].left_percent, 100.0);
        assert_eq!(data.total_requests_used, Some(48));
        assert_eq!(data.total_requests_limit, Some(2000));
        assert_eq!(data.total_requests_remaining, Some(1952));
    }

    #[test]
    fn missing_or_invalid_values_are_not_zero_usage() {
        for value in [
            json!({}),
            json!({"individualUsage":{"plan":{"limit":0,"used":0}}}),
            json!({"individualUsage":{"plan":{"apiPercentUsed":-1}}}),
            json!({"individualUsage":{"plan":{"apiPercentUsed":"unknown"}}}),
        ] {
            assert!(parse_summary(&value).is_err());
        }
    }

    #[test]
    fn zero_usage_and_legacy_ratio_are_supported() {
        let data =
            parse_summary(&json!({"individualUsage":{"plan":{"apiPercentUsed":0}}})).unwrap();
        assert_eq!(data.windows[0].left_percent, 100.0);
        let data =
            parse_summary(&json!({"individualUsage":{"plan":{"used":25,"limit":100}}})).unwrap();
        assert_eq!(data.windows[0].left_percent, 75.0);
        assert_eq!(data.resets_at, None);
    }
}
