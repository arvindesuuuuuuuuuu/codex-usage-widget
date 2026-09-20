use serde::{Deserialize, Serialize};
use std::{sync::Mutex, thread, time::Duration};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, State, WebviewWindow};

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub provider: String,
    pub state: String,
    pub message: String,
    pub windows: Vec<Allowance>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Allowance {
    pub label: String,
    pub left_percent: f64,
}

#[derive(Clone, Default, Serialize)]
pub struct Snapshot {
    revision: u64,
    #[serde(flatten)]
    summary: Summary,
}

#[derive(Default)]
pub struct StatusState(Mutex<Snapshot>);

#[tauri::command]
pub fn publish_usage_summary(
    app: AppHandle,
    window: WebviewWindow,
    summary: Summary,
    state: State<'_, StatusState>,
) -> Result<(), String> {
    if window.label() != "main" {
        return Err("Only the main widget can update usage.".into());
    }
    if !matches!(summary.provider.as_str(), "Codex" | "Cursor")
        || !matches!(summary.state.as_str(), "loading" | "ready" | "error")
        || summary.windows.len() > 8
        || summary.message.len() > 4096
        || summary
            .windows
            .iter()
            .any(|item| item.label.len() > 100 || !item.left_percent.is_finite())
    {
        return Err("Invalid status summary.".into());
    }
    let snapshot = {
        let mut snapshot = state.0.lock().map_err(|_| "Status bar unavailable")?;
        snapshot.revision += 1;
        snapshot.summary = summary;
        snapshot.clone()
    };
    app.emit_to("status-bar", "usage-summary", snapshot)
        .map_err(|_| "Could not update status bar.".into())
}

#[tauri::command]
pub fn get_usage_summary(
    window: WebviewWindow,
    state: State<'_, StatusState>,
) -> Result<Snapshot, String> {
    if !matches!(window.label(), "main" | "status-bar") {
        return Err("Unavailable in this window.".into());
    }
    Ok(state
        .0
        .lock()
        .map_err(|_| "Status bar unavailable")?
        .clone())
}

pub fn restore(app: &AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or("The usage window is unavailable.")?;
    window
        .set_skip_taskbar(false)
        .map_err(|_| "Could not restore the taskbar button.")?;
    window
        .unminimize()
        .map_err(|_| "Could not restore the usage window.")?;
    window
        .show()
        .map_err(|_| "Could not show the usage window.")?;
    window
        .set_focus()
        .map_err(|_| "Could not focus the usage window.")?;
    if let Some(bar) = app.get_webview_window("status-bar") {
        let _ = bar.hide();
    }
    Ok(())
}

#[tauri::command]
pub fn restore_usage_window(app: AppHandle, window: WebviewWindow) -> Result<(), String> {
    if !matches!(window.label(), "main" | "status-bar") {
        return Err("Unavailable in this window.".into());
    }
    restore(&app)
}

fn placement(
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    scale: f64,
) -> (PhysicalPosition<i32>, PhysicalSize<u32>) {
    let margin = (8.0 * scale).round() as u32;
    let bar_width = ((500.0 * scale).round() as u32)
        .min(width.saturating_sub(margin * 2))
        .max(1);
    let bar_height = ((64.0 * scale).round() as u32)
        .min(height.saturating_sub(margin * 2))
        .max(1);
    (
        PhysicalPosition::new(
            x + width.saturating_sub(bar_width) as i32 / 2,
            y + margin as i32,
        ),
        PhysicalSize::new(bar_width, bar_height),
    )
}

// Tauri has no portable Minimized event. Observe native state instead of relying
// on resize events, which some platforms do not send when a window is minimized.
pub fn watch(app: AppHandle) {
    thread::spawn(move || {
        let mut shown = false;
        let mut taskbar_hidden = false;
        let mut last_monitor = None;
        loop {
            let Some(main) = app.get_webview_window("main") else {
                break;
            };
            let Some(bar) = app.get_webview_window("status-bar") else {
                break;
            };
            let minimized = main.is_minimized().unwrap_or(false);
            let visible = main.is_visible().unwrap_or(false);
            if !minimized && visible {
                last_monitor = main.current_monitor().ok().flatten().or(last_monitor);
            }
            let should_show = minimized && visible;
            if should_show != taskbar_hidden {
                if main.set_skip_taskbar(should_show).is_ok() {
                    taskbar_hidden = should_show;
                }
            }
            if should_show && !shown {
                if let Some(tray) = app.tray_by_id("usage-tray") {
                    let _ = tray.set_visible(true);
                }
                let monitor = last_monitor
                    .clone()
                    .or_else(|| main.primary_monitor().ok().flatten());
                if let Some(monitor) = monitor {
                    let area = monitor.work_area();
                    let (position, size) = placement(
                        area.position.x,
                        area.position.y,
                        area.size.width,
                        area.size.height,
                        monitor.scale_factor(),
                    );
                    let _ = bar.set_size(size);
                    let _ = bar.set_position(position);
                }
                shown = bar.show().is_ok();
            } else if !should_show && shown {
                let _ = bar.hide();
                shown = false;
            }
            thread::sleep(Duration::from_millis(250));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn centers_in_the_current_monitors_work_area_with_dpi() {
        let (position, size) = placement(-2560, 40, 2560, 1400, 1.5);
        assert_eq!(size, PhysicalSize::new(750, 96));
        assert_eq!(position, PhysicalPosition::new(-1655, 52));
    }
    #[test]
    fn fits_narrow_screens() {
        let (position, size) = placement(0, 0, 320, 240, 1.0);
        assert_eq!(position.x, 8);
        assert_eq!(size.width, 304);
        assert!(position.y + size.height as i32 <= 240);
    }
}
