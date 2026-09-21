use serde::{Deserialize, Serialize};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    thread,
    time::{Duration, Instant},
};
use tauri::{
    AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, State, WebviewWindow,
};

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

#[derive(Default)]
pub struct CompactState {
    compact: AtomicBool,
    restored_at: Mutex<Option<Instant>>,
}

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

fn mark_restored(app: &AppHandle) {
    let state = app.state::<CompactState>();
    state.compact.store(false, Ordering::SeqCst);
    if let Ok(mut restored_at) = state.restored_at.lock() {
        *restored_at = Some(Instant::now());
    };
}

fn is_restoring(state: &CompactState) -> bool {
    state
        .restored_at
        .lock()
        .ok()
        .and_then(|restored_at| *restored_at)
        .is_some_and(|at| at.elapsed() < Duration::from_millis(750))
}

fn wants_status_bar(compact: bool, minimized: bool, restoring: bool) -> bool {
    !restoring && (compact || minimized)
}

pub fn restore(app: &AppHandle) -> Result<(), String> {
    mark_restored(app);
    let window = app
        .get_webview_window("main")
        .ok_or("The usage window is unavailable.")?;
    window
        .set_skip_taskbar(false)
        .map_err(|_| "Could not restore the taskbar button.")?;
    // GNOME ignores deiconify unless the window is withdrawn first.
    let _ = window.hide();
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

pub fn hide_to_tray(app: &AppHandle, window: &WebviewWindow) -> Result<(), String> {
    mark_restored(app);
    if let Some(bar) = app.get_webview_window("status-bar") {
        let _ = bar.hide();
    }
    let _ = window.set_skip_taskbar(false);
    window
        .hide()
        .map_err(|_| "Could not hide the usage window.".into())
}

const BAR_LOGICAL_WIDTH: f64 = 500.0;
const BAR_LOGICAL_HEIGHT: f64 = 64.0;

fn status_bar_rect(
    app: &AppHandle,
    main: &WebviewWindow,
) -> Option<(PhysicalPosition<i32>, LogicalSize<f64>)> {
    let monitor = main
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| main.primary_monitor().ok().flatten())
        .or_else(|| app.primary_monitor().ok().flatten())?;
    let area = monitor.work_area();
    Some(placement(
        area.position.x,
        area.position.y,
        area.size.width,
        area.size.height,
        monitor.scale_factor(),
    ))
}

fn apply_bar_rect(bar: &WebviewWindow, position: PhysicalPosition<i32>, size: LogicalSize<f64>) {
    let _ = bar.set_min_size(Some(size));
    let _ = bar.set_max_size(Some(size));
    let _ = bar.set_size(size);
    let _ = bar.set_position(position);
    let _ = bar.set_always_on_top(true);
    let _ = bar.set_skip_taskbar(true);
}

fn place_status_bar(app: &AppHandle, bar: &WebviewWindow, main: &WebviewWindow) {
    if let Some((position, size)) = status_bar_rect(app, main) {
        apply_bar_rect(bar, position, size);
    }
}

