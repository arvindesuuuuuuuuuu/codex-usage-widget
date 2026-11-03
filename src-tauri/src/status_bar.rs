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
    #[serde(default)]
    pub plan_type: Option<String>,
    #[serde(default)]
    pub reset_credits_count: Option<u32>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Allowance {
    pub label: String,
    pub left_percent: f64,
    #[serde(default)]
    pub resets_at: Option<i64>,
    #[serde(default)]
    pub subtext: Option<String>,
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

pub const BAR_HORIZONTAL_WIDTH: f64 = 420.0;
pub const BAR_HORIZONTAL_HEIGHT: f64 = 36.0;
pub const BAR_VERTICAL_WIDTH: f64 = 144.0;
pub const BAR_VERTICAL_HEIGHT: f64 = 160.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BarPosition {
    Top,
    Bottom,
    Left,
    Right,
}

impl BarPosition {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "bottom" => BarPosition::Bottom,
            "left" => BarPosition::Left,
            "right" => BarPosition::Right,
            _ => BarPosition::Top,
        }
    }

    pub fn is_vertical(&self) -> bool {
        matches!(self, BarPosition::Left | BarPosition::Right)
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            BarPosition::Top => "top",
            BarPosition::Bottom => "bottom",
            BarPosition::Left => "left",
            BarPosition::Right => "right",
        }
    }

    pub fn dimensions(&self) -> (f64, f64) {
        if self.is_vertical() {
            (BAR_VERTICAL_WIDTH, BAR_VERTICAL_HEIGHT)
        } else {
            (BAR_HORIZONTAL_WIDTH, BAR_HORIZONTAL_HEIGHT)
        }
    }
}

static CURRENT_POSITION: Mutex<Option<BarPosition>> = Mutex::new(None);

pub fn get_current_position() -> BarPosition {
    CURRENT_POSITION
        .lock()
        .ok()
        .and_then(|guard| *guard)
        .unwrap_or(BarPosition::Top)
}

pub fn set_current_position(pos: BarPosition) {
    if let Ok(mut guard) = CURRENT_POSITION.lock() {
        *guard = Some(pos);
    }
}

