mod capture;
mod tray;

pub fn run() {
    let app = tauri::Builder::default()
        .manage(capture::CaptureState::default())
        .invoke_handler(tauri::generate_handler![
            capture::check_screen_permission,
            capture::list_windows,
            capture::capture_window,
            capture::cancel_overlay,
            capture::get_capture_png,
            capture::copy_capture,
            capture::close_editor,
        ])
        .setup(|app| {
            // Tray-only app, no dock icon.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            tray::create_tray(app.handle())?;
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building toolshot");

    app.run(|_app, event| {
        // Keep running from the tray after the last window closes.
        if let tauri::RunEvent::ExitRequested { api, code, .. } = event {
            if code.is_none() {
                api.prevent_exit();
            }
        }
    });
}