pub fn enter_compact(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<CompactState>();
    state.compact.store(true, Ordering::SeqCst);
    let main = app
        .get_webview_window("main")
        .ok_or("The usage window is unavailable.")?;
    let bar = app
        .get_webview_window("status-bar")
        .ok_or("The status bar is unavailable.")?;
    if let Some(tray) = app.tray_by_id("usage-tray") {
        let _ = tray.set_visible(true);
    }
    let rect = status_bar_rect(app, &main);
    let _ = main.set_skip_taskbar(true);
    if let Some((position, size)) = rect {
        apply_bar_rect(&bar, position, size);
    }
    bar.show()
        .map_err(|_| "Could not show the status bar.".to_string())?;
    let _ = bar.unminimize();
    if let Some((position, size)) = rect {
        apply_bar_rect(&bar, position, size);
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
) -> (PhysicalPosition<i32>, LogicalSize<f64>) {
    let scale = scale.max(0.1);
    let margin = 8.0;
    let avail_w = (width as f64 / scale) - margin * 2.0;
    let avail_h = (height as f64 / scale) - margin * 2.0;
    let bar_width = BAR_LOGICAL_WIDTH.min(avail_w).max(1.0);
    let bar_height = BAR_LOGICAL_HEIGHT.min(avail_h).max(1.0);
    let physical_width = (bar_width * scale).round() as u32;
    (
        PhysicalPosition::new(
            x + width.saturating_sub(physical_width) as i32 / 2,
            y + (margin * scale).round() as i32,
        ),
        LogicalSize::new(bar_width, bar_height),
    )
}

#[cfg(target_os = "linux")]
fn clamp_linux_widget_tree(widget: &gtk::Widget, width: i32, height: i32) {
    use gtk::prelude::*;
    widget.set_size_request(width, height);
    widget.set_hexpand(false);
    widget.set_vexpand(false);
    widget.set_hexpand_set(true);
    widget.set_vexpand_set(true);
    if let Ok(gtk_box) = widget.clone().downcast::<gtk::Box>() {
        gtk_box.set_spacing(0);
        gtk_box.set_border_width(0);
        for child in gtk_box.children() {
            gtk_box.set_child_packing(&child, false, true, 0, gtk::PackType::Start);
        }
    }
    if let Ok(container) = widget.clone().downcast::<gtk::Container>() {
        container.set_border_width(0);
        for child in container.children() {
            clamp_linux_widget_tree(&child, width, height);
        }
    }
}

/// WebKitGTK reports a large natural minimum size, so GTK grows the 64px bar.
/// Force the window and webview down to the same logical size Windows uses.
#[cfg(target_os = "linux")]
fn pin_linux_bar_window(win: &gtk::ApplicationWindow, width: i32, height: i32) {
    use gtk::prelude::*;
    win.set_resizable(false);
    win.set_size_request(width, height);
    win.set_default_size(width, height);
    win.set_geometry_hints(
        None::<&gtk::Window>,
        Some(&gtk::gdk::Geometry::new(
            width,
            height,
            width,
            height,
            width,
            height,
            0,
            0,
            0.0,
            0.0,
            gtk::gdk::Gravity::North,
        )),
        gtk::gdk::WindowHints::MIN_SIZE
            | gtk::gdk::WindowHints::MAX_SIZE
            | gtk::gdk::WindowHints::BASE_SIZE,
    );
    clamp_linux_widget_tree(win.upcast_ref(), width, height);
    win.resize(width, height);
    win.queue_resize();
}

#[cfg(target_os = "linux")]
fn widget_has_minimize_icon(widget: &gtk::Widget) -> bool {
    use gtk::prelude::*;
    if let Ok(image) = widget.clone().downcast::<gtk::Image>() {
        return matches!(
            image.icon_name().as_deref(),
            Some("window-minimize-symbolic" | "window-minimize")
        );
    }
    if let Ok(button) = widget.clone().downcast::<gtk::Button>() {
        if let Some(image) = button.image() {
            if widget_has_minimize_icon(&image) {
                return true;
            }
        }
    }
    widget
        .clone()
        .downcast::<gtk::Container>()
        .ok()
        .is_some_and(|container| {
            container
                .children()
                .iter()
                .any(widget_has_minimize_icon)
        })
}

#[cfg(target_os = "linux")]
fn is_minimize_button(widget: &gtk::Widget) -> bool {
    use gtk::prelude::*;
    widget.clone().downcast::<gtk::Button>().is_ok() && widget_has_minimize_icon(widget)
}

#[cfg(target_os = "linux")]
fn hook_minimize_buttons(widget: &gtk::Widget, app: &AppHandle) -> bool {
    use gtk::prelude::*;
    let mut hooked = false;
    if is_minimize_button(widget) {
        let app = app.clone();
        widget
            .clone()
            .downcast::<gtk::Button>()
            .expect("minimize control")
            .connect_button_release_event(move |_, event| {
                if event.button() == 1 {
                    if let Some(main) = app.get_webview_window("main") {
                        let _ = main.hide();
                    }
                    let _ = enter_compact(&app);
                    return gtk::glib::Propagation::Stop;
                }
                gtk::glib::Propagation::Proceed
            });
        hooked = true;
    }
    if let Ok(container) = widget.clone().downcast::<gtk::Container>() {
        for child in container.children() {
            hooked |= hook_minimize_buttons(&child, app);
        }
    }
    hooked
}

/// GNOME/Wayland never sets ICONIFIED, so polling `is_minimized()` never shows the bar.
/// Intercept the CSD minimize control and open the bar from that click (user gesture).
#[cfg(target_os = "linux")]
pub fn hook_native_minimize(app: &AppHandle) {
    use gtk::prelude::*;
    use std::{cell::Cell, rc::Rc};

    if let Some(bar) = app.get_webview_window("status-bar") {
        if let Ok(bar_win) = bar.gtk_window() {
            bar_win.set_skip_taskbar_hint(true);
            bar_win.set_skip_pager_hint(true);
            bar_win.set_keep_above(true);
            bar_win.set_type_hint(gtk::gdk::WindowTypeHint::Normal);
            bar_win.stick();
            let width = BAR_LOGICAL_WIDTH.round() as i32;
            let height = BAR_LOGICAL_HEIGHT.round() as i32;
            pin_linux_bar_window(&bar_win, width, height);
            bar_win.connect_map(move |win| {
                pin_linux_bar_window(win, width, height);
            });
        }
    }

    let Some(main) = app.get_webview_window("main") else {
        return;
    };
    let Ok(gtk_win) = main.gtk_window() else {
        return;
    };
    let hooked = Rc::new(Cell::new(false));
    let app = app.clone();
    let try_hook = {
        let hooked = hooked.clone();
        let app = app.clone();
        move |win: &gtk::ApplicationWindow| {
            if hooked.get() {
                return;
            }
            if hook_minimize_buttons(win.upcast_ref(), &app) {
                hooked.set(true);
            }
        }
    };
    try_hook(&gtk_win);
    gtk_win.connect_map(move |win| try_hook(win));
}

#[cfg(not(target_os = "linux"))]
pub fn hook_native_minimize(_app: &AppHandle) {}

// Tauri has no portable Minimized event. Observe native state instead of relying
// on resize events, which some platforms do not send when a window is minimized.
pub fn watch(app: AppHandle) {
    thread::spawn(move || {
        let mut shown = false;
        let mut taskbar_hidden = false;
        loop {
            let Some(main) = app.get_webview_window("main") else {
                break;
            };
            let Some(bar) = app.get_webview_window("status-bar") else {
                break;
            };
            let state = app.state::<CompactState>();
            let minimized = main.is_minimized().unwrap_or(false);
            let compact = state.compact.load(Ordering::SeqCst);
            let should_show = wants_status_bar(compact, minimized, is_restoring(&state));
            if should_show && !compact {
                state.compact.store(true, Ordering::SeqCst);
            }
            if should_show != taskbar_hidden {
                if main.set_skip_taskbar(should_show).is_ok() {
                    taskbar_hidden = should_show;
                }
            }
            if should_show && !shown {
                if let Some(tray) = app.tray_by_id("usage-tray") {
                    let _ = tray.set_visible(true);
                }
                place_status_bar(&app, &bar, &main);
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
        assert_eq!(size, LogicalSize::new(500.0, 64.0));
        assert_eq!(position, PhysicalPosition::new(-1655, 52));
    }
    #[test]
    fn uses_the_same_logical_size_as_windows() {
        let (_, size) = placement(0, 0, 1920, 1080, 1.0);
        assert_eq!(size, LogicalSize::new(500.0, 64.0));
        let (_, scaled) = placement(0, 0, 3840, 2160, 2.0);
        assert_eq!(scaled, LogicalSize::new(500.0, 64.0));
    }
    #[test]
    fn fits_narrow_screens() {
        let (position, size) = placement(0, 0, 320, 240, 1.0);
        assert_eq!(position.x, 8);
        assert_eq!(size.width, 304.0);
        assert_eq!(size.height, 64.0);
        assert!(position.y + (size.height as i32) <= 240);
    }
    #[test]
    fn shows_bar_from_compact_or_minimize_but_not_while_restoring() {
        assert!(wants_status_bar(true, false, false));
        assert!(wants_status_bar(false, true, false));
        assert!(!wants_status_bar(false, false, false));
        assert!(!wants_status_bar(true, true, true));
    }
}
