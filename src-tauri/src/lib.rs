// One UI session per process. The resident tray daemon (the separate
// toolshot binary in daemon/) spawns this with a mode argument; the
// process exits when its windows are gone, giving all capture memory
// back to the OS.

mod capture;
mod logging;
mod picker;
mod settings;

use std::sync::atomic::{AtomicUsize, Ordering};
use tauri::Manager;

// Non-zero while a command is between windows (overlay closed, editor not
// built yet) so the last-window-closed exit stays suppressed until the
// next window exists.
pub struct Busy(pub AtomicUsize);

// Holds the busy state for as long as it lives. Create it before closing
// the old window and let it drop once the next window exists (or the
// failure path unwinds), so no code path can forget to clear the flag
// and leave an invisible process behind.
pub struct BusyGuard {
    app: tauri::AppHandle,
}

impl BusyGuard {
    pub fn new(app: &tauri::AppHandle) -> Self {
        app.state::<Busy>().0.fetch_add(1, Ordering::SeqCst);
        BusyGuard { app: app.clone() }
    }
}

impl Drop for BusyGuard {
    fn drop(&mut self) {
        self.app.state::<Busy>().0.fetch_sub(1, Ordering::SeqCst);
    }
}

// The daemon sits next to this binary: in target/ during dev, in the
// bundle's binary dir when installed (Tauri ships it as a sidecar).
pub fn daemon_path() -> Result<std::path::PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let dir = exe.parent().ok_or_else(|| "no exe dir".to_string())?;
    Ok(dir.join(if cfg!(windows) { "toolshot.exe" } else { "toolshot" }))
}

// Launched with no argument (double click, Start Menu, login item):
// hand off to the resident daemon and get out of the way before any
// webview machinery starts. This keeps the Tauri binary as the bundle
// entry point for the platform installers while the daemon stays the
// only resident process.
fn launch_daemon() {
    let daemon = match daemon_path() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("daemon not found: {e}");
            std::process::exit(1);
        }
    };

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let err = std::process::Command::new(&daemon).exec();
        eprintln!("failed to exec {}: {err}", daemon.display());
        std::process::exit(1);
    }
    #[cfg(not(unix))]
    {
        if let Err(e) = std::process::Command::new(&daemon).spawn() {
            eprintln!("failed to start {}: {e}", daemon.display());
            std::process::exit(1);
        }
    }
}

pub fn run() {
    let mode = std::env::args().nth(1).unwrap_or_default();
    if mode.is_empty() {
        launch_daemon();
        return;
    }

    logging::init(&format!("toolshot-ui {mode}"), false);

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .manage(capture::CaptureState::default())
        .manage(capture::ScreenState::default())
        .manage(capture::OverlayMode::default())
        .manage(picker::PickerState::default())
        .manage(Busy(AtomicUsize::new(0)))
        .invoke_handler(tauri::generate_handler![
            capture::check_screen_permission,
            capture::list_windows,
            capture::capture_window,
            capture::get_screen_meta,
            capture::get_screen_rgba,
            capture::get_overlay_mode,
            capture::overlay_ready,
            capture::capture_area,
            capture::cancel_overlay,
            capture::get_capture_png,
            capture::copy_annotated,
            capture::save_annotated,
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
            settings::open_log,
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
                other => {
                    eprintln!("unknown mode: {other}");
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
            if code.is_none() && app.state::<Busy>().0.load(Ordering::SeqCst) > 0 {
                api.prevent_exit();
            }
        }
    });
}
