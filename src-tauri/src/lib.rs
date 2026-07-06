// One UI session per process. The resident tray daemon (the separate
// toolshot binary in daemon/) spawns this with a mode argument; the
// process exits when its windows are gone, giving all capture memory
// back to the OS.

mod capture;
mod picker;
mod settings;

use std::sync::atomic::{AtomicBool, Ordering};
use tauri::Manager;

// True while a command is between windows (overlay closed, editor not
// built yet) so the last-window-closed exit stays suppressed until the
// next window exists.
pub struct Busy(pub AtomicBool);

pub fn set_busy(app: &tauri::AppHandle, busy: bool) {
    app.state::<Busy>().0.store(busy, Ordering::SeqCst);
}

pub fn run() {
    let mode = std::env::args().nth(1).unwrap_or_default();

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(capture::CaptureState::default())
        .manage(capture::ScreenState::default())
        .manage(capture::OverlayMode::default())
        .manage(picker::PickerState::default())
        .manage(Busy(AtomicBool::new(false)))
        .invoke_handler(tauri::generate_handler![
            capture::check_screen_permission,
            capture::list_windows,
            capture::capture_window,
            capture::get_screen_meta,
            capture::get_screen_rgba,
            capture::get_overlay_mode,
            capture::capture_area,
            capture::cancel_overlay,
            capture::get_capture_png,
            capture::copy_annotated,
            capture::close_editor,
            picker::pick_color,
            picker::get_picked_color,
            picker::copy_text,
            picker::close_color_popup,
            settings::get_settings,
            settings::set_shortcut,
            settings::get_autostart,
            settings::set_autostart,
            settings::close_settings,
            settings::get_app_version,
            settings::open_url,
        ])
        .setup(move |app| {
            // No dock icon, matching the daemon.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            settings::init(app.handle());

            let handle = app.handle().clone();
            match mode.as_str() {
                "capture" => capture::start_window_pick(&handle),
                "pick" => capture::start_color_pick(&handle),
                "fullscreen" => capture::capture_fullscreen(&handle),
                "settings" => settings::open_settings_window(&handle),
                _ => {
                    eprintln!("usage: toolshot-ui <capture|pick|fullscreen|settings>");
                    std::process::exit(2);
                }
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building toolshot");

    app.run(|app, event| {
        // Exit with the last window unless a command is mid transition
        // (overlay to editor, overlay to color popup).
        if let tauri::RunEvent::ExitRequested { api, code, .. } = event {
            if code.is_none() && app.state::<Busy>().0.load(Ordering::SeqCst) {
                api.prevent_exit();
            }
        }
    });
}