fn status_bar_rect(
    app: &AppHandle,
    main: Option<&WebviewWindow>,
    pos: BarPosition,
) -> Option<(PhysicalPosition<i32>, LogicalSize<f64>)> {
    let monitor = main
        .and_then(|w| w.current_monitor().ok().flatten())
        .or_else(|| {
            app.get_webview_window("status-bar")
                .and_then(|b| b.current_monitor().ok().flatten())
        })
        .or_else(|| main.and_then(|w| w.primary_monitor().ok().flatten()))
        .or_else(|| app.primary_monitor().ok().flatten())?;
    let area = monitor.work_area();
    Some(placement_for(
        area.position.x,
        area.position.y,
        area.size.width,
        area.size.height,
        monitor.scale_factor(),
        pos,
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

fn place_status_bar(app: &AppHandle, bar: &WebviewWindow, main: Option<&WebviewWindow>) {
    let pos = get_current_position();
    if let Some((position, size)) = status_bar_rect(app, main, pos) {
        #[cfg(target_os = "linux")]
        {
            if let Ok(win) = bar.gtk_window() {
                use gtk::prelude::*;
                let w = size.width.round() as i32;
                let h = size.height.round() as i32;
                pin_linux_bar_window(&win, w, h);
                win.move_(position.x, position.y);
            }
        }
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
    let pos = get_current_position();
    let rect = status_bar_rect(app, Some(&main), pos);
    let _ = main.set_skip_taskbar(true);
    if let Some((position, size)) = rect {
        #[cfg(target_os = "linux")]
        {
            if let Ok(win) = bar.gtk_window() {
                use gtk::prelude::*;
                let w = size.width.round() as i32;
                let h = size.height.round() as i32;
                pin_linux_bar_window(&win, w, h);
                win.move_(position.x, position.y);
            }
        }
        apply_bar_rect(&bar, position, size);
    }
    bar.show()
        .map_err(|_| "Could not show the status bar.".to_string())?;
    let _ = bar.unminimize();
    if let Some((position, size)) = rect {
        #[cfg(target_os = "linux")]
        {
            if let Ok(win) = bar.gtk_window() {
                use gtk::prelude::*;
                let w = size.width.round() as i32;
                let h = size.height.round() as i32;
                pin_linux_bar_window(&win, w, h);
                win.move_(position.x, position.y);
            }
        }
        apply_bar_rect(&bar, position, size);
    }
    Ok(())
}

#[tauri::command]
pub fn enter_compact_mode(app: AppHandle, window: WebviewWindow) -> Result<(), String> {
    if window.label() != "main" {
        return Err("Unavailable in this window.".into());
    }
    if let Some(main) = app.get_webview_window("main") {
        let _ = main.hide();
    }
    enter_compact(&app)
}

#[tauri::command]
pub fn restore_usage_window(app: AppHandle, window: WebviewWindow) -> Result<(), String> {
    if !matches!(window.label(), "main" | "status-bar") {
        return Err("Unavailable in this window.".into());
    }
    restore(&app)
}

#[cfg(target_os = "linux")]
static CLICK_THROUGH: AtomicBool = AtomicBool::new(false);

#[cfg(target_os = "linux")]
pub fn apply_linux_click_through(win: &gtk::ApplicationWindow, ignore: bool) {
    use gtk::prelude::*;
    if ignore {
        let empty_region = gtk::cairo::Region::create();
        win.input_shape_combine_region(Some(&empty_region));
    } else {
        win.input_shape_combine_region(None);
    }
}

#[tauri::command]
pub fn set_status_bar_click_through(app: AppHandle, ignore: bool) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        CLICK_THROUGH.store(ignore, Ordering::SeqCst);
        if let Some(bar) = app.get_webview_window("status-bar") {
            if let Ok(win) = bar.gtk_window() {
                apply_linux_click_through(&win, ignore);
            }
        }
        return Ok(());
    }

    #[cfg(not(target_os = "linux"))]
    {
        if let Some(bar) = app.get_webview_window("status-bar") {
            let _ = bar.set_ignore_cursor_events(ignore);
        }
        Ok(())
    }
}

#[tauri::command]
pub fn start_status_bar_dragging(app: AppHandle) -> Result<(), String> {
    if let Some(bar) = app.get_webview_window("status-bar") {
        #[cfg(target_os = "linux")]
        {
            use gtk::prelude::*;
            if let Ok(win) = bar.gtk_window() {
                if !win.is_realized() || !win.is_visible() {
                    return Ok(());
                }
            }
        }
        let _ = bar.start_dragging();
    }
    Ok(())
}

#[tauri::command]
pub fn set_status_bar_position(app: AppHandle, position: String) -> Result<(), String> {
    let pos = BarPosition::parse(&position);
    set_current_position(pos);

    if let Some(bar) = app.get_webview_window("status-bar") {
        let main = app.get_webview_window("main");
        if let Some((pos_pt, size)) = status_bar_rect(&app, main.as_ref(), pos) {
            #[cfg(target_os = "linux")]
            {
                if let Ok(win) = bar.gtk_window() {
                    use gtk::prelude::*;
                    let was_visible = win.is_visible();
                    if was_visible {
                        win.hide();
                    }
                    let w = size.width.round() as i32;
                    let h = size.height.round() as i32;
                    pin_linux_bar_window(&win, w, h);
                    win.move_(pos_pt.x, pos_pt.y);
                    apply_bar_rect(&bar, pos_pt, size);
                    if was_visible {
                        win.show();
                        win.move_(pos_pt.x, pos_pt.y);
                    }
                }
            }
            #[cfg(not(target_os = "linux"))]
            {
                apply_bar_rect(&bar, pos_pt, size);
            }
        }
        let _ = bar.emit("status-bar-position-changed", pos.as_str());
    }
    Ok(())
}

pub fn placement_for(
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    scale: f64,
    pos: BarPosition,
) -> (PhysicalPosition<i32>, LogicalSize<f64>) {
    let scale = scale.max(0.1);
    let margin = 6.0;
    let (bar_w, bar_h) = pos.dimensions();
    let avail_w = (width as f64 / scale) - margin * 2.0;
    let avail_h = (height as f64 / scale) - margin * 2.0;
    let bar_width = bar_w.min(avail_w).max(1.0);
    let bar_height = bar_h.min(avail_h).max(1.0);
    let physical_w = (bar_width * scale).round() as u32;
    let physical_h = (bar_height * scale).round() as u32;
    let phys_margin = (margin * scale).round() as i32;

    let (pos_x, pos_y) = match pos {
        BarPosition::Top => (
            x + (width.saturating_sub(physical_w) as i32) / 2,
            y + phys_margin,
        ),
        BarPosition::Bottom => (
            x + (width.saturating_sub(physical_w) as i32) / 2,
            y + (height.saturating_sub(physical_h) as i32) - phys_margin,
        ),
        BarPosition::Left => (
            x + phys_margin,
            y + (height.saturating_sub(physical_h) as i32) / 2,
        ),
        BarPosition::Right => (
            x + (width.saturating_sub(physical_w) as i32) - phys_margin,
            y + (height.saturating_sub(physical_h) as i32) / 2,
        ),
    };

    (
        PhysicalPosition::new(pos_x, pos_y),
        LogicalSize::new(bar_width, bar_height),
    )
}

#[cfg(test)]
fn placement(
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    scale: f64,
) -> (PhysicalPosition<i32>, LogicalSize<f64>) {
    placement_for(x, y, width, height, scale, BarPosition::Top)
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
    win.set_resizable(true);
    win.set_geometry_hints(
        None::<&gtk::Window>,
        None,
        gtk::gdk::WindowHints::empty(),
    );
    win.set_size_request(width, height);
    win.set_default_size(width, height);
    clamp_linux_widget_tree(win.upcast_ref(), width, height);
    win.resize(width, height);
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
            gtk::gdk::Gravity::NorthWest,
        )),
        gtk::gdk::WindowHints::MIN_SIZE
            | gtk::gdk::WindowHints::MAX_SIZE
            | gtk::gdk::WindowHints::BASE_SIZE,
    );
    win.set_resizable(false);
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
            let pos = get_current_position();
            let (w, h) = pos.dimensions();
            let width = w.round() as i32;
            let height = h.round() as i32;
            pin_linux_bar_window(&bar_win, width, height);
            bar_win.connect_map(move |win| {
                let pos = get_current_position();
                let (w, h) = pos.dimensions();
                pin_linux_bar_window(win, w.round() as i32, h.round() as i32);
                apply_linux_click_through(win, CLICK_THROUGH.load(Ordering::SeqCst));
            });
            bar_win.add_events(
                gtk::gdk::EventMask::ENTER_NOTIFY_MASK
                    | gtk::gdk::EventMask::LEAVE_NOTIFY_MASK
                    | gtk::gdk::EventMask::POINTER_MOTION_MASK,
            );

            let bar_for_enter = bar.clone();
            bar_win.connect_enter_notify_event(move |_, event| {
                if event.detail() != gtk::gdk::NotifyType::Inferior {
                    let _ = bar_for_enter.emit("status-bar-hover", true);
                }
                gtk::glib::Propagation::Proceed
            });

            let bar_for_leave = bar.clone();
            bar_win.connect_leave_notify_event(move |_, event| {
                if event.detail() != gtk::gdk::NotifyType::Inferior {
                    let _ = bar_for_leave.emit("status-bar-hover", false);
                    let _ = bar_for_leave.emit("status-bar-leave", ());
                }
                gtk::glib::Propagation::Proceed
            });

            let bar_for_motion = bar.clone();
            bar_win.connect_motion_notify_event(move |_, _| {
                let _ = bar_for_motion.emit("status-bar-hover", true);
                gtk::glib::Propagation::Proceed
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
                place_status_bar(&app, &bar, Some(&main));
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
        assert_eq!(size, LogicalSize::new(420.0, 36.0));
        assert_eq!(position, PhysicalPosition::new(-1595, 49));
    }
    #[test]
    fn uses_the_same_logical_size_as_windows() {
        let (_, size) = placement(0, 0, 1920, 1080, 1.0);
        assert_eq!(size, LogicalSize::new(420.0, 36.0));
        let (_, scaled) = placement(0, 0, 3840, 2160, 2.0);
        assert_eq!(scaled, LogicalSize::new(420.0, 36.0));
    }
    #[test]
    fn fits_narrow_screens() {
        let (position, size) = placement(0, 0, 320, 240, 1.0);
        assert_eq!(position.x, 6);
        assert_eq!(size.width, 308.0);
        assert_eq!(size.height, 36.0);
        assert!(position.y + (size.height as i32) <= 240);
    }
    #[test]
    fn places_bottom_bar_correctly() {
        let (position, size) = placement_for(0, 0, 1920, 1080, 1.0, BarPosition::Bottom);
        assert_eq!(size, LogicalSize::new(420.0, 36.0));
        assert_eq!(position.x, (1920 - 420) / 2);
        assert_eq!(position.y, 1080 - 36 - 6);
    }
    #[test]
    fn places_left_vertical_dock_correctly() {
        let (position, size) = placement_for(0, 0, 1920, 1080, 1.0, BarPosition::Left);
        assert_eq!(size, LogicalSize::new(144.0, 160.0));
        assert_eq!(position.x, 6);
        assert_eq!(position.y, (1080 - 160) / 2);
    }
    #[test]
    fn places_right_vertical_dock_correctly() {
        let (position, size) = placement_for(0, 0, 1920, 1080, 1.0, BarPosition::Right);
        assert_eq!(size, LogicalSize::new(144.0, 160.0));
        assert_eq!(position.x, 1920 - 144 - 6);
        assert_eq!(position.y, (1080 - 160) / 2);
    }
    #[test]
    fn shows_bar_from_compact_or_minimize_but_not_while_restoring() {
        assert!(wants_status_bar(true, false, false));
        assert!(wants_status_bar(false, true, false));
        assert!(!wants_status_bar(false, false, false));
        assert!(!wants_status_bar(true, true, true));
    }
}
