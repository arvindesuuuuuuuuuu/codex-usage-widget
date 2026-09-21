#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // GNOME/Wayland ignores window moves and always-on-top. Use X11 so the
    // minimized bar can sit at the top of the work area like it does on Windows.
    #[cfg(target_os = "linux")]
    if std::env::var_os("GDK_BACKEND").is_none() {
        std::env::set_var("GDK_BACKEND", "x11");
    }
    codex_usage_widget_lib::run();
}
